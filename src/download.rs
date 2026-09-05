//! VM download orchestration: resolve the HMV redirect to a MEGA public link,
//! then decrypt, verify and store the archive. Ported from DownloadManager
//! (hmv/modules/download.py) and extended with concurrent batch downloads.

use anyhow::{bail, Context, Result};
use console::style;
use futures_util::stream;
use futures_util::StreamExt;
use indicatif::MultiProgress;
use std::path::{Path, PathBuf};

use crate::mega;

/// At most two VM archives are pulled from MEGA in parallel.
const PARALLEL_DOWNLOADS: usize = 2;

pub struct DownloadManager;

impl DownloadManager {
    pub fn new() -> Self {
        Self
    }

    /// Downloads one or many VMs, up to `PARALLEL_DOWNLOADS` at a time.
    pub async fn download_vms(&self, vm_names: &[String]) -> Result<Vec<PathBuf>> {
        if vm_names.len() == 1 {
            let path = self.download_vm(&vm_names[0]).await?;
            return Ok(vec![path]);
        }

        let mut seen = std::collections::HashSet::new();
        let names: Vec<String> = vm_names
            .iter()
            .filter(|name| seen.insert(name.to_lowercase()))
            .cloned()
            .collect();

        println!(
            "{} Initializing Batch Operations ({} VMs, {} in parallel)...",
            style("[+]").green().bold(),
            names.len(),
            PARALLEL_DOWNLOADS
        );

        let multi = MultiProgress::new();
        let jobs: Vec<(String, MultiProgress)> = names
            .iter()
            .cloned()
            .map(|name| (name, multi.clone()))
            .collect();

        let results: Vec<(String, Result<PathBuf>)> = stream::iter(jobs)
            .map(|(name, multi)| async move {
                let bar = multi.add(indicatif::ProgressBar::new_spinner());
                let result = self.download_vm_with(&name, &multi, Some(bar)).await;
                (name, result)
            })
            .buffer_unordered(PARALLEL_DOWNLOADS)
            .collect()
            .await;

        let mut downloaded = Vec::new();
        let mut failures = Vec::new();
        for (name, result) in results {
            match result {
                Ok(path) => downloaded.push(path),
                Err(error) => failures.push(format!("{name}: {error:#}")),
            }
        }

        if !downloaded.is_empty() {
            let names: Vec<String> = downloaded
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            println!(
                "{} Successfully downloaded: {}",
                style("[+]").green().bold(),
                style(names.join(", ")).white()
            );
        }
        if !failures.is_empty() {
            for failure in &failures {
                println!("{} {failure}", style("[!]").red().bold());
            }
            bail!("Some downloads failed.");
        }
        Ok(downloaded)
    }

    /// Single download with a standalone spinner (non-batch path).
    pub async fn download_vm(&self, vm_name: &str) -> Result<PathBuf> {
        let multi = MultiProgress::new();
        self.download_vm_with(vm_name, &multi, None).await
    }

    async fn download_vm_with(
        &self,
        vm_name: &str,
        multi: &MultiProgress,
        bar: Option<indicatif::ProgressBar>,
    ) -> Result<PathBuf> {
        let resolve_url = format!(
            "https://downloads.hackmyvm.eu/{}.zip",
            vm_name.to_lowercase()
        );

        // Resolve the redirect manually: reqwest strips the `#key` fragment
        // from Location URLs while following redirects, but the MEGA key
        // lives exactly in that fragment.
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
        let resolved = match response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
        {
            Some(location) => location,
            None => bail!("Error: Valid MEGA link not found."),
        };

        if !resolved.contains("mega.nz") {
            bail!("Error: Valid MEGA link not found.");
        }

        let _ = multi.println(format!(
            "{} Resolved Link [{}]: {}",
            style("[*]").blue().bold(),
            vm_name,
            style(&resolved).cyan()
        ));

        let bar = bar.unwrap_or_else(|| multi.add(indicatif::ProgressBar::new(0)));
        let output = mega::download_public(&resolved, Path::new("."), bar).await?;

        Ok(output)
    }
}