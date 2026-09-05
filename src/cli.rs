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
    /// Show your personal HackMyVM statistics.
    Stats,
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

    /// Download one or more machines by their name(s) (max 2 in parallel).
    #[arg(short, long, value_name = "NAME", num_args = 1..)]
    pub download: Vec<String>,

    /// Flag token(s) to submit, max 2 (User & Root). Requires -v.
    #[arg(short, long, value_name = "FLAG", num_args = 1..)]
    pub flag: Vec<String>,

    /// Target VM name (Required for -f and -w).
    #[arg(short, long, value_name = "NAME")]
    pub vm: Option<String>,

    /// Fetch community writeups for a machine (Requires -v).
    #[arg(short, long)]
    pub writeups: bool,

    /// Submit a writeup URL for the target VM (Requires -v and -w).
    #[arg(long, value_name = "URL")]
    pub upload: Option<String>,

    /// Show the upcoming machine release schedule.
    #[arg(short = 'r', long)]
    pub release: bool,
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
7. Show upcoming release schedule:              hmv machine -r
8. Download a machine:                          hmv machine -d <name>
9. Download multiple machines (max 2 parallel): hmv machine -d <name1> -d <name2>
10. Get community writeups:                     hmv machine -v <name> -w
11. Submit a flag:                              hmv machine -v <name> -f <flag>
12. Submit user & root flags:                   hmv machine -v <name> -f <flag1> -f <flag2>
13. Submit your writeup:                        hmv machine -v <name> -w --upload <url>

Personal Statistics:
    Show rank, points, trophies and progress:   hmv stats";
