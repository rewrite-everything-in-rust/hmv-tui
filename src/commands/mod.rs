//! Subcommand orchestration modules.

pub mod machine;

use anyhow::Result;
use clap::CommandFactory;
use console::style;

use crate::cli::{Cli, MachineArgs};
use crate::config::{self, ConfigManager};
use crate::download::DownloadManager;
use crate::modules::flag::FlagManager;
use crate::modules::machines::MachineScraper;
use crate::modules::releases::ReleaseScraper;
use crate::modules::session::{login, HmvSession};
use crate::modules::stats::{ProfileStats, StatsManager};
use crate::modules::writeups::WriteupManager;
use crate::tui::{ActionReport, TuiAction, TuiData};

/// Reusable authenticated session for the TUI's lifetime. Cloning an
/// `HmvSession` is cheap (shared connection pool), so one login serves
/// every background action.
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

pub async fn tui_cmd() -> Result<()> {
    // Enter the TUI instantly with an empty state; the first fetch runs
    // inside the event loop with a `⟳ Loading data...` indicator.
    let sessions = SessionCache::new().await?;
    crate::tui::run(
        crate::tui::AppState::loading(),
        || {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(fetch_tui_data(&sessions))
            })
        },
        |action| {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(run_tui_action(&sessions, action))
            })
        },
    )
}

