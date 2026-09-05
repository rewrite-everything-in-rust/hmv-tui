//! Flag submission for a specific VM.
//! Ported from FlagManager (hmv/modules/flag.py).

use anyhow::Result;

use crate::modules::session::HmvSession;

/// Server verdict for a flag check, decoupled from rendering so the TUI can
/// present it in its result popups.
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
}
