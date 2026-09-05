//! Upcoming machine release schedule (`hmv machine -r`).
//! Scrapes the live /hmv/nextreleases.php page (Issue #1, part 2).

use anyhow::Result;
use scraper::Selector;

use crate::modules::session::HmvSession;

#[derive(Debug, Clone)]
pub struct Release {
    pub date: String,
    pub name: String,
    pub os: String,
    /// Machines already launched are struck through on the site.
    pub released: bool,
}

pub struct ReleaseScraper {
    session: HmvSession,
}

impl ReleaseScraper {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    pub async fn get_releases(&self) -> Result<Vec<Release>> {
        let html = self.session.get("/hmv/nextreleases.php").await?;
        Ok(parse_releases(&html))
    }
}

/// Pure parsing function so it can be unit-tested against fixture HTML.
pub fn parse_releases(html: &str) -> Vec<Release> {
    let doc = scraper::Html::parse_document(html);

    let entry_sel = Selector::parse("p.htitulo3").unwrap();
    let os_sel = Selector::parse("img").unwrap();

    doc.select(&entry_sel)
        .map(|entry| {
            let date = entry
                .text()
                .next()
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            // The date already carries the month, e.g. "03-Sept".
            let os = entry
                .select(&os_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|src| {
                    let file = src.rsplit('/').next().unwrap_or("");
                    file.trim_end_matches(".png").to_string()
                })
                .unwrap_or_else(|| "unknown".to_string());
            let name = entry
                .select(&Selector::parse("a").unwrap())
                .next()
                .map(|a| a.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| {
                    // Unreleased machines are plain text after the OS icon.
                    entry
                        .text()
                        .collect::<String>()
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .to_string()
                });

            Release {
                date,
                name,
                os,
                released: entry
                    .parent()
                    .and_then(|node| node.value().as_element())
                    .map(|element| element.name() == "del")
                    .unwrap_or(false),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
    <html><body>
    <h3 class="htitulo2 mt-3">Sept</h3>
    <del><p class="htitulo3 mt-3">03-Sept <span style="width: 10px; border-left: 10px solid orange"><img class="ml-2" src="/img/linux.png" width="24px" height="24px"/><a href="/machines/machine.php?vm=Arcana" class="ml-2">Arcane</a></span></p></del>
    <p class="htitulo3 mt-3">09-Sept <span style="width: 10px; border-left: 10px solid green"><img class="ml-2" src="/img/linux.png" width="24px" height="24px"/><a class="ml-2">INVERNADERO_1.0</a></span></p>
    </body></html>"#;

    #[test]
    fn parses_live_release_fixture() {
        let releases = parse_releases(FIXTURE);
        assert_eq!(releases.len(), 2);

        assert_eq!(releases[0].date, "03-Sept");
        assert_eq!(releases[0].name, "Arcane");
        assert_eq!(releases[0].os, "linux");
        assert!(releases[0].released);

        assert_eq!(releases[1].date, "09-Sept");
        assert_eq!(releases[1].name, "INVERNADERO_1.0");
        assert_eq!(releases[1].os, "linux");
        assert!(!releases[1].released);
    }
}