/// Executes a user action from a TUI popup. Returns the result popup
/// content: verdicts labeled with the original field (User/Root flag),
/// `changed` telling whether dashboard data must be refreshed after the
/// popup closes.
async fn run_tui_action(sessions: &SessionCache, action: TuiAction) -> Result<ActionReport> {
    if action.kind == crate::tui::PopupKind::Download {
        anyhow::bail!("downloads are spawned directly, not through run_action");
    }
    match action.kind {
        crate::tui::PopupKind::Download => unreachable!("handled by the event loop"),
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

pub async fn config_cmd() -> Result<()> {
    println!("{} HackMyVM Account Configuration", style("[*]").blue().bold());
    let username = config::prompt_username()?;
    let password = config::prompt_password()?;
    let cfg = ConfigManager::new();
    cfg.save_credentials(&username, &password)
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

pub async fn stats_cmd() -> Result<()> {
    let cfg = ConfigManager::new();
    let session = login(&cfg).await?;
    let (username, _) = cfg.load_credentials()?;

    let fetching = crate::ui::spinner("Fetching your profile stats...");
    let stats = StatsManager::new(session.clone()).get_stats(&username).await?;
    fetching.finish_and_clear();

    let scraper = MachineScraper::new(session);
    let progress = crate::ui::spinner("Building difficulty progress...");
    let mut catalog = machine::fetch_catalog(&scraper, "all").await?;
    machine::sync_pwned_status(&scraper, &mut catalog).await?;
    progress.finish_and_clear();

    let progress = Progress {
        total_vms: catalog.len() as u64,
        pwned_vms: catalog.iter().filter(|m| m.status != "TO HACK").count() as u64,
        beginner: difficulty_counts(&catalog, "beginner"),
        intermediate: difficulty_counts(&catalog, "intermediate"),
        advanced: difficulty_counts(&catalog, "advanced"),
    };

    print_stats(&stats, &progress);
    Ok(())
}

#[derive(Debug)]
struct Progress {
    total_vms: u64,
    pwned_vms: u64,
    beginner: (u64, u64),
    intermediate: (u64, u64),
    advanced: (u64, u64),
}

fn difficulty_counts(catalog: &[crate::modules::machines::Machine], difficulty: &str) -> (u64, u64) {
    let matching: Vec<&crate::modules::machines::Machine> = catalog
        .iter()
        .filter(|m| m.difficulty.eq_ignore_ascii_case(difficulty))
        .collect();
    let pwned = matching
        .iter()
        .filter(|m| m.status != "TO HACK")
        .count() as u64;
    (pwned, matching.len() as u64)
}

fn progress_bar(value: u64, total: u64, width: usize) -> String {
    let filled = if total == 0 {
        0
    } else {
        ((value as f64 / total as f64) * width as f64)
            .round()
            .clamp(0.0, width as f64) as usize
    };
    format!(
        "[{}{}] {} / {}",
        "#".repeat(filled),
        "-".repeat(width - filled),
        value,
        total
    )
}

fn print_stats(stats: &ProfileStats, progress: &Progress) {
    let username = style(&stats.username).white().bold();
    let rank = stats
        .rank
        .as_ref()
        .map(|r| format!(" {r}"))
        .unwrap_or_default();
    let title = stats
        .title
        .as_ref()
        .map(|t| format!(" | Title: {t}"))
        .unwrap_or_default();
    let country = stats
        .country
        .as_ref()
        .map(|c| format!(" | Country: [{c}]"))
        .unwrap_or_default();

    println!(
        "\nUser: {username}{rank}{title}{country} | Points: {} | Loved: ❤️ {}",
        style(stats.points).green(),
        stats.loved
    );
    println!("{}", style("-".repeat(55)).dim());

    println!("{}", style("[ Stats ]").blue().bold());
    println!("Total Roots   : {}", stats.roots);
    println!("Total Users   : {}", stats.users);
    println!("First Roots   : {}", stats.first_roots);
    println!("First Users   : {}", stats.first_users);
    println!("Challenges    : {}", stats.challenges);
    println!("Writeups      : {}", stats.writeups);

    if !stats.trophies.is_empty() {
        println!("\n{}", style("[ Trophies ]").blue().bold());
        println!(
            "🏆 {}",
            stats.trophies.iter().map(|t| format!("[{t}]")).collect::<Vec<_>>().join(" ")
        );
    }

    println!("\n{}", style("[ Progress ]").blue().bold());
    println!("Total VMs     {}", progress_bar(progress.pwned_vms, progress.total_vms, 20));
    println!(
        "Beginner      {}",
        progress_bar(progress.beginner.0, progress.beginner.1, 20)
    );
    println!(
        "Intermediate  {}",
        progress_bar(progress.intermediate.0, progress.intermediate.1, 20)
    );
    println!(
        "Advanced      {}",
        progress_bar(progress.advanced.0, progress.advanced.1, 20)
    );
}

pub async fn machine_cmd(args: MachineArgs) -> Result<()> {
    let cfg = ConfigManager::new();
    let session = login(&cfg).await?;

    if args.writeups {
        let Some(vm) = args.vm.clone() else {
            anyhow::bail!("Error: Target VM name (-v) is required to fetch writeups.");
        };
        if let Some(url) = &args.upload {
            WriteupManager::new(session.clone())
                .upload(&vm, url)
                .await?;
            return Ok(());
        }
        WriteupManager::new(session.clone()).get_writeups(&vm).await?;
        return Ok(());
    }

    if let Some(_url) = &args.upload {
        anyhow::bail!("Error: Writeup submission requires -v <vm> and -w.");
    }

    if !args.flag.is_empty() {
        let Some(vm) = &args.vm else {
            anyhow::bail!("Error: Target VM name (-v) is required.");
        };
        FlagManager::new(session.clone()).submit_batch(vm, &args.flag).await?;
        return Ok(());
    }

    if args.release {
        let releases = ReleaseScraper::new(session.clone()).get_releases().await?;
        if releases.is_empty() {
            anyhow::bail!("No upcoming releases scheduled.");
        }
        print_releases(&releases);
        return Ok(());
    }

    if let Some(vm) = &args.vm {
        anyhow::bail!(
            "Error: Target VM '{}' specified without an action.\n{} Use -f <flag> to submit or -w to fetch writeups.",
            vm,
            style("[*]").yellow()
        );
    }

    if !args.download.is_empty() {
        DownloadManager::new().download_vms(&args.download).await?;
        return Ok(());
    }

    if args.list || args.all || args.sort.is_some() || args.name.is_some() {
        let scraper = MachineScraper::new(session.clone());
        machine::run(&scraper, args.list, args.all, args.sort, args.name, args.page)
            .await?;
        return Ok(());
    }

    if let Some(machine) = Cli::command().find_subcommand_mut("machine") {
        machine.print_help()?;
    }
    Ok(())
}

fn print_releases(releases: &[crate::modules::releases::Release]) {
    use comfy_table::{presets::UTF8_FULL_CONDENSED, Table};

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_header(vec!["Date", "OS", "VM", "Status"]);

    for release in releases {
        let os_str = if release.os == "windows" {
            style(&release.os).cyan().to_string()
        } else {
            style(&release.os).yellow().to_string()
        };
        let status = if release.released {
            style("RELEASED").green().to_string()
        } else {
            style("UPCOMING").magenta().to_string()
        };
        table.add_row(vec![
            style(&release.date).dim().to_string(),
            os_str,
            style(&release.name).white().bold().to_string(),
            status,
        ]);
    }

    println!(
        "\n{}\n",
        style("Next Machine Releases").blue().bold()
    );
    println!("{table}");
    println!(
        "\n{} {}",
        style("[*]").dim(),
        style("Schedule can change at any time.").dim()
    );
}