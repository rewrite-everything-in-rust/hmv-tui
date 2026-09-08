//! Machine listing data model + scraper for /machines/.
//! Ported from MachineScraper (hmv/modules/scraper.py).

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

use crate::modules::session::HmvSession;

#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub name: String,
    pub creator: String,
    pub size: String,
    pub difficulty: String,
    pub os: String,
    /// Hypervisors the machine is tested on, e.g. "VirtualBox/VMware"
    /// (scraped from the row text; empty when the site lists none).
    pub tested: String,
    pub status: String,
}

/// Hex border color on the site -> difficulty label.
fn color_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("#28a745", "beginner"),
        ("#ffc107", "intermediate"),
        ("#dc3545", "advanced"),
    ])
}

/// Parses a human size like "1.9 Gb" or "450 Mb" into megabytes so machines
/// can be sorted by real size (the site mixes Gb/Mb labels). Unknown shapes
/// sort as 0.
pub fn size_mb(size: &str) -> f64 {
    let mut tokens = size.split_whitespace();
    let number: f64 = tokens.next().and_then(|n| n.parse().ok()).unwrap_or(0.0);
    match tokens.next().unwrap_or("").to_lowercase().as_str() {
        unit if unit.starts_with('g') => number * 1024.0,
        unit if unit.starts_with('m') => number,
        _ => number,
    }
}

pub struct MachineScraper {
    session: HmvSession,
    colors: HashMap<&'static str, &'static str>,
}

pub struct Page {
    pub machines: Vec<Machine>,
    /// "current/total" as rendered by the site, e.g. "3/17".
    pub pages_info: String,
}

impl MachineScraper {
    pub fn new(session: HmvSession) -> Self {
        Self {
            session,
            colors: color_map(),
        }
    }

    /// Fetches one page of machines and pagination info.
    pub async fn get_machines(&self, page: usize, level: Option<&str>) -> Result<Page> {
        let mut path = format!("/machines/?p={page}");
        if let Some(l) = level {
            path.push_str(&format!("&l={l}"));
        }

        let html = self.session.get(&path).await?;
        Ok(parse_machines(&html, page, &self.colors))
    }
}

