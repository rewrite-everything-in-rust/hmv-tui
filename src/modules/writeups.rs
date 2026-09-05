//! Community writeups for a specific VM: fetch and submission verdicts.
//! Ported from WriteupManager (hmv/modules/writeups.py).

use anyhow::{bail, Result};

use crate::modules::session::HmvSession;

#[derive(Debug, Clone)]
pub struct Writeup {
    pub date: String,
    pub author: String,
    pub language: String,
    pub format: String,
    pub url: String,
}

pub struct WriteupManager {
    session: HmvSession,
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

    /// Network-only writeup submission; verdicts are returned, not rendered,
    /// so the TUI presents them in its result popups. (The server only
    /// accepts this after both user and root flags were submitted for the VM.)
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
