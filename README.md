# HMV-CLI (Rust)

### HackMyVM Advanced Versatile Operations CLI Toolkit

**HMV-CLI** is a modern toolkit for the HackMyVM community: search for machines, download VMs, submit flags, and view community writeups — all from an interactive terminal dashboard.

Since **v1.0.0**, HMV-CLI is **dashboard-only**: running `hmv` opens the interactive TUI, and all account management (first-time setup, switching accounts, logout) happens inside it. The classic CLI subcommands (`hmv stats`, `hmv machine ...`, `hmv config`) were removed — everything lives in the dashboard now.

This is the **Rust rewrite** of the original Python [HMV-CLI](https://github.com/setyanoegraha/hackmyvm-commandlineinterface) — a single static binary, no Python runtime required.

---

## Key Features

* **One command**: bare `hmv` opens the dashboard — your stats, accepted writeups, pending writeups, the full machine catalog and downloads in one screen.
* **In-app account management**: first-run setup, account switching and logout via popups (`a`) — no extra commands to remember.
* **Secure Auth**: credentials are stored using the system vault (Secret Service on Linux, Credential Manager on Windows, Keychain on macOS) via the `keyring` library. Only the username and your last download folder touch `~/.hmv/config.json`.
* **Personal Statistics**: rank, title, country, points, roots/users, challenges, writeups, trophies and animated progress gauges per difficulty.
* **Machine Management**: the complete catalog with color-coded difficulty, instant `/` filtering (name, difficulty, creator, status) and global "Pwned" status synchronization.
* **High-Speed Downloader**: downloads VMs directly from MEGA — up to **2 in parallel** (extra ones queue). Files are decrypted on the fly (AES-128-CTR) and **integrity-verified with the MEGA per-chunk MAC** before being moved out of the `.part` staging file.
* **Flag Submission**: dual user/root flag popup (`f`), both fields sent in parallel, status-aware (PWNED machines show a read-only box, DONE machines a "one remains" notice).
* **Writeups Access**: read community writeups (`w`) and submit your own (`u`) without leaving the dashboard.

---

## Prerequisites

* **OS**: Linux (primary target; Windows/macOS builds should work but are untested).
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

Nothing to configure by hand — just run:

```bash
hmv
```

On the very first run the dashboard opens a **Configure HackMyVM** popup:

1. Type your HackMyVM **username**, then press `Tab` / `↓`.
2. Type your **password** (hidden as `•••`), then press `Enter`.

Credentials are validated with a real login **before** anything is saved. If the login fails, the popup reopens with your username kept; `Esc` quits the app.

---

## Usage Guide

```bash
hmv
```

That's the whole command surface. `hmv --version` and `hmv --help` exist for completeness; any other argument is rejected with a hint back to the dashboard.

Five keyboard-driven tabs:

| Keys | Action |
| :--- | :--- |
| `Tab` / `←` `→` | Switch between **Stats**, **Writeups**, **Pending**, **Machines** and **Releases** |
| `↑` `↓` / `j` `k` | Move selection |
| `g` / `Home` | Jump to the top of the list |
| `/` | Filter the current list (type to narrow, `Enter` keeps it, `Esc` clears & exits) |
| `a` | **Account popup** — shows the active account: `Enter` opens the login popup to switch accounts, `l` logs out, `Esc` closes |
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

### Account Management

Press `a` anywhere in the dashboard:

- **`Enter` — switch account**: opens the login popup with the current username prefilled. Enter the new credentials; they are validated by a real login before replacing the stored account, and the dashboard reloads with the new profile.
- **`l` — logout**: removes the password from the system vault and the username from `~/.hmv/config.json` (your download-folder preference is kept), clears the dashboard and shows the login popup. Sign in with another account, or `Esc` to quit.
- Running downloads are never affected — they use public MEGA links, not your session.

Actions submitted from the TUI show their verdicts in a result popup that stays until dismissed (`User flag: ✓ ACCEPTED`, `Root flag: ✗ REJECTED`, ...) and trigger an automatic data refresh on close when your progress changed.

Downloads run in the background (max 2 in parallel, extra ones queue): the Downloads overlay shows live gauges, speed and the final path; downloads keep running when the overlay is closed; quitting while a download is active asks for a second `q`.

### Where Your Data Lives

- `~/.hmv/config.json` — your username and the last download folder. Nothing else.
- System vault — your password, under the `hmv-cli` service. Never on disk in plain text.

---

## Updating

```bash
cargo install --git https://github.com/setyanoegraha/hmv-cli-rust.git --force
```

### Uninstallation & Cleanup

```bash
cargo uninstall hmv
```

HMV stores configuration in the `~/.hmv/` directory and the password in the system vault. Delete the folder (and the `hmv-cli` vault entry) to clear all local data:
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
