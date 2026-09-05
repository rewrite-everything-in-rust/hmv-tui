//! Minimal MEGA public-file client used by HackMyVM downloads.

pub mod crypto;

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::io::AsyncWriteExt;

use crypto::{ChunkedMac, CtrStream, FileKeys};

const API_URL: &str = "https://g.api.mega.co.nz/cs";
static SEQUENCE: AtomicU32 = AtomicU32::new(1);

struct PublicFile {
    name: String,
    size: u64,
    url: String,
}

/// Downloads and decrypts a public MEGA file, reporting through `progress`.
/// Pass a `ProgressBar` attached to a `MultiProgress` for batch operations.
pub async fn download_public(
    url: &str,
    destination: &Path,
    progress: ProgressBar,
) -> Result<PathBuf> {
    let info_spinner = crate::ui::spinner("Fetching file metadata...");
    let (file, keys) = fetch_public_file(url).await?;
    info_spinner.finish_and_clear();

    let filename = sanitize_filename(&file.name);
    let output = destination.join(&filename);
    if output.exists() {
        bail!("File '{}' already exists.", filename);
    }

    let part = destination.join(format!("{filename}.part"));
    progress.set_length(file.size);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.blue} {msg} {bar:40.cyan/blue} {percent:>3}% • {bytes}/{total_bytes} • {bytes_per_sec} • {eta}",
        )
        .expect("static progress template")
        .progress_chars("█░"),
    );
    progress.set_message(format!("Downloading {filename}"));

    let result = stream_to_file(&file, &keys, &part, &progress).await;
    match result {
        Ok(()) => {
            progress.finish_and_clear();
            tokio::fs::rename(&part, &output)
                .await
                .context("Failed to finalize downloaded file")?;
            Ok(output)
        }
        Err(error) => {
            progress.finish_and_clear();
            let _ = tokio::fs::remove_file(&part).await;
            Err(error)
        }
    }
}

async fn fetch_public_file(url: &str) -> Result<(PublicFile, FileKeys)> {
    let (file_id, raw_key) = crypto::parse_public_url(url)?;
    let keys = crypto::derive_file_keys(&raw_key)?;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let request = serde_json::json!([{"a": "g", "g": 1, "p": file_id}]);

    let response = reqwest::Client::builder()
        .user_agent("HMV-CLI/0.2.0")
        .timeout(std::time::Duration::from_secs(160))
        .build()?
        .post(format!("{API_URL}?id={sequence}&n={file_id}"))
        .json(&request)
        .send()
        .await
        .context("MEGA API request failed")?
        .error_for_status()
        .context("MEGA API returned an error status")?;

    let body: serde_json::Value = response
        .json()
        .await
        .context("MEGA API returned invalid JSON")?;
    let value = body
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| anyhow!("MEGA API returned an empty response"))?;

    if let Some(code) = value.as_i64() {
        bail!("MEGA API error {code}: {}", api_error(code));
    }

    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("MEGA API returned an unexpected response"))?;
    let download_url = object
        .get("g")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("File is not accessible anymore"))?;
    let size = object
        .get("s")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("MEGA response missing file size"))?;
    let attributes = object
        .get("at")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("MEGA response missing file attributes"))?;
    let name = crypto::decrypt_attr(attributes, &keys.aes_key)
        .unwrap_or_else(|_| "download.zip".to_string());

    Ok((
        PublicFile {
            name,
            size,
            url: download_url.replacen("http://", "https://", 1),
        },
        keys,
    ))
}

async fn stream_to_file(
    info: &PublicFile,
    keys: &FileKeys,
    part: &Path,
    progress: &ProgressBar,
) -> Result<()> {
    let response = reqwest::Client::new()
        .get(&info.url)
        .send()
        .await
        .context("MEGA download request failed")?
        .error_for_status()
        .context("MEGA storage node returned an error")?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(part)
        .await
        .context("Failed to create partial output file")?;
    let mut ctr = CtrStream::new(keys);
    let mut mac = ChunkedMac::new(keys, info.size);
    let mut pending = Vec::with_capacity(256 * 1024);
    let mut written = 0u64;

    while let Some(chunk) = stream.next().await {
        pending.extend_from_slice(&chunk.context("Network error during MEGA download")?);

        // Only decrypt whole blocks now; the raw tail stays in `pending` until
        // it too forms a full block. Re-keying the whole buffer would re-XOR
        // already-decrypted bytes and corrupt the output.
        let full = pending.len() / 16 * 16;
        if full > 0 {
            let (blocks, _) = pending.split_at_mut(full);
            ctr.apply(blocks);
            for block in blocks.as_chunks::<16>().0 {
                mac.update(block);
            }
            file.write_all(blocks).await?;
            written += full as u64;
            progress.set_position(written);
            pending.drain(..full);
        }
    }

    if !pending.is_empty() {
        let actual_len = pending.len();
        ctr.apply(&mut pending);
        file.write_all(&pending).await?;
        written += actual_len as u64;
        let mut final_block = [0u8; 16];
        final_block[..actual_len].copy_from_slice(&pending);
        mac.update(&final_block);
        progress.set_position(written);
    }

    if written != info.size {
        bail!("Incomplete MEGA download: received {written} of {} bytes", info.size);
    }
    mac.verify()?;
    file.flush().await?;
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|character| *character != '/' && *character != '\\' && !character.is_control())
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "download.zip".to_string()
    } else {
        cleaned
    }
}

fn api_error(code: i64) -> &'static str {
    match code {
        -3 => "rate limit exceeded",
        -9 => "file not found",
        -11 => "access denied",
        _ => "unknown MEGA error",
    }
}
