//! Community writeups listing for a specific VM.
//! Ported from WriteupManager (hmv/modules/writeups.py).

use anyhow::{bail, Result};
use console::style;

use crate::modules::session::HmvSession;

pub struct WriteupManager {
    session: HmvSession,
}

pub struct Writeup {
    date: String,
    author: String,
    language: String,
    format: String,
    url: String,
}

impl WriteupManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    pub async fn get_writeups(&self, vm_name: &str) -> Result<()> {
        let sp = spinner(vm_name);
        let html = self
            .session
            .get(&format!("/machines/machine.php?vm={vm_name}"))
            .await?;

        if html.contains("machine not found") {
            sp.finish_and_clear();
            return Err(crate::modules::HmvError::MachineNotFound(vm_name.to_string()).into());
        }

        let writeups = parse_writeups(&html);
        sp.finish_and_clear();

        if writeups.is_empty() {
            bail!("No community writeups found for {vm_name}.");
        }

        print_table(vm_name, &writeups);
        Ok(())
    }
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
