//! CLI definition using clap derive. Mirrors the options of hmv/main.py.

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "hmv",
    version,
    about = "HMV-CLI - HackMyVM Advanced Versatile Operations CLI Toolkit",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Configure your HackMyVM credentials securely.
    Config,
    /// Manage and interact with HackMyVM machines.
    Machine(MachineArgs),
}

#[derive(Args, Debug)]
#[command(after_help = MACHINE_EXAMPLES)]
pub struct MachineArgs {
    /// List machines with pagination (Local pagination if filtered).
    #[arg(short, long)]
    pub list: bool,

    /// Fetch and display ALL machines in a single table.
    #[arg(short, long)]
    pub all: bool,

    /// Filter: beginner, intermediate, advanced, windows, linux, size, hacked, all.
    #[arg(short, long, value_name = "FILTER")]
    pub sort: Option<String>,

    /// Search for a specific machine by name.
    #[arg(short, long, value_name = "NAME")]
    pub name: Option<String>,

    /// Page number (Default: 1).
    #[arg(short, long, default_value_t = 1, value_name = "NUMBER")]
    pub page: usize,

    /// Download a machine by its name.
    #[arg(short, long, value_name = "NAME")]
    pub download: Option<String>,

    /// Flag token to submit.
    #[arg(short, long, value_name = "FLAG")]
    pub flag: Option<String>,

    /// Target VM name (Required for -f and -w).
    #[arg(short, long, value_name = "NAME")]
    pub vm: Option<String>,

    /// Fetch community writeups for a machine (Requires -v).
    #[arg(short, long)]
    pub writeups: bool,
}

pub const MACHINE_EXAMPLES: &str = "\
Usage Examples:

1. List machines (Default first 20):           hmv machine -l
2. List specific page:                          hmv machine -l -p <number>
3. Display ALL machines in one single table:    hmv machine -a
4. Search for a machine by name:                hmv machine -n <name>
5. Filter by difficulty or OS:                  hmv machine -s <beginner|intermediate|advanced>
                                                hmv machine -s <linux|windows> -a
6. Sort all machines by size:                   hmv machine -s size -a
7. Download a machine:                          hmv machine -d <name>
8. Get community writeups:                      hmv machine -v <name> -w
9. Submit a flag:                               hmv machine -v <name> -f <flag>";
