//! Flag submission for a specific VM.
//! Ported from FlagManager (hmv/modules/flag.py).

use anyhow::{bail, Result};
use console::style;
use futures_util::future::join_all;

use crate::modules::session::HmvSession;

pub struct FlagManager {
    session: HmvSession,
}

impl FlagManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    /// Submits one flag and renders the various server responses.
    pub async fn submit(&self, vm: &str, flag: &str) -> Result<()> {
        let body = self
            .session
            .post_form("/machines/checkflag.php", &[("vm", vm), ("flag", flag)])
            .await?;
        let msg = body.to_lowercase();

        if msg.contains("correct") {
            println!(
                "{} You hacked {}!",
                style("[✓] Correct!").green().bold(),
                style(vm).white().bold()
            );
        } else if msg.contains("wrong") {
            println!(
                "{} Wrong flag. Try harder!",
                style("[!]").red().bold()
            );
        } else if msg.contains("<link") || msg.contains("stylesheet") || msg.contains("<html") {
            println!(
                "{} Error: Machine '{}' was not found.",
                style("[!]").red().bold(),
                style(vm).white().bold()
            );
            println!(
                "{} Please check the VM name spelling.",
                style("[*]").yellow()
            );
        } else {
            println!(
                "{} Unknown server response: {}",
                style("[?]").yellow().bold(),
                body.trim()
            );
        }
        Ok(())
    }

    /// Submits up to two flags (User & Root) concurrently, labeling each result.
    pub async fn submit_batch(&self, vm: &str, flags: &[String]) -> Result<()> {
        if flags.len() > 2 {
            bail!("Error: A maximum of 2 flags (User & Root) can be submitted per command.");
        }

        if flags.len() == 1 {
            return self.submit(vm, &flags[0]).await;
        }

        println!(
            "{} Submitting Flags for [{}]...",
            style("[+]").green().bold(),
            style(vm).white().bold()
        );

        let futures = flags.iter().enumerate().map(|(index, flag)| {
            let flag = flag.clone();
            async move {
                let body = self
                    .session
                    .post_form("/machines/checkflag.php", &[("vm", vm), ("flag", flag.as_str())])
                    .await?;
                Ok::<_, anyhow::Error>((index + 1, flag, body))
            }
        });

        let results = join_all(futures).await;
        for result in results {
            let (position, flag, body) = result?;
            let msg = body.to_lowercase();
            let label = style(format!("Flag {position}")).cyan().bold();
            let token = style(&flag).dim();
            if msg.contains("correct") {
                println!("{label} {token}: {} (Accepted)", style("[✓]").green().bold());
            } else if msg.contains("wrong") {
                println!("{label} {token}: {} Rejected", style("[!]").red().bold());
            } else if msg.contains("<link") || msg.contains("stylesheet") || msg.contains("<html") {
                println!("{label} {token}: Machine '{vm}' was not found.");
            } else {
                println!(
                    "{label} {token}: {} Unknown server response: {}",
                    style("[?]").yellow().bold(),
                    body.trim()
                );
            }
        }
        Ok(())
    }
}
