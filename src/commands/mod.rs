//! Dashboard orchestration: session lifecycle, data fetching and the host
//! closures handed to the TUI event loop.

pub mod machine;

use anyhow::Result;

use crate::config::ConfigManager;
use crate::modules::HmvError;
use crate::modules::flag::FlagManager;
use crate::modules::machines::MachineScraper;
use crate::modules::releases::ReleaseScraper;
use crate::modules::session::{login, login_with, HmvSession};
use crate::modules::stats::StatsManager;
use crate::modules::writeups::WriteupManager;
use crate::tui::{ActionReport, TuiAction, TuiData};

/// Reusable authenticated session for the TUI's lifetime. Cloning an
/// `HmvSession` is cheap (shared connection pool), so one login serves
/// every background action.
#[derive(Clone)]
pub struct SessionCache {
    session: HmvSession,
    username: String,
}

impl SessionCache {
    pub async fn new() -> Result<Self> {
        let cfg = ConfigManager::new();
        let (username, _) = cfg.load_credentials()?;
        let session = login(&cfg).await?;
        Ok(Self { session, username })
    }

    pub fn session(&self) -> HmvSession {
        self.session.clone()
    }
}

/// Session slot shared by all TUI closures; `None` until the config popup
/// succeeds and after a logout.
type SharedSession = std::sync::Arc<std::sync::Mutex<Option<SessionCache>>>;

fn take_session(shared: &SharedSession) -> Result<SessionCache> {
    shared
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Not configured — no HackMyVM session available."))
}

pub async fn tui_cmd() -> Result<()> {
    // Bare `hmv`: without usable stored credentials the TUI starts directly
    // in the config popup; otherwise log in now and enter the dashboard with
    // an empty state (the first fetch runs inside the event loop with a
    // `⟳ Loading data...` indicator).
    let cfg = ConfigManager::new();
    let stored_username = cfg.stored_username();
    let sessions = if stored_username.is_some() {
        match SessionCache::new().await {
            Ok(sessions) => Some(sessions),
            Err(error)
                if error.chain().any(|cause| {
                    cause
                        .downcast_ref::<HmvError>()
                        .is_some_and(|e| matches!(e, HmvError::AuthFailed))
                }) =>
            {
                // Stale password: offer re-configuration inside the TUI.
                None
            }
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let unconfigured = sessions.is_none();

    let shared: SharedSession = std::sync::Arc::new(std::sync::Mutex::new(sessions));
    let fetch_sessions = shared.clone();
    let action_sessions = shared.clone();
    let writeups_sessions = shared.clone();
    let config_sessions = shared.clone();
    let logout_sessions = shared.clone();

    let initial = if unconfigured {
        crate::tui::AppState::unconfigured(stored_username.as_deref())
    } else {
        crate::tui::AppState::loading()
    };

    crate::tui::run(
        initial,
        move || {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let sessions = take_session(&fetch_sessions)?;
                    fetch_tui_data(&sessions).await
                })
            })
        },
        move |action| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let sessions = take_session(&action_sessions)?;
                    run_tui_action(&sessions, action).await
                })
            })
        },
        move |vm| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let sessions = take_session(&writeups_sessions)?;
                    WriteupManager::new(sessions.session()).fetch(vm).await
                })
            })
        },
        move |username, password| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(configure_account(&config_sessions, username, password))
            })
        },
        move || {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(logout_account(&logout_sessions))
            })
        },
    )
}

/// Validates the entered credentials by logging in, stores them, and
/// installs the session for the rest of the dashboard lifetime. Used by the
/// first-run popup and by account switching.
async fn configure_account(shared: &SharedSession, username: &str, password: &str) -> Result<()> {
    let session = login_with(username, password).await?;
    ConfigManager::new().save_credentials(username, password)?;
    *shared.lock().unwrap() = Some(SessionCache {
        session,
        username: username.to_string(),
    });
    Ok(())
}

/// Removes the stored account and drops the in-memory session. Called from
/// the account popup (`l`). Running downloads are unaffected — they use
/// public MEGA links, not the session.
async fn logout_account(shared: &SharedSession) -> Result<()> {
    ConfigManager::new().clear_credentials()?;
    *shared.lock().unwrap() = None;
    Ok(())
}