/// Pure parsing function so it can be unit-tested against fixture HTML.
pub fn parse_machines(
    html: &str,
    page: usize,
    colors: &HashMap<&'static str, &'static str>,
) -> Page {
    let doc = scraper::Html::parse_document(html);

    let row_sel = scraper::Selector::parse("table.table-dark tbody tr").unwrap();
    let name_sel = scraper::Selector::parse("h4.vmname a").unwrap();
    let style_sel = scraper::Selector::parse("div[style*='border-top']").unwrap();
    let img_sel = scraper::Selector::parse("img").unwrap();
    let badge_sel = scraper::Selector::parse("span.badge").unwrap();
    let creator_sel = scraper::Selector::parse("a.creator").unwrap();
    let size_sel = scraper::Selector::parse("p.size").unwrap();

    let mut machines = Vec::new();
    for row in doc.select(&row_sel) {
        let Some(name_node) = row.select(&name_sel).next() else {
            continue;
        };

        // Difficulty is encoded as an inline border-top color.
        let diff_hex = row
            .select(&style_sel)
            .next()
            .and_then(|n| n.value().attr("style"))
            .map(|s| s.to_lowercase())
            .and_then(|style| {
                style
                    .rsplit("solid ")
                    .next()
                    .map(|c| c.trim_end_matches(';').trim().to_string())
            })
            .unwrap_or_default();

        // OS is derived from linux/windows icons in the row.
        let mut os_type = "unknown";
        for img in row.select(&img_sel) {
            let src = img.value().attr("src").unwrap_or("").to_lowercase();
            let title = img.value().attr("title").unwrap_or("").to_lowercase();
            if src.contains("linux") || title.contains("linux") {
                os_type = "linux";
                break;
            } else if src.contains("windows") || title.contains("windows") {
                os_type = "windows";
                break;
            }
        }

        // Status badge: TO HACK / DONE / PWNED.
        let mut status = "TO HACK";
        for badge in row.select(&badge_sel) {
            let text = badge.text().collect::<String>().trim().to_uppercase();
            if matches!(text.as_str(), "TO HACK" | "DONE" | "PWNED") {
                status = match text.as_str() {
                    "DONE" => "DONE",
                    "PWNED" => "PWNED",
                    _ => "TO HACK",
                };
                break;
            }
        }

        // Hypervisor compatibility: the row carries icon images whose src
        // encodes the tested hypervisor(s):
        //   /img/vb.png    -> tested on VirtualBox only
        //   /img/vbvm.png  -> tested on VirtualBox and VMware
        let mut hypervisors: Vec<&str> = Vec::new();
        for img in row.select(&img_sel) {
            let src = img.value().attr("src").unwrap_or("").to_lowercase();
            if src.ends_with("/vb.png") || src == "vb.png" {
                if !hypervisors.contains(&"VirtualBox") {
                    hypervisors.push("VirtualBox");
                }
            } else if src.ends_with("/vbvm.png") || src == "vbvm.png" {
                if !hypervisors.contains(&"VirtualBox") {
                    hypervisors.push("VirtualBox");
                }
                if !hypervisors.contains(&"VMware") {
                    hypervisors.push("VMware");
                }
            }
        }

        machines.push(Machine {
            name: name_node.text().collect::<String>().trim().to_string(),
            creator: row
                .select(&creator_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "-".into()),
            size: row
                .select(&size_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "0 MB".into()),
            difficulty: colors
                .get(diff_hex.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown".into()),
            os: os_type.to_string(),
            tested: hypervisors.join("/"),
            status: status.to_string(),
        });
    }

    // Pagination: the site renders the current page in a disabled page item.
    let pages_sel = scraper::Selector::parse("li.page-item.disabled a.page-link").unwrap();
    let mut pages_info = format!("{page}/?");
    for item in doc.select(&pages_sel) {
        let text = item.text().collect::<String>().trim().to_string();
        if text.contains('/') {
            pages_info = text;
            break;
        }
    }

    Page {
        machines,
        pages_info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes_into_megabytes() {
        assert_eq!(size_mb("2 Gb"), 2048.0);
        assert_eq!(size_mb("450 Mb"), 450.0);
        assert_eq!(size_mb("700 MB"), 700.0); // unit is case-insensitive
        assert_eq!(size_mb("1.5 gb"), 1536.0);
        assert_eq!(size_mb("garbage"), 0.0);
        assert_eq!(size_mb(""), 0.0);
    }

    const ROW_FIXTURE: &str = r##"
    <html><body><table class="table-dark"><tbody>
    <tr>
      <td><div style="border-top: solid #28a745"></div>
          <h4 class="vmname"><a href="#">Easy</a></h4>
          <img class="ml-2 mr-2" src="/img/linux.png" title="Linux VM" width="25" height="25">
          <img class="ml-2" src="../img/vb.png" title="Tested on VirtualBox">
          <a class="creator">tom</a>
          <p class="size">450 Mb</p>
          <span class="badge">TO HACK</span></td>
    </tr>
    <tr>
      <td><div style="border-top: solid #dc3545"></div>
          <h4 class="vmname"><a href="#">Hard</a></h4>
          <img class="ml-2 mr-2" src="/img/windows.png" title="Windows VM" width="25" height="25">
          <img class="ml-2" src="/img/vbvm.png" title="Tested on VirtualBox and VMWare.">
          <a class="creator">someone</a>
          <p class="size">1.9 Gb</p>
          <span class="badge">PWNED</span></td>
    </tr>
    <tr>
      <td><div style="border-top: solid #ffc107"></div>
          <h4 class="vmname"><a href="#">Medium</a></h4>
          <img class="ml-2 mr-2" src="/img/linux.png" title="Linux VM" width="25" height="25">
          <a class="creator">third</a>
          <p class="size">800 Mb</p>
          <span class="badge">TO HACK</span></td>
    </tr>
    </tbody></table></body></html>"##;

    #[test]
    fn parses_os_and_tested_from_row() {
        let page = parse_machines(ROW_FIXTURE, 1, &color_map());
        assert_eq!(page.machines.len(), 3);

        let easy = &page.machines[0];
        assert_eq!(easy.name, "Easy");
        assert_eq!(easy.os, "linux");
        assert_eq!(easy.difficulty, "beginner");
        assert_eq!(easy.tested, "VirtualBox");

        let hard = &page.machines[1];
        assert_eq!(hard.os, "windows");
        assert_eq!(hard.status, "PWNED");
        assert_eq!(hard.tested, "VirtualBox/VMware");

        let medium = &page.machines[2];
        assert_eq!(medium.os, "linux");
        assert_eq!(medium.tested, "", "row without hypervisor icon stays empty");
    }
}
