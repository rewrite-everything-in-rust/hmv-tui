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
use crate::modules::session::login;
use crate::modules::writeups::WriteupManager;

pub async fn config_cmd() -> Result<()> {
    println!("{} HackMyVM Account Configuration", style("[*]").blue().bold());
    let username = config::prompt_username()?;
    let password = config::prompt_password()?;
    let cfg = ConfigManager::new();
    cfg.save_credentials(&username, &password)
}

pub async fn machine_cmd(args: MachineArgs) -> Result<()> {
    let cfg = ConfigManager::new();
    let session = login(&cfg).await?;

    if args.writeups {
        let Some(vm) = args.vm.clone() else {
            anyhow::bail!("Error: Target VM name (-v) is required to fetch writeups.");
        };
        WriteupManager::new(session.clone()).get_writeups(&vm).await?;
        return Ok(());
    }

    if let Some(flag) = &args.flag {
        let Some(vm) = &args.vm else {
            anyhow::bail!("Error: Target VM name (-v) is required.");
        };
        FlagManager::new(session.clone()).submit(vm, flag).await?;
        return Ok(());
    }

    if let Some(vm) = &args.vm {
        anyhow::bail!(
            "Error: Target VM '{}' specified without an action.\n{} Use -f <flag> to submit or -w to fetch writeups.",
            vm,
            style("[*]").yellow()
        );
    }

    if let Some(name) = &args.download {
        DownloadManager::new(session.clone()).download_vm(name).await?;
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