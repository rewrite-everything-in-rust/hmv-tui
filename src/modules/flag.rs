//! Flag submission for a specific VM.
//! Ported from FlagManager (hmv/modules/flag.py).

use anyhow::{bail, Result};
use console::style;
use futures_util::future::join_all;

use crate::modules::session::HmvSession;

/// Server verdict for a flag check, decoupled from printing so both the CLI
/// and the TUI can render it their own way.
#[derive(Debug, Clone)]
pub enum FlagVerdict {
    Correct,
    Wrong,
    MachineNotFound,
    Unknown(String),
}

pub struct FlagManager {
    session: HmvSession,
}

impl FlagManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    /// Network-only check: submits the flag and classifies the response.
    pub async fn check(&self, vm: &str, flag: &str) -> Result<FlagVerdict> {
        let body = self
            .session
            .post_form("/machines/checkflag.php", &[("vm", vm), ("flag", flag)])
            .await?;
        let msg = body.to_lowercase();
        Ok(if msg.contains("correct") {
            FlagVerdict::Correct
        } else if msg.contains("wrong") {
            FlagVerdict::Wrong
        } else if msg.contains("<link") || msg.contains("stylesheet") || msg.contains("<html") {
            FlagVerdict::MachineNotFound
        } else {
            FlagVerdict::Unknown(body.trim().to_string())
        })
    }

    /// Submits one flag and renders the various server responses. (CLI path)
    pub async fn submit(&self, vm: &str, flag: &str) -> Result<()> {
        match self.check(vm, flag).await? {
            FlagVerdict::Correct => {
                println!(
                    "{} You hacked {}!",
                    style("[✓] Correct!").green().bold(),
                    style(vm).white().bold()
                );
            }
            FlagVerdict::Wrong => {
                println!("{} Wrong flag. Try harder!", style("[!]").red().bold());
            }
            FlagVerdict::MachineNotFound => {
                println!(
                    "{} Error: Machine '{}' was not found.",
                    style("[!]").red().bold(),
                    style(vm).white().bold()
                );
                println!("{} Please check the VM name spelling.", style("[*]").yellow());
            }
            FlagVerdict::Unknown(body) => {
                println!(
                    "{} Unknown server response: {}",
                    style("[?]").yellow().bold(),
                    body
                );
            }
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
                let verdict = self.check(vm, &flag).await?;
                Ok::<_, anyhow::Error>((index + 1, flag, verdict))
            }
        });

        let results = join_all(futures).await;
        for result in results {
            let (position, flag, verdict) = result?;
            let label = style(format!("Flag {position}")).cyan().bold();
            let token = style(&flag).dim();
            match verdict {
                FlagVerdict::Correct => {
                    println!("{label} {token}: {} (Accepted)", style("[✓]").green().bold());
                }
                FlagVerdict::Wrong => {
                    println!("{label} {token}: {} Rejected", style("[!]").red().bold());
                }
                FlagVerdict::MachineNotFound => {
                    println!("{label} {token}: Machine '{vm}' was not found.");
                }
                FlagVerdict::Unknown(body) => {
                    println!(
                        "{label} {token}: {} Unknown server response: {}",
                        style("[?]").yellow().bold(),
                        body
                    );
                }
            }
        }
        Ok(())
    }
}
