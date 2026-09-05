mod banner;
mod cli;
mod commands;
mod config;
mod download;
mod mega;
mod modules;
mod ui;

use clap::{CommandFactory, Parser};
use console::style;

use crate::cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(error) = run(cli).await {
        eprintln!("{} {error:#}", style("[!]").red().bold());
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        None => {
            println!("{}", banner::get_banner());
            Cli::command().print_help()?;
        }
        Some(Commands::Config) => commands::config_cmd().await?,
        Some(Commands::Machine(args)) => {
            println!("{}", banner::get_banner());
            commands::machine_cmd(args).await?;
        }
    }
    Ok(())
}