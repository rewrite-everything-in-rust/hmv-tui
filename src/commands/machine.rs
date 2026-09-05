//! Catalog fetching for the dashboard: full-catalog retrieval with bounded
//! concurrency and global pwned-status sync.

use anyhow::Result;
use futures_util::stream;
use futures_util::StreamExt;

use crate::modules::machines::{Machine, MachineScraper, Page};

const CONCURRENCY: usize = 3;

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
