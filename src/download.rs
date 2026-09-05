//! VM download orchestration: resolve the HMV redirect to a MEGA public link,
//! then decrypt, verify and store the archive. Ported from DownloadManager
//! (hmv/modules/download.py).

use anyhow::{bail, Result};
use console::style;
use std::path::Path;

use crate::mega;
use crate::modules::session::HmvSession;
use crate::ui::spinner;

pub struct DownloadManager {
    session: HmvSession,
}

impl DownloadManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    pub async fn download_vm(&self, vm_name: &str) -> Result<std::path::PathBuf> {
        let resolve_url = format!(
            "https://downloads.hackmyvm.eu/{}.zip",
            vm_name.to_lowercase()
        );

        let resolving = spinner(format!("Resolving download link for {vm_name}..."));
        let response = self.session.get_raw(&resolve_url).await?;
        let resolved = response.url().to_string();
        resolving.finish_and_clear();

        if !resolved.contains("mega.nz") {
            bail!("Error: Valid MEGA link not found.");
        }

        println!(
            "{} Resolved Link: {}",
            style("[*]").blue().bold(),
            style(&resolved).cyan()
        );

        let output = mega::download_public(&resolved, Path::new(".")).await?;

        println!(
            "{} Successfully downloaded: {}",
            style("[+]").green().bold(),
            style(output.display().to_string()).white()
        );
        Ok(output)
    }
}