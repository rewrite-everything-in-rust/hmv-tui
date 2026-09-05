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
use crate::modules::session::login;
use crate::modules::stats::{ProfileStats, StatsManager};
use crate::modules::writeups::WriteupManager;
use crate::tui::{AppState, TuiData};

pub async fn config_cmd() -> Result<()> {
    println!("{} HackMyVM Account Configuration", style("[*]").blue().bold());
    let username = config::prompt_username()?;
    let password = config::prompt_password()?;
    let cfg = ConfigManager::new();
    cfg.save_credentials(&username, &password)
}

pub async fn tui_cmd() -> Result<()> {
    let data = fetch_tui_data().await?;
    crate::tui::run(AppState::new(data), || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(fetch_tui_data())
        })
    })
}

/// Fetches every dataset the dashboard shows: profile stats + accepted
/// writeups, and the pwned catalog for gauges & pending machines.
async fn fetch_tui_data() -> Result<TuiData> {
    let cfg = ConfigManager::new();
    let session = login(&cfg).await?;
    let (username, _) = cfg.load_credentials()?;

    let fetching = crate::ui::spinner("Fetching dashboard data...");
    let stats = StatsManager::new(session.clone()).get_stats(&username).await?;

    let scraper = MachineScraper::new(session.clone());
    let mut catalog = machine::fetch_catalog(&scraper, "all").await?;
    machine::sync_pwned_status(&scraper, &mut catalog).await?;
    fetching.finish_and_clear();

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