//! `hmv machine` orchestration: listing, filtering, search, pagination and
//! pwned-status sync. Ported from the `machine` command body in hmv/main.py.

use anyhow::Result;
use console::style;
use futures_util::stream;
use futures_util::StreamExt;

use crate::modules::machines::{Machine, MachineScraper, Page};
use crate::ui::spinner;

const PER_PAGE: usize = 20;
const CONCURRENCY: usize = 3;
const DIFFICULTIES: [&str; 3] = ["beginner", "intermediate", "advanced"];
const CATEGORIES_NEEDING_ALL: [&str; 7] = [
    "beginner",
    "intermediate",
    "advanced",
    "all",
    "size",
    "linux",
    "windows",
];

/// Parses "123.4 MB" -> 123.4 (0.0 on failure), mirroring parse_size in main.py.
fn parse_size(s: &str) -> f64 {
    s.split_whitespace()
        .next()
        .and_then(|n| n.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn total_pages_of(pages_info: &str) -> usize {
    pages_info
        .split('/')
        .next_back()
        .and_then(|t| t.trim().parse::<usize>().ok())
        .unwrap_or(1)
}

/// Concurrently fetches pages `2..=total`, max 3 in flight (asyncio.gather +
/// Semaphore(3) equivalent). Results stay in page order.
async fn fetch_remaining_pages(
    scraper: &MachineScraper,
    level: Option<&str>,
    total: usize,
) -> Result<Vec<Machine>> {
    let pages: Vec<anyhow::Result<Page>> = stream::iter(2..=total)
        .map(|p| scraper.get_machines(p, level))
        .buffered(CONCURRENCY)
        .collect()
        .await;

    let mut out = Vec::new();
    for page in pages {
        out.extend(page?.machines);
    }
    Ok(out)
}

/// Fetches the complete catalog for a level, deduplicated by lowercase name.
pub async fn fetch_catalog(scraper: &MachineScraper, level: &str) -> Result<Vec<Machine>> {
    let first = scraper.get_machines(1, Some(level)).await?;
    let total = total_pages_of(&first.pages_info);
    let mut machines = first.machines;
    if total > 1 {
        machines.extend(fetch_remaining_pages(scraper, Some(level), total).await?);
    }

    let mut seen = std::collections::HashSet::new();
    machines.retain(|m| seen.insert(m.name.trim().to_lowercase()));
    Ok(machines)
}

/// Fetches the full "hacked" catalog and overwrites matching statuses.
pub async fn sync_pwned_status(scraper: &MachineScraper, machines: &mut [Machine]) -> Result<()> {
    let first = scraper.get_machines(1, Some("hacked")).await?;
    let total = total_pages_of(&first.pages_info);
    let mut all_hacked = first.machines;
    if total > 1 {
        all_hacked.extend(fetch_remaining_pages(scraper, Some("hacked"), total).await?);
    }

    let hacked_map: std::collections::HashMap<String, String> = all_hacked
        .into_iter()
        .map(|m| (m.name.trim().to_lowercase(), m.status))
        .collect();

    for m in machines.iter_mut() {
        if let Some(status) = hacked_map.get(&m.name.trim().to_lowercase()) {
            m.status = status.clone();
        }
    }
    Ok(())
}

pub async fn run(
    scraper: &MachineScraper,
    list: bool,
    all: bool,
    sort: Option<String>,
    name: Option<String>,
    page: usize,
) -> Result<()> {
    let _ = list; // kept for CLI parity; -l is the default listing mode
    let sort = sort.map(|s| s.to_lowercase());

    // Does this invocation need the *entire* catalog?
    let is_fetch_all = all || sort.as_deref() == Some("all") || name.is_some();

    let mut machines: Vec<Machine>;
    let info_text: String;

    if is_fetch_all {
        let target_level = if name.is_some()
            || sort.as_deref().is_some_and(|s| CATEGORIES_NEEDING_ALL.contains(&s))
            || (all && sort.is_none())
        {
            "all"
        } else {
            sort.as_deref().unwrap_or("all")
        };

        let status_msg = match &name {
            Some(q) => format!("Searching for '{q}'..."),
            None => "Fetching full machine catalog...".to_string(),
        };
        let sp = spinner(&status_msg);

        let first = scraper.get_machines(1, Some(target_level)).await?;
        let total = total_pages_of(&first.pages_info);
        let mut found = first.machines;
        if total > 1 {
            found.extend(fetch_remaining_pages(scraper, Some(target_level), total).await?);
        }

        // Deduplicate by lowercase name (mirrors main.py).
        let mut seen = std::collections::HashSet::new();
        found.retain(|m| seen.insert(m.name.trim().to_lowercase()));

        // Local filtering / sorting (mirrors main.py).
        if let Some(s) = &sort {
            if s == "linux" || s == "windows" {
                found.retain(|m| m.os == *s);
            } else if DIFFICULTIES.contains(&s.as_str()) {
                found.retain(|m| m.difficulty.eq_ignore_ascii_case(s));
            } else if s == "size" {
                found.sort_by(|a, b| {
                    parse_size(&a.size)
                        .partial_cmp(&parse_size(&b.size))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        if let Some(q) = &name {
            let q_low = q.to_lowercase();
            found.retain(|m| m.name.to_lowercase().contains(&q_low));
        }

        info_text = format!("Total Found: {}", found.len());
        machines = found;
        sp.finish_and_clear();
    } else {
        let sp = spinner("Fetching data...");
        let current = scraper.get_machines(page, sort.as_deref()).await?;
        let mut list = current.machines;

        if sort.is_some() && list.len() > PER_PAGE {
            // Local pagination for filtered single-page results.
            let total_count = list.len();
            let total_pages = total_count.div_ceil(PER_PAGE);
            let start = page.saturating_sub(1) * PER_PAGE;
            let end = (start + PER_PAGE).min(total_count);
            list = list[start.min(total_count)..end].to_vec();
            info_text = format!("Page {page}/{total_pages}");
        } else {
            info_text = format!("Page {}", current.pages_info);
        }
        machines = list;
        sp.finish_and_clear();
    }

    // Sync global "pwned" status unless we are listing the hacked filter itself.
    if !machines.is_empty() && sort.as_deref() != Some("hacked") {
        let sp = spinner("Syncing pwned status...");
        sync_pwned_status(scraper, &mut machines).await?;
        sp.finish_and_clear();
    }

    if machines.is_empty() {
        anyhow::bail!("No machines found matching your criteria.");
    }

    print_table(&machines, &info_text, sort.as_deref(), name.as_deref());
    Ok(())
}

fn print_table(machines: &[Machine], info: &str, sort: Option<&str>, search: Option<&str>) {
    use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

    let mut title = format!("HMV Machines ({info})");
    if let Some(s) = sort {
        title += &format!(" | Filter: {}", s.to_uppercase());
    }
    if let Some(q) = search {
        title += &format!(" | Search: '{q}'");
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_header(vec!["VM Name", "Difficulty", "Creator", "Size", "Status"]);

    for m in machines {
        let diff = m.difficulty.to_lowercase();
        let diff_text = m.difficulty.to_uppercase();
        let diff_str = if diff.contains("beginner") {
            style(&diff_text).green()
        } else if diff.contains("inter") {
            style(&diff_text).yellow()
        } else if diff.contains("adv") {
            style(&diff_text).red()
        } else {
            style(&diff_text).white()
        };

        let raw_status = m.status.to_uppercase();
        let status_str = if raw_status.contains("DONE") || raw_status.contains("PWNED") {
            style(&raw_status).green()
        } else {
            style(&raw_status).yellow()
        };

        table.add_row(vec![
            style(&m.name).cyan().to_string(),
            diff_str.to_string(),
            style(&m.creator).magenta().to_string(),
            style(&m.size).green().to_string(),
            status_str.to_string(),
        ]);
    }

    println!("\n{}\n", style(title).blue().bold());
    println!("{table}");
}
