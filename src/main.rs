mod commands;
mod config;
mod download;
mod mega;
mod modules;
mod tui;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        None => commands::tui_cmd().await,
        Some("--version" | "-V") => {
            println!("hmv {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help" | "-h") => {
            print_help();
            Ok(())
        }
        Some(command) => {
            eprintln!(
                "[!] Unknown command '{command}'. HMV-TUI is dashboard-only since v1.0.0 — run 'hmv' to open the dashboard."
            );
            std::process::exit(1);
        }
    };

    if let Err(error) = result {
        eprintln!("[!] {error:#}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "HMV-TUI v{} — HackMyVM Advanced Versatile Operations Toolkit",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("  hmv            open the interactive dashboard");
    println!("  hmv --version  print the version");
    println!("  hmv --help     show this help");
    println!();
    println!("Everything else lives inside the dashboard: account management (a),");
    println!("flag submission (f), downloads (d), writeups (w/u) and releases.");
    println!("The classic CLI subcommands were removed in v1.0.0.");
}
