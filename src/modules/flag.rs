//! Flag submission for a specific VM.
//! Ported from FlagManager (hmv/modules/flag.py).

use anyhow::Result;
use console::style;

use crate::modules::session::HmvSession;

pub struct FlagManager {
    session: HmvSession,
}

impl FlagManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    /// Submits a flag and renders the various server responses.
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
}
