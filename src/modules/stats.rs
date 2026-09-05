//! Personal profile statistics for the authenticated user.
//! Implements `hmv stats` (Issue #1) against the live profile page.

use anyhow::{anyhow, Result};
use scraper::Selector;

use crate::modules::session::HmvSession;

#[derive(Debug, Clone, Default)]
pub struct ProfileStats {
    pub username: String,
    pub rank: Option<String>,
    pub country: Option<String>,
    pub title: Option<String>,
    pub points: u64,
    pub roots: u64,
    pub users: u64,
    pub first_roots: u64,
    pub first_users: u64,
    pub challenges: u64,
    pub writeups: u64,
    pub loved: u64,
    pub trophies: Vec<String>,
}

pub struct StatsManager {
    session: HmvSession,
}

impl StatsManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    pub async fn get_stats(&self, username: &str) -> Result<ProfileStats> {
        let html = self
            .session
            .get(&format!("/profile/?user={username}"))
            .await?;
        parse_profile(&html)
    }
}

/// Pure parsing function so it can be unit-tested against fixture HTML.
pub fn parse_profile(html: &str) -> Result<ProfileStats> {
    let doc = scraper::Html::parse_document(html);

    // Identity: "username #rank" inside h3.profile-username.
    let head_sel = Selector::parse("h3.profile-username").unwrap();
    let header = doc
        .select(&head_sel)
        .next()
        .ok_or_else(|| anyhow!("Profile not found."))?;
    let combined = header.text().collect::<String>();
    let (username, rank) = match combined.split_once('#') {
        Some((name, rest)) => (
            name.trim().to_string(),
            Some(format!("#{rest}").trim().to_string()),
        ),
        None => (combined.trim().to_string(), None),
    };

    // Country flag icon: /img/flags/<code>.svg
    let flag_sel = Selector::parse("h3.profile-username img[src*='/img/flags/']").unwrap();
    let country = doc
        .select(&flag_sel)
        .next()
        .and_then(|img| img.value().attr("src"))
        .and_then(|src| src.rsplit('/').next())
        .and_then(|file| file.strip_suffix(".svg"))
        .map(|code| code.to_uppercase());

    // Title: the bracketed rank title right below the header, e.g. [WTF].
    let title_sel = Selector::parse("span.h5").unwrap();
    let title = doc
        .select(&title_sel)
        .next()
        .map(|span| span.text().collect::<String>().trim().to_string())
        .filter(|text| !text.is_empty());

    // Points badge: "1767 points".
    let badge_sel = Selector::parse("span.badge.badge-light").unwrap();
    let points = doc
        .select(&badge_sel)
        .next()
        .map(|badge| badge.text().collect::<String>())
        .and_then(|text| {
            text.split_whitespace()
                .next()
                .and_then(|number| number.parse::<u64>().ok())
        })
        .unwrap_or(0);

    // Stats block: p.caz.piz lines "Label: N" plus the trailing heart count.
    let stat_sel = Selector::parse("p.caz.piz").unwrap();
    let mut stats = ProfileStats {
        username,
        rank,
        country,
        title,
        points,
        ..Default::default()
    };

    for element in doc.select(&stat_sel) {
        let line = element.text().collect::<String>();
        let line = line.trim();
        if let Some((label, value)) = line.split_once(':') {
            let value = value.trim().parse().unwrap_or(0);
            match label.trim().to_lowercase().as_str() {
                "total roots" => stats.roots = value,
                "total users" => stats.users = value,
                "firstroots" => stats.first_roots = value,
                "firstusers" => stats.first_users = value,
                "challenges" => stats.challenges = value,
                "writeups" => stats.writeups = value,
                _ => {}
            }
        } else {
            // Heart-eyes emoji line holds only the "loved" count.
            stats.loved = line.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
        }
    }

    // Trophies are rendered as titled images from /img/trophies/.
    let trophy_sel = Selector::parse("img[src*='/img/trophies/']").unwrap();
    stats.trophies = doc
        .select(&trophy_sel)
        .filter_map(|img| img.value().attr("title"))
        .map(str::to_string)
        .collect();

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
    <html><body>
    <h3 class="pixeltitle nospace profile-username font-weight-bold mb-0 text">demouser<img class="ml-2 mb-2" src="/img/flags/id.svg" width="15px" /> <span class="nospace primary-text-color profile-username font-weight-bold mb-0 text">#38</span></h3>
    <span class="font-weight-bold h5">[WTF]</span>
    <span class="rounded font-weight-bold badge badge-light">1767 points</span>
    <span class="vmtitle">Stats</span>
    <p class="caz piz">Total Roots: 166</p>
    <p class="caz piz">Total Users: 166</p>
    <p class="caz piz">FirstRoots: 1</p>
    <p class="caz piz">FirstUsers: 1</p>
    <p class="caz piz">Challenges: 56</p>
    <p class="caz piz">Writeups: 125</p>
    <p class="caz piz"><svg></svg> 9</p>
    <h3 class="pixeltitle mt-2">Trophies</h3>
    <img src="/img/trophies/vfinisher.png" title="vfinisher">
    <img src="/img/trophies/poet.png" title="poet">
    </body></html>"#;

    #[test]
    fn parses_live_profile_fixture() {
        let stats = parse_profile(FIXTURE).unwrap();
        assert_eq!(stats.username, "demouser");
        assert_eq!(stats.rank.as_deref(), Some("#38"));
        assert_eq!(stats.country.as_deref(), Some("ID"));
        assert_eq!(stats.title.as_deref(), Some("[WTF]"));
        assert_eq!(stats.points, 1767);
        assert_eq!(stats.roots, 166);
        assert_eq!(stats.users, 166);
        assert_eq!(stats.first_roots, 1);
        assert_eq!(stats.first_users, 1);
        assert_eq!(stats.challenges, 56);
        assert_eq!(stats.writeups, 125);
        assert_eq!(stats.loved, 9);
        assert_eq!(stats.trophies, vec!["vfinisher", "poet"]);
    }
}