//! Community writeups listing for a specific VM.
//! Ported from WriteupManager (hmv/modules/writeups.py).

use anyhow::{bail, Result};
use console::style;

use crate::modules::session::HmvSession;

pub struct WriteupManager {
    session: HmvSession,
}

#[derive(Debug, Clone)]
pub struct Writeup {
    pub date: String,
    pub author: String,
    pub language: String,
    pub format: String,
    pub url: String,
}

impl WriteupManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    /// Network-only fetch of the community writeups for a VM. Empty list =
    /// the machine exists but has no accepted writeups. Callers render.
    pub async fn fetch(&self, vm_name: &str) -> Result<Vec<Writeup>> {
        let html = self
            .session
            .get(&format!("/machines/machine.php?vm={vm_name}"))
            .await?;

        if machine_missing(&html) {
            return Err(crate::modules::HmvError::MachineNotFound(vm_name.to_string()).into());
        }

        Ok(parse_writeups(&html))
    }

    /// CLI wrapper around [`Self::fetch`] with spinner and printed table.
    pub async fn get_writeups(&self, vm_name: &str) -> Result<()> {
        let sp = spinner(vm_name);
        let writeups = match self.fetch(vm_name).await {
            Ok(writeups) => writeups,
            Err(error) => {
                sp.finish_and_clear();
                return Err(error);
            }
        };
        sp.finish_and_clear();

        if writeups.is_empty() {
            bail!("No community writeups found for {vm_name}.");
        }

        print_table(vm_name, &writeups);
        Ok(())
    }

    /// Network-only writeup submission; verdicts are returned, not printed,
    /// so both the CLI and the TUI can render them their own way. (The
    /// server only accepts this after both user and root flags were
    /// submitted for the VM.)
    pub async fn submit(&self, vm_name: &str, url: &str) -> Result<UploadVerdict> {
        let normalized = url.trim();
        if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
            bail!("Error: The writeup URL must start with http:// or https://.");
        }

        // Resolve the canonical VM name first: the API rejects mismatched
        // casing ("liar" fails where "Liar" works), and the machine page's
        // hidden form field carries the exact spelling.
        let page = self
            .session
            .get(&format!("/machines/machine.php?vm={vm_name}"))
            .await?;
        if machine_missing(&page) {
            return Ok(UploadVerdict::NotFound);
        }
        let canonical = extract_hidden_vm(&page).unwrap_or_else(|| vm_name.to_string());

        let body = self
            .session
            .post_form(
                "/machines/checkwriteup.php",
                &[("writeup", normalized), ("vm", canonical.as_str())],
            )
            .await?;

        let msg = body.to_lowercase();
        Ok(if msg.contains("submitted the writeup successfully") || msg.contains("correct") {
            UploadVerdict::Submitted
        } else if msg.contains("repeated writeup") {
            UploadVerdict::Repeated
        } else if msg.contains("something went wrong") {
            UploadVerdict::Rejected
        } else if msg.contains("not found") {
            UploadVerdict::NotFound
        } else {
            UploadVerdict::Unknown(body.trim().to_string())
        })
    }

    /// CLI wrapper around [`Self::submit`] with spinner and printed verdicts.
    pub async fn upload(&self, vm_name: &str, url: &str) -> Result<()> {
        let sp = crate::ui::spinner(format!("Submitting writeup for {vm_name}..."));
        let verdict = match self.submit(vm_name, url).await {
            Ok(verdict) => verdict,
            Err(error) => {
                sp.finish_and_clear();
                return Err(error);
            }
        };
        sp.finish_and_clear();

        match verdict {
            UploadVerdict::Submitted => {
                println!(
                    "{} Writeup submitted for {}!",
                    style("[✓]").green().bold(),
                    style(vm_name).white().bold()
                );
                println!("{} Link: {}", style("[*]").blue(), style(url.trim()).cyan());
            }
            UploadVerdict::Repeated => {
                println!(
                    "{} A writeup for {} was already submitted.",
                    style("[!]").yellow().bold(),
                    style(vm_name).white().bold()
                );
            }
            UploadVerdict::Rejected => {
                bail!(
                    "Error: The server rejected the writeup for '{vm_name}'. \
                     Check that both user and root flags were submitted."
                );
            }
            UploadVerdict::NotFound => {
                bail!("Error: Machine '{vm_name}' was not found.");
            }
            UploadVerdict::Unknown(body) => {
                println!(
                    "{} Unknown server response: {}",
                    style("[?]").yellow().bold(),
                    body
                );
            }
        }
        Ok(())
    }
}

