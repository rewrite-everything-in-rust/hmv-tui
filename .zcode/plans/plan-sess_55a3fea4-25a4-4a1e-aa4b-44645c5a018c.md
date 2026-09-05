# HMV-CLI v1.0.0 — TUI-only + Manajemen Akun di Dashboard

## Tujuan
1. Hapus seluruh CLI — `hmv` (tanpa argumen) menjadi satu-satunya entry point ke dashboard TUI.
2. Manajemen akun di dalam TUI: tombol `a` membuka **popup Akun** (`Enter` = ganti akun via popup login, `l` = logout, `Esc` = tutup). Popup login first-run (v0.7.0) tetap jadi jalur login.
3. Logout = hapus permanen kredensial: password dari OS vault (keyring `delete_password`), username dari `~/.hmv/config.json`. **`download_dir` tetap tersimpan**. Dashboard dikosongkan → popup login muncul. Download yang sedang berjalan **tidak terganggu** (tidak memakai sesi).
4. Ganti akun = alur config existing: divalidasi dengan login sungguhan sebelum disimpan, sesi bersama diganti, data dashboard auto-refresh.

## Perubahan kode

**Argumen & entry point (hapus clap)**
- `main.rs`: parsing argumen manual. Tanpa argumen → `tui_cmd()`. `--version`/`-V` → cetak versi. `--help`/`-h` → help minimal. Argumen lain (termasuk `hmv tui`, `hmv stats`, …) → pesan `[!] Unknown command '...' — HMV-CLI is dashboard-only since v1.0.0. Run 'hmv'.` + exit 1. Error fatal tercetak polos `eprintln!("[!] {error:#}")`. Mod `banner` dan `ui` dihapus.

**File dihapus**
- `src/cli.rs`, `src/banner.rs`, `src/ui.rs`.

**commands/mod.rs (ramping)**
- Hapus: `config_cmd`, `stats_cmd`, `machine_cmd`, `print_stats`, `Progress`, `difficulty_counts`, `progress_bar`, `print_releases`, import Cli.
- Tetap: `SessionCache`, `tui_cmd`, `fetch_tui_data`, `run_tui_action`, `configure_account`.
- Baru: closure `logout` (parameter ke-5 `tui::run`) + helper `logout_account(&SharedSession)` → `ConfigManager::clear_credentials()` + slot sesi → `None`.

**commands/machine.rs**
- Hapus: `run`, `print_table`, `parse_size`, `PER_PAGE`, `DIFFICULTIES`, `CATEGORIES_NEEDING_ALL`.
- Tetap (dipakai TUI): `fetch_catalog`, `sync_pwned_status`, `total_pages_of`, `fetch_remaining_pages`, `CONCURRENCY`.

**tui/mod.rs**
- `PopupKind::Account` + `open_account_popup()` (ditolak bila `needs_config` atau ada popup lain).
- Key normal mode: `a` → popup Akun.
- `handle_key`: cabang khusus popup Akun sebelum cabang popup generik — `Enter` → tutup popup Akun, buka popup login dengan username aktif terisi + notice kontekstual ("Switch account — enter new credentials."); `l` → set `pending_logout`; `Esc`/`q` → tutup.
- Event loop: konsumsi `pending_logout` → panggil closure `logout` → `needs_config = true`, `set_data(TuiData::empty())`, buka popup login (username kosong), status "[✓] Logged out…".
- `open_config_popup` diberi parameter konteks notice (first-run / login gagal / ganti akun) — saat ini notice diturunkan dari presence username, kurang tepat untuk ganti akun.
- Unit test baru: popup Akun terbuka via `a` & ter-gate saat `needs_config`; `Enter` → popup Config dengan username terisi; `l` → `pending_logout`; `Esc` menutup.

**render.rs**
- `draw_popup`: arm `Account` — kotak info hijau: judul `Account — {username}` (dari `data.stats.username`), hint `Enter switch · l logout · Esc close`.
- Footer: hint khusus popup Akun.

**config.rs**
- `ConfigFile.username` menjadi `Option<String>` (logout menulis `None`, `download_dir` dipertahankan).
- Baru: `clear_credentials()` — hapus keyring entry (abaikan `NoEntry`) + tulis ulang config dengan username `None`.
- Hapus `prompt_username`/`prompt_password` dan `println!` console di `save_credentials` (agar tidak merusak layar TUI).

**modules (buang jalur render CLI)**
- `flag.rs`: hapus `submit`/`submit_batch` (console); simpan `check` + `FlagVerdict`.
- `writeups.rs`: hapus `get_writeups`/`upload`/`print_table`/`spinner` (console, comfy-table); simpan `fetch`/`submit`/`parse_writeups`/`UploadVerdict`/test.
- `download.rs`: hapus `DownloadManager` (indicatif); simpan `resolve_mega_link`.

**Cargo.toml**
- `version = "1.0.0"`; buang deps: `clap`, `console`, `indicatif`, `comfy-table`, `rpassword`.

## README
- Rewrite: pernyataan TUI-only + breaking change (subcommand `stats`/`machine`/`config`/`tui` dihapus, arahkan ke dashboard), section popup Akun (`a`/`Enter`/`l`) di tabel keys + penjelasan logout, hapus section CLI, contoh output stats, dan alternatif `hmv config`. First Setup, Downloads, Security Notes tetap.

## QA & rilis
1. `cargo test` (34 existing + test baru) + `cargo clippy` bersih.
2. `cargo install --path .` (workdir eksplisit, cargo toolchain langsung) → `hmv --version` = 1.0.0.
3. Verifikasi tmux:
   - `hmv` → dashboard dengan data (akun demouser).
   - `a` → popup Akun tampil; `Esc` tutup.
   - `Enter` di popup Akun → popup login terisi username + notice ganti akun.
   - `l` → logout: popup login muncul, data kosong, config.json `username: null`, keyring entry terhapus (dicek via `keyctl`).
   - Re-login setelah logout untuk memulihkan state: password akun asli kubackup dulu dari keyutils (`keyctl search/print hmv-cli:demouser`); bila tak terbaca, kupause dan kamu yang isi password — konfirmasi dulu sebelum test logout dengan akun asli.
   - `hmv stats` / `hmv tui` → pesan arahan + exit 1; `hmv --help`/`--version` jalan.
4. Commit `feat!: TUI-only dashboard + account management (v1.0.0)` + push + install.

## Di luar lingkup ini (milestone berikutnya)
- Publish ke crates.io + GitHub Actions binary release — butuh keputusan & akunmu, terpisah setelah v1.0.0 stabil.