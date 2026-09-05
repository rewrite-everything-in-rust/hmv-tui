//! UI elements: ASCII banner and package metadata.

use console::style;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR: &str = "Ouba";
pub const GITHUB_URL: &str = "https://github.com/setyanoegraha/hmv-cli-rust";

const BANNER_ART: &str = r"
 ___  ___  _____ ______   ___      ___
|\  \|\  \|\   _ \  _   \|\  \    /  /|
\ \  \\\  \ \  \\\__\ \  \ \  \  /  / /
 \ \   __  \ \  \\|__| \  \ \  \/  / /
  \ \  \ \  \ \  \    \ \  \ \    / /
   \ \__\ \__\ \__\    \ \__\ \__/ /
    \|__|\|__|\|__|     \|__|\|__|/
";

/// Combines the project logo with metadata, mirroring `constants.get_banner`.
pub fn get_banner() -> String {
    format!(
        "{}\n{} {}\n{}\n",
        style(BANNER_ART).cyan().bold(),
        style("   HackMyVM Command Line Interface").white().bold(),
        style(format!("v{VERSION}")).green().bold(),
        style(format!("   Created by {AUTHOR} | {GITHUB_URL}")).dim(),
    )
}