/// Server verdict for a writeup submission.
#[derive(Debug, Clone)]
pub enum UploadVerdict {
    Submitted,
    Repeated,
    Rejected,
    NotFound,
    Unknown(String),
}

/// Extracts the canonical VM name from the submission form's hidden field.
fn extract_hidden_vm(html: &str) -> Option<String> {
    let marker = html.find("name=\"vm\" type=\"hidden\"")?;
    let rest = &html[marker..];
    let start = rest.find("value=\"")? + "value=\"".len();
    let end = rest[start..].find('"')? + start;
    Some(rest[start..end].to_string())
}

/// The server renders a tiny error page for unknown VMs.
fn machine_missing(html: &str) -> bool {
    html.to_lowercase().contains("machine doesnt exist")
        || html.to_lowercase().contains("machine not found")
}

fn spinner(vm_name: &str) -> indicatif::ProgressBar {
    crate::ui::spinner(format!("Fetching writeup list for {vm_name}..."))
}

/// Pure parsing function so it can be unit-tested against fixture HTML.
pub fn parse_writeups(html: &str) -> Vec<Writeup> {
    let doc = scraper::Html::parse_document(html);

    let row_sel = scraper::Selector::parse("table.table-striped tbody tr").unwrap();
    let date_sel = scraper::Selector::parse("th[scope=row]").unwrap();
    let author_sel = scraper::Selector::parse("a.creator").unwrap();
    let link_sel = scraper::Selector::parse("a.download").unwrap();
    let lang_sel = scraper::Selector::parse("span.size").unwrap();

    let mut writeups = Vec::new();
    for row in doc.select(&row_sel) {
        let date = row
            .select(&date_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "N/A".into());
        let author = row
            .select(&author_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Unknown".into());

        let Some(link) = row.select(&link_sel).next() else {
            continue;
        };
        let url = link.value().attr("href").unwrap_or("").to_string();
        let format = link
            .text()
            .collect::<String>()
            .trim()
            .replace('!', "");
        let language = row
            .select(&lang_sel)
            .next()
            .map(|n| n.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Unknown".into());

        writeups.push(Writeup {
            date,
            author,
            language,
            format,
            url,
        });
    }
    writeups
}

fn print_table(vm_name: &str, writeups: &[Writeup]) {
    use comfy_table::{presets::NOTHING, Table};

    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_header(vec!["Date", "Author (Poet)", "Language", "Format", "Link"]);
    for column in table.column_iter_mut() {
        column.set_padding((0, 2));
    }

    for w in writeups {
        let lang_str = if w.language.contains("English") {
            style(&w.language).green().to_string()
        } else {
            style(&w.language).yellow().to_string()
        };
        let format_str = if w.format.contains("Read") {
            style(w.format.to_uppercase()).cyan().to_string()
        } else {
            style(w.format.to_uppercase()).magenta().to_string()
        };

        table.add_row(vec![
            style(&w.date).dim().to_string(),
            style(&w.author).white().bold().to_string(),
            lang_str,
            format_str,
            style(&w.url).blue().to_string(),
        ]);
    }

    println!(
        "\n{}\n",
        style(format!("Community Writeups: {vm_name}"))
            .magenta()
            .bold()
    );
    println!("{table}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_canonical_vm_from_hidden_field() {
        let html = r#"<form action="checkwriteup.php" method="post"><input name="vm" type="hidden" value="Fuxa" /></form>"#;
        assert_eq!(extract_hidden_vm(html).as_deref(), Some("Fuxa"));
        assert_eq!(extract_hidden_vm("<html></html>"), None);
    }
}