/// Executes a user action from a TUI popup. Returns the result popup
/// content: verdicts labeled with the original field (User/Root flag),
/// `changed` telling whether dashboard data must be refreshed after the
/// popup closes.
async fn run_tui_action(sessions: &SessionCache, action: TuiAction) -> Result<ActionReport> {
    if matches!(
        action.kind,
        crate::tui::PopupKind::Download
            | crate::tui::PopupKind::Config
            | crate::tui::PopupKind::Account
    ) {
        anyhow::bail!("downloads, configuration and logout are handled directly by the event loop");
    }
    match action.kind {
        crate::tui::PopupKind::Download
        | crate::tui::PopupKind::Config
        | crate::tui::PopupKind::Account => unreachable!("handled by the event loop"),
        crate::tui::PopupKind::Flag => {
            use crate::modules::flag::FlagVerdict;

            if action.values.len() > 2 {
                anyhow::bail!("A maximum of 2 flags (user & root) can be submitted.");
            }

            let vm = action.vm.clone();
            let vm_ref = vm.as_str();
            let sessions_ref: &SessionCache = sessions;
            let futures = action.values.iter().map(|(field, flag)| {
                let flag = flag.clone();
                let vm = vm_ref;
                async move {
                    FlagManager::new(sessions_ref.session())
                        .check(vm, &flag)
                        .await
                        .map(|verdict| (*field, verdict))
                }
            });
            let results = futures_util::future::join_all(futures)
                .await
                .into_iter()
                .collect::<Result<Vec<(usize, FlagVerdict)>>>()?;

            Ok(crate::tui::build_flag_report(&action.vm, results))
        }
        crate::tui::PopupKind::Upload => {
            let url = action.values[0].1.clone();
            let verdict = WriteupManager::new(sessions.session())
                .submit(&action.vm, &url)
                .await?;
            use crate::tui::{ActionReport, ReportKind};
            let (entries, changed, status) = match verdict {
                crate::modules::writeups::UploadVerdict::Submitted => (
                    vec![(
                        ReportKind::Success,
                        format!("Writeup: ✓ ACCEPTED — {}", url),
                    )],
                    true,
                    format!("[✓] Writeup submitted for {}!", action.vm),
                ),
                crate::modules::writeups::UploadVerdict::Repeated => (
                    vec![(ReportKind::Info, "Writeup: [=] ALREADY SUBMITTED".to_string())],
                    false,
                    format!("[=] Writeup for {} was already submitted.", action.vm),
                ),
                crate::modules::writeups::UploadVerdict::Rejected => (
                    vec![(
                        ReportKind::Failure,
                        "Writeup: ✗ REJECTED — flags missing?".to_string(),
                    )],
                    false,
                    format!("[!] Server rejected writeup for {}.", action.vm),
                ),
                crate::modules::writeups::UploadVerdict::NotFound => (
                    vec![(ReportKind::Failure, format!("Machine '{}' not found", action.vm))],
                    false,
                    format!("[!] Machine '{}' not found.", action.vm),
                ),
                crate::modules::writeups::UploadVerdict::Unknown(ref body) => (
                    vec![(ReportKind::Info, format!("Unknown response: {body}"))],
                    false,
                    format!("[?] Unknown response: {body}"),
                ),
            };
            Ok(ActionReport {
                title: format!(" Writeup results — {} ", action.vm),
                entries,
                changed,
                status,
            })
        }
    }
}

/// Fetches every dataset the dashboard shows: profile stats + accepted
/// writeups, and the pwned catalog for gauges & pending machines.
/// No terminal output here — the TUI owns the screen and reports progress
/// through its footer (`⟳ Loading data...` / `⟳ Refreshing data...`).
async fn fetch_tui_data(sessions: &SessionCache) -> Result<TuiData> {
    let session = sessions.session();

    let stats = StatsManager::new(session.clone())
        .get_stats(&sessions.username)
        .await?;

    let scraper = MachineScraper::new(session.clone());
    let mut catalog = machine::fetch_catalog(&scraper, "all").await?;
    machine::sync_pwned_status(&scraper, &mut catalog).await?;

    let total_vms = catalog.len() as u64;
    let pwned_vms = catalog.iter().filter(|m| m.status != "TO HACK").count() as u64;

    let difficulty = |name: &str| -> (u64, u64) {
        let matching: Vec<&crate::modules::machines::Machine> = catalog
            .iter()
            .filter(|m| m.difficulty.eq_ignore_ascii_case(name))
            .collect();
        let pwned = matching.iter().filter(|m| m.status != "TO HACK").count() as u64;
        (pwned, matching.len() as u64)
    };

    let uploaded: std::collections::HashSet<String> = stats
        .accepted_writeups
        .iter()
        .map(|w| w.vm.to_lowercase())
        .collect();
    let pending: Vec<String> = catalog
        .iter()
        .filter(|m| m.status != "TO HACK" && !uploaded.contains(&m.name.to_lowercase()))
        .map(|m| m.name.clone())
        .collect();

    // Release schedule is nice-to-have: a failure here must not blank out
    // the whole dashboard, so it degrades to an empty tab.
    let releases = ReleaseScraper::new(session.clone())
        .get_releases()
        .await
        .unwrap_or_default();

    Ok(TuiData {
        stats,
        progress: vec![
            ("Total VMs".to_string(), pwned_vms, total_vms),
            ("Beginner".to_string(), difficulty("beginner").0, difficulty("beginner").1),
            (
                "Intermediate".to_string(),
                difficulty("intermediate").0,
                difficulty("intermediate").1,
            ),
            ("Advanced".to_string(), difficulty("advanced").0, difficulty("advanced").1),
        ],
        pending,
        catalog,
        releases,
    })
}
