# HMV-CLI (Rust)

### HackMyVM Advanced Versatile Operations CLI Toolkit

**HMV-CLI** is a modern command-line toolkit designed specifically for the HackMyVM community. It allows you to search for machines, download VMs, submit flags, and view community writeups efficiently directly from your terminal with fast performance and an intuitive interface.

This is the **Rust rewrite** of the original Python [HMV-CLI](https://github.com/setyanoegraha/hackmyvm-commandlineinterface) — a single static binary, no Python runtime required.

---

## Key Features

* **Secure Auth**: Securely stores your credentials using the system vault (Linux keyutils / Secret Service, Windows Credential Manager, macOS Keychain) via the `keyring` library. Only the username touches `~/.hmv/config.json`.
* **Personal Statistics** (`hmv stats`): rank, title, country, points, roots/users, challenges, writeups, trophies and visual progress bars per difficulty.
* **Machine Management**:
    * Smart paginated machine listing (max 3 concurrent page fetches).
    * Instant machine search by name.
    * Filters for difficulty (beginner, intermediate, advanced) or OS (linux/windows).
    * Global "Pwned" status synchronization to track your progress.
    * Upcoming machine **release schedule** (`hmv machine -r`).
* **High-Speed Downloader**: Downloads VMs directly from MEGA with accurate progress bars — up to **2 VMs in parallel** (`-d a -d b`). Files are decrypted on the fly (AES-128-CTR) and **integrity-verified with the MEGA per-chunk MAC** before being moved out of the `.part` staging file.
* **Flag Submission**: Submit flags with clear visual feedback — including **dual user/root flag submission** in one command (`-f <user> -f <root>`, max 2, concurrent).
* **Writeups Access**: View community writeups (articles or videos) without opening a browser. **Submit your own writeup** directly from the CLI (`-w --upload <url>`) once both flags are pwned.
* **Interactive Dashboard** (`hmv tui`): a ratatui-powered TUI with your stats & progress gauges, all accepted writeups (filterable, open links in the browser), the list of pwned machines still missing a writeup, and the **full machine catalog** — submit flags (`f`) and writeup links (`u`) directly from popups without leaving the dashboard.

---

## Prerequisites

* **OS**: Linux (primary target for v0.2.0; Windows/macOS builds should work but are untested).
* An active account on [HackMyVM](https://hackmyvm.eu/).
* A Secret Service provider on Linux (e.g. GNOME Keyring / KWallet) for credential storage.

---

## Installation

### 1. From source (Recommended)

```bash
git clone https://github.com/setyanoegraha/hmv-cli-rust.git
cd hmv-cli-rust
cargo install --path .
```

### 2. From git directly

```bash
cargo install --git https://github.com/setyanoegraha/hmv-cli-rust.git
```

> Requires the Rust toolchain (1.85+): https://rustup.rs

---

## Initial Configuration

After installation, you must run the configuration command to save your account:

```bash
hmv config
```

**NOTE:** Your password is encrypted by the operating system and is not stored in plain text.

---

## Usage Guide

### General Commands

| Command | Function |
| :--- | :--- |
| `hmv` | Show banner and help menu. |
| `hmv stats` | Show your personal stats: rank, points, trophies and progress. |
| `hmv machine -l` | Show the latest 20 machines from HackMyVM. |
| `hmv machine -a` | Show the entire machine catalog in one large table. |
| `hmv machine -n <name>` | Search for machines by name (e.g., `hmv machine -n hunter`). |
| `hmv machine -s <filter>` | Sorting / Filtering the machines by some category (e.g., `hmv machine -s beginner`). |
| `hmv machine -r` | Show the upcoming machine release schedule. |
| `hmv machine -d <name>` | Download for machine by name (e.g., `hmv machine -d redacted`). |
| `hmv machine -d <a> -d <b>` | Download two machines in parallel. |
| `hmv machine -v <name> -f <flag>` | Submit flag for some machine (e.g, `hmv machine -v fuzzz -f flag{abc}`). |
| `hmv machine -v <name> -f <f1> -f <f2>` | Submit user & root flags concurrently. |
| `hmv machine -v <name> -w` | See write-up for machine from community (e.g., `hmv machine -v skid -w`). |
| `hmv machine -v <name> -w --upload <url>` | Submit your writeup link for a machine (requires both flags submitted). |

### Personal Statistics

```bash
hmv stats
```

```text
User: demouser #38 | Title: [WTF] | Country: [ID] | Points: 1767 | Loved: ❤️ 9
-------------------------------------------------------
[ Stats ]
Total Roots   : 166
...

[ Trophies ]
🏆 [vfinisher] [noobchad] [starter] ...

[ Progress ]
Total VMs     [#########-----------] 166 / 371
Beginner      [###################-] 163 / 171
Intermediate  [--------------------] 2 / 136
Advanced      [--------------------] 1 / 64
```

### Release Schedule

```bash
hmv machine -r
```

### Interactive Dashboard (TUI)

```bash
hmv tui
```

Three tabs driven entirely by the keyboard:

| Keys | Action |
| :--- | :--- |
| `Tab` / `←` `→` | Switch between **Stats**, **Writeups**, **Pending** and **Machines** |
| `↑` `↓` / `j` `k` | Move selection |
| `/` | Filter the current list (type to narrow, `Esc` clears) |
| `f` | **Machines only** — flag popup with User & Root fields (fill one or both, sent in parallel) |
| `u` | **Pending only** — submit a writeup URL for the pwned machine |
| `Enter` | Open the selected writeup link in your browser |
| `r` | Re-fetch all data |
| `q` / `Esc` / `Ctrl-C` | Quit |

- **Stats** — identity, achievements, trophies and animated progress gauges per difficulty.
- **Writeups** — every writeup accepted on HackMyVM (VM, language, link).
- **Pending** — machines you fully pwned (user + root flags) that still have no accepted writeup.
- **Machines** — the complete catalog (VM, difficulty, creator, size, status) with color-coded difficulty.

Actions submitted from the TUI show their verdict in the footer (`[✓] You hacked X!`, `[!] Wrong flag`, `[=] already submitted`, ...) and trigger an automatic data refresh when they change your progress.

### VM Interaction

* **Download VM:**
    ```bash
    hmv machine -d <vm_name>
    ```
* **View Writeups:**
    ```bash
    hmv machine -v <vm_name> -w
    ```
* **Submit Flag:**
    ```bash
    hmv machine -v <vm_name> -f <flag_token>
    ```

### Show All Machine based on Filtering & Sorting

* **By OS:** `hmv machine -s linux -a`
* **By Difficulty:** `hmv machine -s beginner -a`
* **By Size:** `hmv machine -s size -a`

### Updating

```bash
cargo install --git https://github.com/setyanoegraha/hmv-cli-rust.git --force
```

### Uninstallation & Cleanup

```bash
cargo uninstall hmv
```

Cleaning up Remaining Data

HMV stores configuration in the `~/.hmv/` directory and the password in the system vault. Delete the folder (and the `hmv-cli` keyring entry) to clear all local data:
- Linux: `~/.hmv`

---

## Security Notes

* No HackMyVM or MEGA credentials are ever sent to MEGA — public-file downloads use MEGA's anonymous API.
* Downloaded archives are AES-CTR decrypted in a streaming fashion and verified against the MEGA MAC embedded in the file key; corrupt/interrupted downloads are rejected and never leave a partial `.zip` behind.
* All traffic uses HTTPS (MEGA storage URLs are upgraded from `http://` to `https://`).

---

## Official Links

- Website: [hackmyvm.eu](https://hackmyvm.eu)
- Discord: [Official HackMyVM](https://discord.com/invite/DxDFQrJ)
- Legacy Python version: [hackmyvm-commandlineinterface](https://github.com/setyanoegraha/hackmyvm-commandlineinterface)

## Acknowledgements

A massive thanks and maximum respect to the HackMyVM community, the staff, and all the machine creators. This toolkit exists because of the incredible platform and community you've built for cybersecurity enthusiasts to learn, share, and grow.

MEGA crypto implementation ported from [mega.py](https://github.com/odwyersoftware/mega.py) (odwyersoftware).

---

Made with ❤️ by [Ouba](https://github.com/setyanoegraha).

*Happy Hacking on HackMyVM!*
