# HMV-CLI (Rust)

### HackMyVM Advanced Versatile Operations CLI Toolkit

**HMV-CLI** is a modern toolkit designed specifically for the HackMyVM community. It lets you search for machines, download VMs, submit flags, and view community writeups efficiently, directly from your terminal.

Since **v0.7.0**, running `hmv` opens the **interactive dashboard (TUI)** — that is the primary interface. The classic CLI subcommands (`hmv stats`, `hmv machine ...`) remain fully available for scripting and one-liners.

This is the **Rust rewrite** of the original Python [HMV-CLI](https://github.com/setyanoegraha/hackmyvm-commandlineinterface) — a single static binary, no Python runtime required.

---

## Key Features

* **Dashboard-first**: bare `hmv` opens the interactive TUI dashboard — your stats, accepted writeups, pending writeups, the full machine catalog and downloads in one screen.
* **First-run config popup**: no manual setup — the dashboard walks you through account configuration on first launch (or when the stored password stopped working).
* **Secure Auth**: credentials are stored using the system vault (Linux keyutils / Secret Service, Windows Credential Manager, macOS Keychain) via the `keyring` library. Only the username and your last download folder touch `~/.hmv/config.json`.
* **Personal Statistics** (`hmv stats`): rank, title, country, points, roots/users, challenges, writeups, trophies and visual progress bars per difficulty.
* **Machine Management**:
    * Smart paginated machine listing (max 3 concurrent page fetches).
    * Instant machine search by name.
    * Filters for difficulty (beginner, intermediate, advanced) or OS (linux/windows).
    * Global "Pwned" status synchronization to track your progress.
    * Upcoming machine **release schedule** (`hmv machine -r`).
* **High-Speed Downloader**: Downloads VMs directly from MEGA with accurate progress bars — up to **2 VMs in parallel** (`-d a -d b`). Files are decrypted on the fly (AES-128-CTR) and **integrity-verified with the MEGA per-chunk MAC** before being moved out of the `.part` staging file.
* **Flag Submission**: Submit flags with clear visual feedback — including **dual user/root flag submission** in one command (`-f <user> -f <root>`, max 2, concurrent).
* **Writeups Access**: View community writeups (articles or videos) without opening a browser. **Submit your own writeup** directly (`-w --upload <url>`) once both flags are pwned.

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

## First Setup

Nothing to configure by hand — just run `hmv`:

```bash
hmv
```

On the very first run (or when the stored password no longer works) the dashboard opens a **Configure HackMyVM** popup:

1. Type your HackMyVM **username**, then press `Tab` / `↓`.
2. Type your **password** (hidden as `•••`), then press `Enter`.

Credentials are validated with a real login **before** anything is saved. If the login fails, the popup reopens with your username kept; `Esc` quits the app.

Prefer the terminal, or need to switch accounts later? Use the classic command:

```bash
hmv config
```

**Where your data lives:** the username and the last download folder are stored in `~/.hmv/config.json`; the password never touches disk — it goes into your OS vault (keyutils / Secret Service on Linux, Credential Manager on Windows, Keychain on macOS).

---

## Usage Guide

Two ways to drive HMV-CLI: the **interactive dashboard** (primary) and the **CLI subcommands** (scripts & one-liners). Both use the same stored account.

### 1. Interactive Dashboard — just run `hmv`

```bash
hmv
```

(`hmv tui` does the same.) Five keyboard-driven tabs:

| Keys | Action |
| :--- | :--- |
| `Tab` / `←` `→` | Switch between **Stats**, **Writeups**, **Pending**, **Machines** and **Releases** |
| `↑` `↓` / `j` `k` | Move selection |
| `g` / `Home` | Jump to the top of the list |
| `/` | Filter the current list (type to narrow, `Enter` keeps it, `Esc` clears & exits) |
| `f` | **Machines only** — flag popup with User & Root fields (fill one or both, sent in parallel). Results show in a popup (`✓ ACCEPTED` / `✗ REJECTED` per field); a data refresh runs after you close it. Status-aware: PWNED machines show a read-only "Already PWNED" box, machines with one flag in get a "one remains" notice. |
| `d` | **Machines only** — download popup: pick the destination folder (remembered across sessions), MEGA link resolved automatically, streaming download with live progress in the Downloads overlay. MAC-verified before the file lands. |
| `w` | **Machines & Pending** — community writeups popup for the selected machine: `j`/`k` to select, `Enter` opens the link in your browser, `Esc` closes. |
| `u` | **Pending only** — submit a writeup URL for the pwned machine (result popup as well). |
| `o` | Toggle the **Downloads** overlay (live gauges, speed, final paths). Closing it never stops running downloads. |
| `c` | **In the Downloads overlay** — cancel the most recent active download (the staged `.part` file is cleaned). |
| `Enter` | Open the selected writeup link in your browser (**Writeups tab** and writeups popup). |
| `r` | Re-fetch all data |
| `q` / `Esc` / `Ctrl-C` | Quit (with active downloads, the first `q` lists them — press `q` again to abort). |

- **Stats** — identity, achievements, trophies and animated progress gauges per difficulty.
- **Writeups** — every writeup accepted on HackMyVM (VM, language, link).
- **Pending** — machines you fully pwned (user + root flags) that still have no accepted writeup.
- **Machines** — the complete catalog (VM, difficulty, creator, size, status) with color-coded difficulty.
- **Releases** — the upcoming machine release schedule (RELEASED / UPCOMING).

Actions submitted from the TUI show their verdicts in a result popup that stays until dismissed (`User flag: ✓ ACCEPTED`, `Root flag: ✗ REJECTED`, ...) and trigger an automatic data refresh on close when your progress changed.

Downloads run in the background (max 2 in parallel, extra ones queue): the Downloads overlay shows live gauges, speed and the final path; downloads keep running when the overlay is closed; quitting while a download is active asks for a second `q`.

### 2. CLI for Scripting & One-liners

| Command | Function |
| :--- | :--- |
| `hmv` / `hmv tui` | Launch the interactive dashboard. |
| `hmv config` | (Re)configure your HackMyVM account. |
| `hmv stats` | Show your personal stats: rank, points, trophies and progress. |
| `hmv machine -l` | Show the latest 20 machines (`-p <n>` for another page). |
| `hmv machine -a` | Show the entire machine catalog in one large table. |
| `hmv machine -n <name>` | Search for machines by name (e.g., `hmv machine -n hunter`). |
| `hmv machine -s <filter>` | Filter / sort by category: `beginner`, `intermediate`, `advanced`, `linux`, `windows`, `size`, `hacked`, `all`. |
| `hmv machine -r` | Show the upcoming machine release schedule. |
| `hmv machine -d <name>` | Download a machine by name (e.g., `hmv machine -d redacted`). |
| `hmv machine -d <a> -d <b>` | Download two machines in parallel. |
| `hmv machine -v <name> -f <flag>` | Submit a flag (e.g., `hmv machine -v fuzzz -f flag{abc}`). |
| `hmv machine -v <name> -f <f1> -f <f2>` | Submit user & root flags concurrently. |
| `hmv machine -v <name> -w` | View community writeups for a machine (e.g., `hmv machine -v skid -w`). |
| `hmv machine -v <name> -w --upload <url>` | Submit your writeup link for a machine (requires both flags submitted). |

#### Personal Statistics

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

#### Release Schedule

```bash
hmv machine -r
```

#### VM Interaction

```bash
hmv machine -d <vm_name>              # Download a VM
hmv machine -v <vm_name> -w           # View community writeups
hmv machine -v <vm_name> -f <flag>    # Submit a flag
```

#### Filter & Sort Examples

* **By OS:** `hmv machine -s linux -a` or `hmv machine -s windows -a`
* **By difficulty:** `hmv machine -s beginner -a`
* **By size:** `hmv machine -s size -a`
* **Only machines you pwned:** `hmv machine -s hacked -a`

---

## Updating

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
