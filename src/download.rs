//! Resolves the HackMyVM download redirect to a public MEGA link. Consumed
//! by the dashboard's background downloader (`tui/downloads.rs`).

use anyhow::{bail, Context, Result};

/// Resolves the HackMyVM redirect to the public MEGA link. Done manually:
/// reqwest strips the `#key` fragment from Location URLs while following
/// redirects, but the MEGA key lives exactly in that fragment. No session
/// required — the endpoint is public.
pub async fn resolve_mega_link(vm_name: &str) -> Result<String> {
    let resolve_url = format!(
        "https://downloads.hackmyvm.eu/{}.zip",
        vm_name.to_lowercase()
    );
    let resolver = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;
    let response = resolver
        .get(&resolve_url)
        .send()
        .await
        .context("Connection error")?;
    let resolved = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    match resolved {
        Some(link) if link.contains("mega.nz") => Ok(link),
        _ => bail!("Error: Valid MEGA link not found."),
    }
}
