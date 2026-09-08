//! Interactive dashboard: application state, input handling and the event
//! loop. Rendering lives in `render.rs`; all state transitions here are pure
//! and unit-tested.

pub mod render;

pub mod downloads;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::path::PathBuf;

use crate::i18n::{Key, Lang};
use crate::modules::flag::FlagVerdict;
use crate::modules::machines::Machine;
use crate::modules::releases::Release;
use crate::modules::stats::{ProfileStats, ProfileWriteup};
use crate::modules::submissions::{FieldKind, FormField, QueueEntry};
use crate::modules::writeups::Writeup;

/// What a popup asks the user for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Flag,
    Upload,
    Download,
    /// Credentials popup (first run, stale password, account switch).
    Config,
    /// Account overview popup (`a`): shows the active account with actions
    /// to switch (`Enter`) or logout (`l`).
    Account,
    /// Submit-your-VM form (`v` on the Submissions tab).
    SubmissionForm,
    /// Submission rules viewer (`i` on the Submissions tab).
    Rules,
}

/// Why the credentials popup was opened — drives its yellow notice line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigContext {
    FirstRun,
    LoginFailed,
    Switch,
    LoggedOut,
}

/// A text-input popup bound to one machine. The Flag popup carries two
/// fields (user & root); Upload has one. Read-only popups (already-PWNED
/// machines) render as an info box and cannot submit anything.
#[derive(Debug, Clone)]
pub struct Popup {
    pub kind: PopupKind,
    pub vm: String,
    pub buffers: Vec<String>,
    pub field: usize,
    /// Yellow banner rendered above the fields (e.g. one flag already in).
    pub notice: Option<String>,
    /// Info-only popup: no fields, Enter/Esc just closes it.
    pub readonly: bool,
    /// Path-completion candidates for the Download popup (Tab).
    pub completions: Vec<String>,
}

impl Popup {
    /// Select-field helper: moves the active buffer through `values`.
    /// Forward = next value; backward = previous (wraps both ways).
    pub fn cycle_value_step(&mut self, values: &[String], forward: bool) {
        if values.is_empty() {
            return;
        }
        if let Some(buffer) = self.buffers.get_mut(self.field) {
            let next = match values.iter().position(|v| v == buffer) {
                Some(i) => {
                    if forward {
                        (i + 1) % values.len()
                    } else {
                        (i + values.len() - 1) % values.len()
                    }
                }
                None => {
                    if forward {
                        0
                    } else {
                        values.len() - 1
                    }
                }
            };
            *buffer = values[next].clone();
        }
    }

    pub fn push(&mut self, c: char) {
        if let Some(buffer) = self.buffers.get_mut(self.field) {
            buffer.push(c);
        }
        // Typing invalidates a previous completion listing.
        self.completions.clear();
    }

    pub fn pop(&mut self) {
        if let Some(buffer) = self.buffers.get_mut(self.field) {
            buffer.pop();
        }
        self.completions.clear();
    }

    pub fn next_field(&mut self) {
        if self.buffers.len() > 1 {
            self.field = (self.field + 1) % self.buffers.len();
        }
    }

    pub fn previous_field(&mut self) {
        if self.buffers.len() > 1 {
            self.field = (self.field + self.buffers.len() - 1) % self.buffers.len();
        }
    }

    /// zsh-style destination completion for the Download popup: `Tab`
    /// expands `~`, completes the last path component against the parent
    /// directory's subdirectories (common prefix first) and stores the
    /// candidate list so the popup can display it.
    pub fn complete_destination(&mut self) {
        let Some(buffer) = self.buffers.get_mut(0) else {
            return;
        };
        let raw = buffer.clone();
        let expanded = expand_tilde(&raw);
        let ends_with_sep = raw.ends_with('/');
        // A trailing separator means "complete inside this directory": the
        // partial component is empty even though Path::file_name would
        // still report one.
        let (parent, partial) = if ends_with_sep {
            (expanded.clone(), String::new())
        } else {
            split_parent_partial(&expanded)
        };

        let mut matches: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&parent) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !partial.is_empty() && !name.starts_with(partial.as_str()) {
                    continue;
                }
                // Skip hidden dirs unless the user typed the dot herself.
                if name.starts_with('.') && (partial.is_empty() || !partial.starts_with('.')) {
                    continue;
                }
                if entry.path().is_dir() {
                    matches.push(name);
                }
            }
        }
        matches.sort();
        if matches.is_empty() {
            self.completions.clear();
            return;
        }

        let completed = common_prefix(&matches);
        // Replace the partial component with the completed prefix, keeping
        // the original `~` spelling the user typed.
        let mut new_raw = raw[..raw.len().saturating_sub(partial.chars().count())].to_string();
        new_raw.push_str(&completed);
        if matches.len() == 1 && completed == matches[0] && !ends_with_sep {
            new_raw.push(std::path::MAIN_SEPARATOR);
        }
        *buffer = new_raw;
        self.completions = matches;
    }
}

/// `~` and `~/...` expand to the user's home directory.
fn expand_tilde(raw: &str) -> std::path::PathBuf {
    if raw == "~" {
        return home::home_dir().unwrap_or_else(|| std::path::PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(raw)
}

/// Splits an expanded path into (parent directory, last component); paths
/// ending in a separator (or the filesystem root) complete with no partial.
fn split_parent_partial(expanded: &std::path::Path) -> (std::path::PathBuf, String) {
    if expanded.as_os_str().is_empty() {
        return (std::path::PathBuf::from("."), String::new());
    }
    match (expanded.parent(), expanded.file_name()) {
        (Some(parent), Some(name)) => (parent.to_path_buf(), name.to_string_lossy().to_string()),
        _ => (expanded.to_path_buf(), String::new()),
    }
}

/// Longest prefix shared by every candidate.
fn common_prefix(items: &[String]) -> String {
    let mut prefix = items[0].clone();
    for item in &items[1..] {
        while !item.starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}

/// Result of one background host operation, delivered to the event loop
/// via an mpsc channel.
pub enum HostOutcome {
    Fetched(Box<Result<TuiData>>),
    Actioned(Result<ActionReport>),
    Writeups {
        vm: String,
        result: Result<Vec<Writeup>>,
    },
    Configured {
        username: String,
        result: Result<()>,
    },
    LoggedOut(Result<()>),
    Submissions {
        result: Result<(Vec<QueueEntry>, Option<String>, Vec<FormField>)>,
    },
}

/// Identifies the selected row of one tab so `set_data` can restore the
enum SelectionKey {
    Machine(String),
    Pending(String),
    Writeup { vm: String, url: String },
    Release(String),
    Submission { name: String, user: String },
}

/// A user action queued from a popup, executed by the host application.
/// `values` carries `(original field index, value)` so verdicts can be
/// labeled with the field they came from (User flag / Root flag); uploads
/// always carry exactly one URL.
#[derive(Debug, Clone)]
pub struct TuiAction {
    pub kind: PopupKind,
    pub vm: String,
    pub values: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
pub struct TuiData {
    pub stats: ProfileStats,
    /// (label, pwned, total) rows for the progress gauges.
    pub progress: Vec<(String, u64, u64)>,
    /// Machines fully pwned (user+root flags) without an accepted writeup.
    pub pending: Vec<String>,
    /// Full machine catalog for the Machines tab.
    pub catalog: Vec<Machine>,
    /// Upcoming machine release schedule (Releases tab).
    pub releases: Vec<Release>,
    /// Submission queue (Submissions tab): every user's submitted VMs.
    pub submissions: Vec<QueueEntry>,
    /// Scraped submit-form fields (cached for the submit popup).
    pub submission_form: Vec<FormField>,
    /// Submission rules text (Submissions tab info popup).
    pub rules: Option<String>,
}

impl TuiData {
    /// Placeholder shown while the first fetch is still running.
    pub fn empty() -> Self {
        Self {
            stats: ProfileStats::default(),
            progress: Vec::new(),
            pending: Vec::new(),
            catalog: Vec::new(),
            releases: Vec::new(),
            submissions: Vec::new(),
            submission_form: Vec::new(),
            rules: None,
        }
    }
}

/// One line of an action result popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Success,
    Failure,
    Info,
}

/// Shown after a flag/writeup action; persists until dismissed. If `changed`
/// is true, a data refresh is queued for when the user closes it (Option A).
#[derive(Debug, Clone)]
pub struct ActionReport {
    pub title: String,
    pub entries: Vec<(ReportKind, String)>,
    pub changed: bool,
    /// Short footer summary (5s expiry), separate from the popup.
    pub status: String,
}

/// Community writeups for one machine, fetched on demand (`w`) and rendered
/// as a table popup. `selected` indexes into `entries` for Enter-to-open.
#[derive(Debug, Clone)]
pub struct WriteupsPopup {
    pub vm: String,
    pub entries: Vec<Writeup>,
    pub selected: usize,
}

impl WriteupsPopup {
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.entries.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn selected_url(&self) -> Option<&str> {
        self.entries.get(self.selected).map(|w| w.url.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Stats,
    Writeups,
    Pending,
    Machines,
    Releases,
    Submissions,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Stats,
        Tab::Writeups,
        Tab::Pending,
        Tab::Machines,
        Tab::Releases,
        Tab::Submissions,
    ];

    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let last = Self::ALL.len() - 1;
        Self::ALL[(index + last) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
}

/// Sort order for the Machines tab (`s` cycles: site order -> smallest ->
/// largest -> back to site order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MachineSort {
    #[default]
    Default,
    SizeAsc,
    SizeDesc,
}

impl MachineSort {
    fn next(self) -> Self {
        match self {
            MachineSort::Default => MachineSort::SizeAsc,
            MachineSort::SizeAsc => MachineSort::SizeDesc,
            MachineSort::SizeDesc => MachineSort::Default,
        }
    }

    /// Suffix shown in the Machines tab title, e.g. " · size ↑".
    pub fn indicator(self) -> &'static str {
        match self {
            MachineSort::Default => "",
            MachineSort::SizeAsc => " · size ↑",
            MachineSort::SizeDesc => " · size ↓",
        }
    }
}

/// Overlay listing background download jobs (`o` toggles it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Normal,
    Downloads,
}

pub struct AppState {
    pub tab: Tab,
    pub input_mode: InputMode,
    /// Downloads overlay visibility (`o`).
    pub view: ViewMode,
    /// True while no usable session exists (first run or stale password);
    /// the config popup is then the only way forward.
    pub needs_config: bool,
    /// Set by the account popup (`l`); consumed by the event loop.
    pub pending_logout: bool,
    /// Background download jobs (newest last). Shared with the renderer.
    pub download_jobs: Vec<std::sync::Arc<downloads::DownloadJob>>,
    /// First `q` with active downloads only sets this; second quits.
    pub quit_warned: bool,
    /// UI language (EN/ES), switched with `l`, persisted in the config.
    pub lang: Lang,
    pub filter: String,
    pub selected: usize,
    /// Sort order of the Machines tab (cycled with `s`).
    pub machine_sort: MachineSort,
    /// Machines whose status is PWNED are hidden on the Machines tab (`h`).
    pub hide_pwned: bool,
    /// First visible row for the active list (manual scrolling window).
    pub scroll: usize,
    pub quit: bool,
    /// Set by `r`; consumed when the event loop is idle.
    pub refresh_requested: bool,
    /// When set, the footer shows `⟳ <label>` while a blocking fetch runs.
    pub fetching: Option<String>,
    pub status: Option<String>,
    /// When the status message should disappear (5s lifetime).
    pub status_expiry: Option<std::time::Instant>,
    /// Open text-input popup, if any.
    pub popup: Option<Popup>,
    /// Action queued by a popup, executed by the host application.
    pub pending_action: Option<TuiAction>,
    /// Downloads beyond the parallel cap, started FIFO when a slot frees.
    pub download_queue: std::collections::VecDeque<(String, PathBuf)>,
    /// Action result popup (persists until dismissed).
    pub report: Option<ActionReport>,
    /// Community-writeups popup for one machine (`w`, Machines/Pending).
    pub writeups_popup: Option<WriteupsPopup>,
    /// VM whose writeups must be fetched when the event loop goes idle.
    pub pending_writeups: Option<String>,
    /// Refresh queued for when the report popup closes (Option A).
    pub pending_refresh_after_close: bool,
    /// Queue a submissions fetch (`v` popup / refresh on the tab).
    pub pending_submissions: bool,
    pub data: TuiData,
    /// Row budget reported by the renderer after layout.
    pub last_visible_rows: Option<usize>,
}

/// How long a status message stays visible in the footer.
const STATUS_LIFETIME: Duration = Duration::from_secs(5);

/// Substitutes the `{}` placeholders of a translated template with `args`
/// in order; extra placeholders stay as `{}` (never a panic, unlike
/// `format!` with a non-literal template).
pub fn fmt_key(template: &str, args: &[&str]) -> String {
    let mut out = template.to_string();
    for arg in args {
        out = out.replacen("{}", arg, 1);
    }
    out
}

impl AppState {
    #[cfg(test)]
    pub fn new(data: TuiData) -> Self {
        Self::new_with_lang(data, Lang::En)
    }

    /// Same as [`new`](Self::new) but with the persisted UI language.
    pub fn new_with_lang(data: TuiData, lang: Lang) -> Self {
        Self {
            tab: Tab::Stats,
            input_mode: InputMode::Normal,
            view: ViewMode::Normal,
            needs_config: false,
            pending_logout: false,
            download_jobs: Vec::new(),
            quit_warned: false,
            lang,
            filter: String::new(),
            hide_pwned: false,
            selected: 0,
            machine_sort: MachineSort::default(),
            scroll: 0,
            quit: false,
            refresh_requested: false,
            fetching: None,
            status: None,
            status_expiry: None,
            popup: None,
            pending_action: None,
            download_queue: std::collections::VecDeque::new(),
            report: None,
            writeups_popup: None,
            pending_writeups: None,
            pending_refresh_after_close: false,
            pending_submissions: false,
            data,
            last_visible_rows: None,
        }
    }

    /// Entry state for `hmv tui`: draws immediately, then loads all data.
    pub fn loading_with_lang(lang: Lang) -> Self {
        let mut state = Self::new_with_lang(TuiData::empty(), lang);
        state.fetching = Some(state.lang.t(Key::FetchingLoading).to_string());
        state
    }

    #[cfg(test)]
    pub fn loading() -> Self {
        Self::loading_with_lang(Lang::En)
    }

    /// Entry state for bare `hmv` with no usable stored credentials: opens
    /// straight into the configuration popup.
    #[cfg(test)]
    pub fn unconfigured(stored_username: Option<&str>) -> Self {
        Self::unconfigured_with_lang(stored_username, Lang::En)
    }

    pub fn unconfigured_with_lang(stored_username: Option<&str>, lang: Lang) -> Self {
        let mut state = Self::new_with_lang(TuiData::empty(), lang);
        state.needs_config = true;
        let context = if stored_username
            .as_deref()
            .filter(|u| !u.trim().is_empty())
            .is_some()
        {
            ConfigContext::LoginFailed
        } else {
            ConfigContext::FirstRun
        };
        state.open_config_popup(context, stored_username);
        state
    }

    /// Number of background downloads still running.
    pub fn active_downloads(&self) -> usize {
        self.download_jobs
            .iter()
            .filter(|job| job.is_active())
            .count()
    }

    /// Toggles the downloads overlay; harmless while popups are open.
    pub fn toggle_downloads_view(&mut self) {
        if self.popup.is_none() && self.report.is_none() {
            self.view = match self.view {
                ViewMode::Normal => ViewMode::Downloads,
                ViewMode::Downloads => ViewMode::Normal,
            };
        }
    }

    /// Index of the newest active download (cancel target in the overlay).
    pub fn download_selected(&self) -> usize {
        self.download_jobs
            .iter()
            .rposition(|job| job.is_active())
            .unwrap_or(self.download_jobs.len().saturating_sub(1))
    }

    /// First `q` with active downloads warns instead of quitting.
    pub fn request_quit(&mut self) {
        let active = self.active_downloads();
        if active > 0 && !self.quit_warned {
            self.quit_warned = true;
            let jobs: Vec<String> = self
                .download_jobs
                .iter()
                .filter(|job| job.is_active())
                .map(|job| {
                    let state = job.state.lock().unwrap();
                    let pct = state
                        .downloaded
                        .checked_mul(100)
                        .and_then(|pct| pct.checked_div(state.total))
                        .map(|pct| format!(" {pct}%"))
                        .unwrap_or_default();
                    format!("↓ {}{pct}", job.vm)
                })
                .collect();
            self.set_status(fmt_key(
                self.lang.t(Key::QuitWarnDownloads),
                &[&active.to_string(), &jobs.join(" · ")],
            ));
            return;
        }
        self.quit = true;
    }

    /// Shows a status message in the footer, auto-expiring after 5 seconds.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
        self.status_expiry = Some(std::time::Instant::now() + STATUS_LIFETIME);
    }

    /// Clears expired status messages; called once per event-loop iteration.
    pub fn tick(&mut self) {
        if let Some(expiry) = self.status_expiry {
            if std::time::Instant::now() >= expiry {
                self.status = None;
                self.status_expiry = None;
            }
        }
    }

    /// Replaces the dashboard data. The list selection survives refreshes:
    /// the selected row is identified by its key (machine name / VM / URL /
    /// release name) and looked up again in the new data; a missing key
    /// falls back to the top of the list.
    pub fn set_data(&mut self, data: TuiData) {
        let selected_key = self.selection_key();
        self.data = data;
        self.selected = selected_key
            .and_then(|key| self.find_restored_index(&key))
            .unwrap_or(0);
        self.scroll = 0;
        self.ensure_selected_visible();
    }

    /// Identifies the row under the selection on the active tab, if any.
    fn selection_key(&self) -> Option<SelectionKey> {
        match self.tab {
            Tab::Machines => self
                .visible_machines()
                .get(self.selected)
                .map(|m| SelectionKey::Machine(m.name.clone())),
            Tab::Pending => self
                .visible_pending()
                .get(self.selected)
                .map(|vm| SelectionKey::Pending((*vm).clone())),
            Tab::Writeups => {
                self.visible_writeups()
                    .get(self.selected)
                    .map(|w| SelectionKey::Writeup {
                        vm: w.vm.clone(),
                        url: w.url.clone(),
                    })
            }
            Tab::Releases => self
                .visible_releases()
                .get(self.selected)
                .map(|r| SelectionKey::Release(r.name.clone())),
            Tab::Submissions => {
                self.visible_submissions()
                    .get(self.selected)
                    .map(|e| SelectionKey::Submission {
                        name: e.name.clone(),
                        user: e.user.clone(),
                    })
            }
            Tab::Stats => None,
        }
    }

    /// Index of the selection key in the freshly set data (same filters as
    /// the capture pass, since the filter did not change between the two).
    fn find_restored_index(&self, key: &SelectionKey) -> Option<usize> {
        match key {
            SelectionKey::Machine(name) => {
                self.visible_machines().iter().position(|m| &m.name == name)
            }
            SelectionKey::Pending(vm) => self.visible_pending().iter().position(|v| *v == vm),
            SelectionKey::Writeup { vm, url } => self
                .visible_writeups()
                .iter()
                .position(|w| &w.vm == vm && &w.url == url),
            SelectionKey::Release(name) => {
                self.visible_releases().iter().position(|r| &r.name == name)
            }
            SelectionKey::Submission { name, user } => self
                .visible_submissions()
                .iter()
                .position(|e| &e.name == name && &e.user == user),
        }
    }

    pub fn next_tab(&mut self) {
        self.tab = self.tab.next();
        self.reset_list_position();
    }

    pub fn previous_tab(&mut self) {
        self.tab = self.tab.previous();
        self.reset_list_position();
    }

    fn reset_list_position(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    /// Lowercase-filtered accepted writeups for the Writeups tab.
    pub fn visible_writeups(&self) -> Vec<&ProfileWriteup> {
        let needle = self.filter.to_lowercase();
        self.data
            .stats
            .accepted_writeups
            .iter()
            .filter(|w| {
                needle.is_empty()
                    || w.vm.to_lowercase().contains(&needle)
                    || w.language.to_lowercase().contains(&needle)
                    || w.url.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Lowercase-filtered pending machine names for the Pending tab.
    pub fn visible_pending(&self) -> Vec<&String> {
        let needle = self.filter.to_lowercase();
        self.data
            .pending
            .iter()
            .filter(|vm| needle.is_empty() || vm.to_lowercase().contains(&needle))
            .collect()
    }

    /// Lowercase-filtered machine catalog for the Machines tab. Filter
    /// matches name, difficulty, creator or status; fully-PWNED machines
    /// are dropped while `hide_pwned` is set (`h`); `machine_sort` orders
    /// the result by size (smallest/largest) or keeps the site order.
    pub fn visible_machines(&self) -> Vec<&Machine> {
        let needle = self.filter.to_lowercase();
        let mut machines: Vec<&Machine> = self
            .data
            .catalog
            .iter()
            .filter(|m| {
                if self.hide_pwned && m.status.to_uppercase().contains("PWNED") {
                    return false;
                }
                needle.is_empty()
                    || m.name.to_lowercase().contains(&needle)
                    || m.difficulty.to_lowercase().contains(&needle)
                    || m.creator.to_lowercase().contains(&needle)
                    || m.status.to_lowercase().contains(&needle)
            })
            .collect();
        match self.machine_sort {
            MachineSort::Default => {}
            MachineSort::SizeAsc => machines.sort_by(|a, b| {
                crate::modules::machines::size_mb(&a.size)
                    .partial_cmp(&crate::modules::machines::size_mb(&b.size))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            MachineSort::SizeDesc => machines.sort_by(|a, b| {
                crate::modules::machines::size_mb(&b.size)
                    .partial_cmp(&crate::modules::machines::size_mb(&a.size))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
        }
        machines
    }

    /// Toggles hiding fully-PWNED machines on the Machines tab. Gated to
    /// that tab like the other machine actions.
    pub fn toggle_hide_pwned(&mut self) {
        if self.tab != Tab::Machines {
            self.set_status(self.lang.t(Key::StatusHidePwnedOnly));
            return;
        }
        self.hide_pwned = !self.hide_pwned;
        self.reset_list_position();
        let key = if self.hide_pwned {
            Key::StatusPwnedHidden
        } else {
            Key::StatusPwnedShown
        };
        self.set_status(self.lang.t(key));
    }

    /// Switches the UI language (EN ↔ ES) and persists the choice.
    pub fn toggle_language(&mut self) {
        self.lang = self.lang.other();
        // Persistence is best-effort: the switch applies to this session
        // even if the config file cannot be written.
        let _ = crate::config::ConfigManager::new().save_language(self.lang);
        self.set_status(fmt_key(
            self.lang.t(Key::StatusLangSwitched),
            &[self.lang.label()],
        ));
    }

    /// Cycles the Machines-tab size sort: site order -> smallest -> largest.
    /// Gated to the Machines tab like the other actions.
    pub fn cycle_machine_sort(&mut self) {
        if self.tab != Tab::Machines {
            self.set_status(self.lang.t(Key::StatusSortOnly));
            return;
        }
        self.machine_sort = self.machine_sort.next();
        self.reset_list_position();
    }

    /// Submission queue rows for the Submissions tab (filter on name,
    /// user or status).
    pub fn visible_submissions(&self) -> Vec<&QueueEntry> {
        let needle = self.filter.to_lowercase();
        self.data
            .submissions
            .iter()
            .filter(|e| {
                needle.is_empty()
                    || e.name.to_lowercase().contains(&needle)
                    || e.user.to_lowercase().contains(&needle)
                    || e.status.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Filtered release schedule for the Releases tab (date, name, os).
    pub fn visible_releases(&self) -> Vec<&Release> {
        let needle = self.filter.to_lowercase();
        self.data
            .releases
            .iter()
            .filter(|r| {
                needle.is_empty()
                    || r.name.to_lowercase().contains(&needle)
                    || r.date.to_lowercase().contains(&needle)
                    || r.os.to_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Dismisses the result popup; returns true if a refresh was queued.
    pub fn close_report(&mut self) -> bool {
        self.report = None;
        std::mem::take(&mut self.pending_refresh_after_close)
    }

    /// Name of the machine under the selection on machine-centric tabs.
    pub fn selected_machine_name(&self) -> Option<String> {
        match self.tab {
            Tab::Machines => self
                .visible_machines()
                .get(self.selected)
                .map(|m| m.name.clone()),
            Tab::Pending => self
                .visible_pending()
                .get(self.selected)
                .map(|vm| (*vm).clone()),
            _ => None,
        }
    }

    /// The full machine under the selection (Machines tab only).
    fn selected_machine(&self) -> Option<&Machine> {
        if self.tab != Tab::Machines {
            return None;
        }
        self.visible_machines().get(self.selected).copied()
    }

    /// Opens the input popup for the given action. Actions are gated per
    /// tab: flags belong on Machines, writeups on Pending. Flag popups are
    /// status-aware: PWNED machines get a read-only info box, DONE ones a
    /// "one flag remains" notice.
    pub fn open_action_popup(&mut self, kind: PopupKind) {
        if self.fetching.is_some() {
            self.set_status(self.lang.t(Key::StatusBusy));
            return;
        }
        if self.popup.is_some() {
            return;
        }
        if kind == PopupKind::Config {
            // Config popups are managed by the startup path and the event
            // loop (first run / failed login), never opened ad hoc.
            return;
        }
        let allowed = match kind {
            PopupKind::Flag | PopupKind::Download => self.tab == Tab::Machines,
            PopupKind::Upload => self.tab == Tab::Pending,
            PopupKind::SubmissionForm | PopupKind::Rules => self.tab == Tab::Submissions,
            PopupKind::Config | PopupKind::Account => false, // managed elsewhere
        };
        if !allowed {
            self.set_status(match kind {
                PopupKind::Flag => self.lang.t(Key::StatusFlagOnlyMachines),
                PopupKind::Upload => self.lang.t(Key::StatusUploadOnlyPending),
                PopupKind::Download => self.lang.t(Key::StatusDownloadOnlyMachines),
                PopupKind::SubmissionForm | PopupKind::Rules => {
                    self.lang.t(Key::StatusSubmitOnlySubmissions)
                }
                PopupKind::Config | PopupKind::Account => return, // never opened ad hoc
            });
            return;
        }
        let Some(vm) = self.selected_machine_name() else {
            self.set_status(self.lang.t(Key::StatusNothingSelected));
            return;
        };

        if kind == PopupKind::Flag {
            let status = self
                .selected_machine()
                .map(|m| m.status.to_uppercase())
                .unwrap_or_default();
            if status.contains("PWNED") {
                // Fully completed machine: show an info box, no inputs.
                self.popup = Some(Popup {
                    kind,
                    vm,
                    buffers: Vec::new(),
                    field: 0,
                    notice: None,
                    readonly: true,
                    completions: Vec::new(),
                });
                return;
            }
            let notice = if status.contains("DONE") {
                Some(self.lang.t(Key::NoticeOneFlagRemains).to_string())
            } else {
                None
            };
            self.popup = Some(Popup {
                kind,
                vm,
                buffers: vec![String::new(), String::new()],
                field: 0,
                notice,
                readonly: false,
                completions: Vec::new(),
            });
            return;
        }

        if kind == PopupKind::Download {
            // Prefill with the persisted choice, else the working directory.
            let prefill = crate::config::ConfigManager::new()
                .download_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
            self.popup = Some(Popup {
                kind,
                vm,
                buffers: vec![prefill.display().to_string()],
                field: 0,
                notice: None,
                readonly: false,
                completions: Vec::new(),
            });
            return;
        }

        self.popup = Some(Popup {
            kind,
            vm,
            buffers: vec![String::new()],
            field: 0,
            notice: None,
            readonly: false,
            completions: Vec::new(),
        });
    }

    /// Opens the credentials popup for the given reason, optionally
    /// prefilled with a username.
    pub fn open_config_popup(&mut self, context: ConfigContext, username: Option<&str>) {
        if self.popup.is_some() {
            return;
        }
        let notice = match context {
            ConfigContext::FirstRun => self.lang.t(Key::NoticeFirstRun),
            ConfigContext::LoginFailed => self.lang.t(Key::NoticeLoginFailed),
            ConfigContext::Switch => self.lang.t(Key::NoticeSwitch),
            ConfigContext::LoggedOut => self.lang.t(Key::NoticeLoggedOut),
        };
        self.popup = Some(Popup {
            kind: PopupKind::Config,
            vm: String::new(),
            buffers: vec![username.unwrap_or_default().to_string(), String::new()],
            field: 0,
            notice: Some(notice.to_string()),
            readonly: false,
            completions: Vec::new(),
        });
    }

    /// Opens the account popup for the active session (`a`): shows the
    /// logged-in account with actions to switch (`Enter`) or logout (`l`).
    /// The username rides in `Popup::vm` for the renderer.
    pub fn open_account_popup(&mut self) {
        if self.fetching.is_some() {
            self.set_status(self.lang.t(Key::StatusBusy));
            return;
        }
        if self.needs_config
            || self.popup.is_some()
            || self.report.is_some()
            || self.writeups_popup.is_some()
        {
            return;
        }
        self.popup = Some(Popup {
            kind: PopupKind::Account,
            vm: self.data.stats.username.clone(),
            buffers: Vec::new(),
            field: 0,
            notice: None,
            readonly: true,
            completions: Vec::new(),
        });
    }

    /// Enter on the account popup: close it and open the login popup
    /// prefilled with the current username for an account switch. The
    /// dashboard keeps showing the current account until the switch
    /// succeeds (a failure just reopens the login popup).
    pub fn begin_account_switch(&mut self) {
        let username = self.data.stats.username.clone();
        self.popup = None;
        self.open_config_popup(ConfigContext::Switch, Some(&username));
    }

    /// Opens the submit-your-VM popup built from the cached scraped form
    /// fields. Gated to the Submissions tab.
    pub fn open_submission_form(&mut self) {
        if self.fetching.is_some() {
            self.set_status(self.lang.t(Key::StatusBusy));
            return;
        }
        if self.popup.is_some() || self.report.is_some() {
            return;
        }
        if self.tab != Tab::Submissions {
            self.set_status(self.lang.t(Key::StatusSubmitOnlySubmissions));
            return;
        }
        if self.data.submission_form.is_empty() {
            self.set_status(self.lang.t(Key::FetchingForm));
            self.pending_submissions = true;
            return;
        }
        let buffers = self
            .data
            .submission_form
            .iter()
            .map(|f| f.value.clone())
            .collect();
        self.popup = Some(Popup {
            kind: PopupKind::SubmissionForm,
            vm: String::new(),
            buffers,
            field: 0,
            notice: Some(self.lang.t(Key::SubmitFormNotice).to_string()),
            readonly: false,
            completions: Vec::new(),
        });
    }

    /// Opens the rules popup with the cached rules text.
    pub fn open_rules_popup(&mut self) {
        if self.popup.is_some() || self.report.is_some() {
            return;
        }
        if self.tab != Tab::Submissions {
            self.set_status(self.lang.t(Key::StatusSubmitOnlySubmissions));
            return;
        }
        let text = self
            .data
            .rules
            .clone()
            .unwrap_or_else(|| self.lang.t(Key::RulesUnavailable).to_string());
        self.popup = Some(Popup {
            kind: PopupKind::Rules,
            vm: String::new(),
            buffers: vec![text],
            field: 0,
            notice: None,
            readonly: true,
            completions: Vec::new(),
        });
    }

    /// Queues a writeups fetch for the selected machine; the event loop
    /// runs the (blocking) fetch, then opens the popup. Gated to the
    /// Machines and Pending tabs.
    pub fn open_writeups_popup(&mut self) {
        if self.fetching.is_some() {
            self.set_status(self.lang.t(Key::StatusBusy));
            return;
        }
        if self.writeups_popup.is_some() || self.popup.is_some() || self.report.is_some() {
            return;
        }
        if !matches!(self.tab, Tab::Machines | Tab::Pending) {
            self.set_status(self.lang.t(Key::StatusWriteupsTabs));
            return;
        }
        let Some(vm) = self.selected_machine_name() else {
            self.set_status(self.lang.t(Key::StatusNothingToInspect));
            return;
        };
        self.pending_writeups = Some(vm);
    }

    /// Opens the selected writeup of the current writeups popup in a browser.
    pub fn open_selected_writeup_link(&mut self) {
        let Some(popup) = self.writeups_popup.as_ref() else {
            return;
        };
        let Some(url) = popup.selected_url() else {
            return;
        };
        let opened = std::process::Command::new("xdg-open")
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        self.set_status(match opened {
            Ok(_) => fmt_key(self.lang.t(Key::StatusOpenedBrowser), &[url]),
            Err(error) => fmt_key(self.lang.t(Key::StatusXdgOpenFailed), &[&error.to_string()]),
        });
    }

    /// Closes the writeups popup (Esc / q).
    pub fn close_writeups_popup(&mut self) {
        self.writeups_popup = None;
    }

    /// Confirms the popup: queues the action and closes the popup.
    /// With two fields, non-empty entries are submitted together; a single
    /// non-empty entry (or single-field popup) submits alone. Read-only
    /// popups never queue anything.
    pub fn confirm_popup(&mut self) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        if popup.readonly {
            self.set_status(fmt_key(self.lang.t(Key::StatusAlreadyPwned), &[&popup.vm]));
            return;
        }
        let values: Vec<(usize, String)> = popup
            .buffers
            .iter()
            .enumerate()
            .map(|(index, b)| (index, b.trim().to_string()))
            .filter(|(_, v)| !v.is_empty())
            .collect();

        if popup.kind == PopupKind::Config {
            if values.len() < 2 {
                self.popup = Some(popup);
                self.set_status(self.lang.t(Key::StatusCredentialsRequired));
                return;
            }
            self.set_status(self.lang.t(Key::StatusConnecting));
            self.pending_action = Some(TuiAction {
                kind: popup.kind,
                vm: popup.vm,
                values,
            });
            return;
        }

        if values.is_empty() {
            self.set_status(self.lang.t(Key::StatusEmptyInput));
            return;
        }
        if popup.kind == PopupKind::SubmissionForm {
            // Validate every required scraped field + the level value.
            let mut missing: Vec<String> = Vec::new();
            let mut level_ok = true;
            for (index, field) in self.data.submission_form.iter().enumerate() {
                let value = popup.buffers.get(index).cloned().unwrap_or_default();
                let empty = value.trim().is_empty();
                if field.required && empty {
                    missing.push(field.name.clone());
                }
                if let FieldKind::Select(options) = &field.kind {
                    if !empty && !options.iter().any(|o| o.eq_ignore_ascii_case(value.trim())) {
                        level_ok = false;
                    }
                }
            }
            if !missing.is_empty() {
                self.popup = Some(popup);
                self.set_status(fmt_key(
                    self.lang.t(Key::SubmitRequiredMissing),
                    &[&missing.join(", ")],
                ));
                return;
            }
            if !level_ok {
                self.popup = Some(popup);
                self.set_status(self.lang.t(Key::SubmitInvalidLevel));
                return;
            }
            self.set_status(self.lang.t(Key::SubmitOk));
            self.pending_action = Some(TuiAction {
                kind: popup.kind,
                vm: popup.vm,
                values,
            });
            return;
        }

        if popup.kind == PopupKind::Download {
            // Enforce the parallel cap here: overflow goes to the queue.
            let active = self.active_downloads();
            if active >= downloads::PARALLEL_DOWNLOADS {
                let vm = popup.vm.clone();
                self.download_queue
                    .push_back((popup.vm, PathBuf::from(values[0].1.clone())));
                self.set_status(fmt_key(
                    self.lang.t(Key::StatusDownloadQueued),
                    &[&vm, &downloads::PARALLEL_DOWNLOADS.to_string()],
                ));
                return;
            }
        }

        let status_key = match popup.kind {
            PopupKind::Flag => {
                if values.len() > 1 {
                    Key::StatusQueuedFlags
                } else {
                    Key::StatusQueuedFlag
                }
            }
            PopupKind::Upload => Key::StatusQueuedWriteup,
            PopupKind::Download => Key::StatusQueuedDownload,
            PopupKind::Config | PopupKind::Account => unreachable!("handled above"),
            PopupKind::SubmissionForm | PopupKind::Rules => unreachable!("handled above"),
        };
        self.set_status(fmt_key(self.lang.t(status_key), &[&popup.vm]));
        self.pending_action = Some(TuiAction {
            kind: popup.kind,
            vm: popup.vm,
            values,
        });
    }

    fn row_count(&self) -> usize {
        match self.tab {
            Tab::Stats => 0,
            Tab::Writeups => self.visible_writeups().len(),
            Tab::Pending => self.visible_pending().len(),
            Tab::Machines => self.visible_machines().len(),
            Tab::Releases => self.visible_releases().len(),
            Tab::Submissions => self.visible_submissions().len(),
        }
    }

    pub fn move_down(&mut self) {
        let last = self.row_count().saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
        self.ensure_selected_visible();
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.ensure_selected_visible();
    }

    pub fn move_start(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    #[cfg(test)]
    pub fn reset_list_position_for_test(&mut self) {
        self.reset_list_position();
    }

    /// Keeps `selected` inside the `[scroll, scroll + visible)` window.
    pub fn ensure_selected_visible(&mut self) {
        let visible = self.last_visible_rows.unwrap_or(10).max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible {
            self.scroll = self.selected + 1 - visible;
        }
    }

    /// Row budget reported by the renderer after layout.
    pub fn set_visible_rows(&mut self, rows: usize) {
        self.last_visible_rows = Some(rows.max(1));
        self.ensure_selected_visible();
    }

    pub fn enter_filter_mode(&mut self) {
        self.input_mode = InputMode::Filter;
    }

    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.reset_list_position();
    }

    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.reset_list_position();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.reset_list_position();
    }

    /// URL of the selected accepted writeup, for opening in a browser.
    pub fn selected_writeup_url(&self) -> Option<&str> {
        if self.tab != Tab::Writeups {
            return None;
        }
        self.visible_writeups()
            .get(self.selected)
            .map(|w| w.url.as_str())
    }

    pub fn open_selected_link(&mut self) {
        if let Some(url) = self.selected_writeup_url() {
            let opened = std::process::Command::new("xdg-open")
                .arg(url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            self.set_status(match opened {
                Ok(_) => fmt_key(self.lang.t(Key::StatusOpenedBrowser), &[url]),
                Err(error) => fmt_key(self.lang.t(Key::StatusXdgOpenFailed), &[&error.to_string()]),
            });
        }
    }

    /// Manual refresh: only allowed while idle to keep the state machine
    /// sane. The event loop owns the `fetching` label; this only queues the
    /// request — setting the label here too would make the loop's trigger
    /// condition (`fetching.is_none()`) never fire.
    pub fn request_refresh(&mut self) {
        if self.fetching.is_none() {
            self.refresh_requested = true;
        }
    }

    /// Whether the event loop should run a (re)fetch right now.
    pub fn should_fetch(&self, pending_fetch: bool) -> bool {
        pending_fetch || self.refresh_requested
    }
}

/// Builds the result popup for a flag submission. Verdicts are labeled with
/// the field they were typed into (User flag / Root flag) — the API does not
/// expose the flag level. A lone accepted flag keeps the celebratory footer.
pub fn build_flag_report(lang: Lang, vm: &str, results: Vec<(usize, FlagVerdict)>) -> ActionReport {
    let mut entries = Vec::new();
    let mut compact = Vec::new();
    let mut changed = false;

    for (field, verdict) in results {
        let key = if field == 0 {
            Key::PopupUserFlag
        } else {
            Key::PopupRootFlag
        };
        let label = lang.t(key).trim_end_matches(':');
        let short = if field == 0 {
            lang.t(Key::CompactUser)
        } else {
            lang.t(Key::CompactRoot)
        };
        match verdict {
            FlagVerdict::Correct => {
                entries.push((
                    ReportKind::Success,
                    fmt_key(lang.t(Key::ReportFlagAccepted), &[label]),
                ));
                compact.push(format!("{short} ✓"));
                changed = true;
            }
            FlagVerdict::Wrong => {
                entries.push((
                    ReportKind::Failure,
                    fmt_key(lang.t(Key::ReportFlagRejected), &[label]),
                ));
                compact.push(format!("{short} ✗"));
            }
            FlagVerdict::MachineNotFound => {
                entries.push((
                    ReportKind::Failure,
                    fmt_key(lang.t(Key::ReportMachineNotFound), &[vm]),
                ));
                compact.push(lang.t(Key::CompactMachineNotFound).to_string());
            }
            FlagVerdict::Unknown(body) => {
                let body: String = body.chars().take(60).collect();
                entries.push((
                    ReportKind::Info,
                    fmt_key(lang.t(Key::ReportUnknownResponse), &[&body]),
                ));
                compact.push(lang.t(Key::CompactUnknown).to_string());
            }
        }
    }

    let status = if entries.len() == 1 && changed {
        fmt_key(lang.t(Key::ReportYouHacked), &[vm])
    } else {
        let marker = if changed { "+" } else { "!" };
        format!("[{marker}] {}", compact.join(" · "))
    };

    ActionReport {
        entries,
        title: fmt_key(lang.t(Key::FlagResultsTitle), &[vm]),
        changed,
        status,
    }
}

type RefetchFn = std::sync::Arc<dyn Fn() -> Result<TuiData> + Send + Sync>;
type RunActionFn = std::sync::Arc<dyn Fn(TuiAction) -> Result<ActionReport> + Send + Sync>;
type WriteupsFn = std::sync::Arc<dyn Fn(&str) -> Result<Vec<Writeup>> + Send + Sync>;
type ConfigFn = std::sync::Arc<dyn Fn(&str, &str) -> Result<()> + Send + Sync>;
type LogoutFn = std::sync::Arc<dyn Fn() -> Result<()> + Send + Sync>;
/// Fetches (queue, rules, submit-form fields) in one background call.
type SubmissionsFn = std::sync::Arc<
    dyn Fn() -> Result<(Vec<QueueEntry>, Option<String>, Vec<FormField>)> + Send + Sync,
>;

/// Host-provided callbacks the event loop spawns on background threads.
/// Each closure lives in its own Arc so a spawn clones one cheap handle.
struct HostOps {
    refetch: RefetchFn,
    run_action: RunActionFn,
    run_writeups_fetch: WriteupsFn,
    run_config: ConfigFn,
    logout: LogoutFn,
    fetch_submissions: SubmissionsFn,
}

pub fn run(
    mut app: AppState,
    refetch: impl Fn() -> Result<TuiData> + Send + Sync + 'static,
    run_action: impl Fn(TuiAction) -> Result<ActionReport> + Send + Sync + 'static,
    run_writeups_fetch: impl Fn(&str) -> Result<Vec<Writeup>> + Send + Sync + 'static,
    run_config: impl Fn(&str, &str) -> Result<()> + Send + Sync + 'static,
    logout: impl Fn() -> Result<()> + Send + Sync + 'static,
    fetch_submissions: impl Fn() -> Result<(Vec<QueueEntry>, Option<String>, Vec<FormField>)>
        + Send
        + Sync
        + 'static,
) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel::<HostOutcome>();
    let mut terminal = ratatui::init();
    // Kick off the first load (and any pending request) before looping.
    let pending_fetch = app.fetching.is_some();
    let result = event_loop(
        &mut terminal,
        &mut app,
        tx,
        rx,
        pending_fetch,
        HostOps {
            refetch: std::sync::Arc::new(refetch),
            run_action: std::sync::Arc::new(run_action),
            run_writeups_fetch: std::sync::Arc::new(run_writeups_fetch),
            run_config: std::sync::Arc::new(run_config),
            logout: std::sync::Arc::new(logout),
            fetch_submissions: std::sync::Arc::new(fetch_submissions),
        },
    );
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
    tx: std::sync::mpsc::Sender<HostOutcome>,
    rx: std::sync::mpsc::Receiver<HostOutcome>,
    mut pending_fetch: bool,
    host: HostOps,
) -> Result<()> {
    let HostOps {
        refetch,
        run_action,
        run_writeups_fetch,
        run_config,
        logout,
        fetch_submissions,
    } = host;
    // One host operation in the air at a time; set together with
    // `app.fetching` and cleared when its outcome is applied.
    let mut in_flight = false;
    loop {
        terminal.draw(|frame| crate::tui::render::draw(frame, app))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key(app, key);
                }
            }
        }

        app.tick();

        // Apply any outcomes that arrived since the last iteration
        // (non-blocking). Each one clears the busy state.
        while let Ok(outcome) = rx.try_recv() {
            in_flight = false;
            app.fetching = None;
            apply_outcome(app, outcome, &mut pending_fetch);
        }

        if !in_flight {
            // Logout from the account popup (`l`): drop the stored account
            // and the session, then return to the login popup. Active
            // downloads keep running — they use public MEGA links, not the
            // session.
            if app.pending_logout {
                app.pending_logout = false;
                in_flight = true;
                app.fetching = Some(app.lang.t(Key::FetchingLogout).to_string());
                let tx = tx.clone();
                let logout = logout.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(HostOutcome::LoggedOut(logout()));
                });
            }

            // User actions from popups (config, flag submission, writeup
            // upload, download start).
            if let Some(action) = app.pending_action.take() {
                match action.kind {
                    PopupKind::Download => {
                        // Non-blocking: spawn a background job and move on.
                        let dir = PathBuf::from(action.values[0].1.clone());
                        match downloads::start_download(action.vm.clone(), dir) {
                            Ok(job) => {
                                app.set_status(fmt_key(
                                    app.lang.t(Key::StatusDownloadStarted),
                                    &[&action.vm],
                                ));
                                app.download_jobs.push(std::sync::Arc::new(job));
                            }
                            Err(error) => app.set_status(fmt_key(
                                app.lang.t(Key::StatusDownloadFailed),
                                &[&format!("{error:#}")],
                            )),
                        }
                    }
                    PopupKind::Config => {
                        let value = |field: usize| {
                            action
                                .values
                                .iter()
                                .find(|(f, _)| *f == field)
                                .map(|(_, v)| v.clone())
                                .unwrap_or_default()
                        };
                        in_flight = true;
                        let (username, password) = (value(0), value(1));
                        let tx = tx.clone();
                        let run_config = run_config.clone();
                        std::thread::spawn(move || {
                            let result = run_config(&username, &password);
                            let _ = tx.send(HostOutcome::Configured { username, result });
                        });
                    }
                    PopupKind::Account | PopupKind::Rules => {
                        unreachable!("no action is queued from these popups")
                    }
                    PopupKind::Flag | PopupKind::Upload | PopupKind::SubmissionForm => {
                        let label = match action.kind {
                            PopupKind::Flag => {
                                fmt_key(app.lang.t(Key::FetchingFlag), &[&action.vm])
                            }
                            PopupKind::Upload => {
                                fmt_key(app.lang.t(Key::FetchingWriteup), &[&action.vm])
                            }
                            _ => unreachable!("handled above"),
                        };
                        in_flight = true;
                        app.fetching = Some(label);
                        let tx = tx.clone();
                        let run_action = run_action.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(HostOutcome::Actioned(run_action(action)));
                        });
                    }
                }
            }

            // Queued writeups fetch for the `w` key. Runs with a
            // `⟳ Loading writeups for <vm>...` label; opens the popup when
            // the outcome arrives.
            if let Some(vm) = app.pending_writeups.take() {
                in_flight = true;
                app.fetching = Some(fmt_key(app.lang.t(Key::FetchingWriteupsList), &[&vm]));
                let tx = tx.clone();
                let run_writeups_fetch = run_writeups_fetch.clone();
                std::thread::spawn(move || {
                    let result = run_writeups_fetch(&vm);
                    let _ = tx.send(HostOutcome::Writeups { vm, result });
                });
            }

            // Queued submissions fetch (Submissions tab open or `r` there).
            if app.pending_submissions {
                app.pending_submissions = false;
                in_flight = true;
                app.fetching = Some(app.lang.t(Key::FetchingRefreshing).to_string());
                let tx = tx.clone();
                let fetch_submissions = fetch_submissions.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(HostOutcome::Submissions {
                        result: fetch_submissions(),
                    });
                });
            }

            // Full dashboard refresh.
            if app.should_fetch(pending_fetch) {
                pending_fetch = false;
                app.refresh_requested = false;
                in_flight = true;
                app.fetching = Some(app.lang.t(Key::FetchingRefreshing).to_string());
                let tx = tx.clone();
                let refetch = refetch.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(HostOutcome::Fetched(Box::new(refetch())));
                });
            }
        }

        // Start queued downloads as slots free up. Terminal jobs are kept
        // for the rest of the session as a small success/failure history.
        if app.active_downloads() < downloads::PARALLEL_DOWNLOADS {
            while let Some((vm, dir)) = app.download_queue.pop_front() {
                match downloads::start_download(vm.clone(), dir) {
                    Ok(job) => {
                        app.set_status(fmt_key(app.lang.t(Key::StatusDownloadStarted), &[&vm]));
                        app.download_jobs.push(std::sync::Arc::new(job));
                    }
                    Err(error) => app.set_status(fmt_key(
                        app.lang.t(Key::StatusDownloadFailed),
                        &[&format!("{error:#}")],
                    )),
                }
                if app.active_downloads() >= downloads::PARALLEL_DOWNLOADS {
                    break;
                }
            }
        }

        if app.quit {
            // Abort active tasks and clean their staged `.part` files.
            for job in &app.download_jobs {
                if job.is_active() {
                    job.request_cancel();
                    job.remove_part();
                }
            }
            return Ok(());
        }
    }
}

/// Applies one background operation result to the app state. Called with
/// `fetching` already cleared; the loop dispatches new work on the next
/// iteration.
fn apply_outcome(app: &mut AppState, outcome: HostOutcome, pending_fetch: &mut bool) {
    match outcome {
        HostOutcome::Fetched(result) => match *result {
            Ok(data) => {
                app.set_data(data);
                app.set_status(app.lang.t(Key::StatusDataRefreshed));
            }
            Err(error) => app.set_status(fmt_key(
                app.lang.t(Key::StatusFetchFailed),
                &[&format!("{error:#}")],
            )),
        },
        HostOutcome::Actioned(Ok(report)) => {
            // Footer shows a 5s summary; the popup persists.
            app.set_status(report.status.clone());
            app.pending_refresh_after_close = report.changed;
            app.report = Some(report);
        }
        HostOutcome::Actioned(Err(error)) => app.set_status(fmt_key(
            app.lang.t(Key::StatusActionFailed),
            &[&format!("{error:#}")],
        )),
        HostOutcome::Writeups { vm, result } => match result {
            Ok(entries) => {
                if entries.is_empty() {
                    app.set_status(fmt_key(app.lang.t(Key::StatusNoWriteups), &[&vm]));
                } else {
                    app.writeups_popup = Some(WriteupsPopup {
                        vm,
                        entries,
                        selected: 0,
                    });
                }
            }
            Err(error) => app.set_status(fmt_key(
                app.lang.t(Key::StatusFetchFailed),
                &[&format!("{error:#}")],
            )),
        },
        HostOutcome::Configured { username, result } => match result {
            Ok(()) => {
                app.needs_config = false;
                app.set_status(fmt_key(app.lang.t(Key::StatusConnectedAs), &[&username]));
                *pending_fetch = true;
            }
            Err(error) => {
                app.set_status(fmt_key(
                    app.lang.t(Key::StatusConfigFailed),
                    &[&format!("{error:#}")],
                ));
                app.open_config_popup(ConfigContext::LoginFailed, Some(&username));
            }
        },
        HostOutcome::LoggedOut(Ok(())) => {
            app.needs_config = true;
            app.tab = Tab::Stats;
            app.input_mode = InputMode::Normal;
            app.view = ViewMode::Normal;
            app.filter.clear();
            app.set_data(TuiData::empty());
            app.data.stats.username.clear();
            app.open_config_popup(ConfigContext::LoggedOut, None);
            app.set_status(app.lang.t(Key::StatusLoggedOut));
        }
        HostOutcome::LoggedOut(Err(error)) => {
            app.set_status(fmt_key(
                app.lang.t(Key::StatusLogoutFailed),
                &[&format!("{error:#}")],
            ));
        }
        HostOutcome::Submissions { result } => match result {
            Ok((queue, rules, form)) => {
                app.data.submissions = queue;
                if rules.is_some() {
                    app.data.rules = rules;
                }
                if !form.is_empty() {
                    app.data.submission_form = form;
                }
                if app.tab == Tab::Submissions {
                    app.reset_list_position();
                }
            }
            Err(error) => app.set_status(fmt_key(
                app.lang.t(Key::StatusFetchFailed),
                &[&format!("{error:#}")],
            )),
        },
    }
}

fn handle_key(app: &mut AppState, key: crossterm::event::KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.request_quit();
        return;
    }

    // Account popup captures everything until dismissed.
    if app.popup.as_ref().map(|p| p.kind) == Some(PopupKind::Account) {
        match key.code {
            KeyCode::Enter => app.begin_account_switch(),
            KeyCode::Char('l') => {
                app.popup = None;
                app.pending_logout = true;
            }
            KeyCode::Esc | KeyCode::Char('q') => app.popup = None,
            _ => {}
        }
        return;
    }

    // Writeups popup captures everything until dismissed.
    if app.writeups_popup.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.close_writeups_popup(),
            KeyCode::Enter => app.open_selected_writeup_link(),
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(popup) = app.writeups_popup.as_mut() {
                    popup.move_selection(-1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(popup) = app.writeups_popup.as_mut() {
                    popup.move_selection(1);
                }
            }
            _ => {}
        }
        return;
    }

    // Result report popup captures everything until dismissed. Closing it
    // with `changed` set queues the deferred refresh (Option A).
    if app.report.is_some() {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                app.refresh_requested |= app.close_report();
            }
            _ => {}
        }
        return;
    }

    // Popup input mode captures everything first.
    if app.popup.is_some() {
        // Read-only popups (already-PWNED machines) just close.
        if app.popup.as_ref().map(|p| p.readonly) == Some(true) {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.popup = None,
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc => {
                let is_config = app.popup.as_ref().map(|p| p.kind) == Some(PopupKind::Config);
                app.popup = None;
                if is_config {
                    // Nothing else to do without an account — leave.
                    app.quit = true;
                } else {
                    app.set_status(app.lang.t(Key::StatusCancelled));
                }
            }
            KeyCode::Enter => app.confirm_popup(),
            KeyCode::Backspace => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.pop();
                }
            }
            KeyCode::Up => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.previous_field();
                }
            }
            KeyCode::Tab => {
                let is_download = app.popup.as_ref().map(|p| p.kind) == Some(PopupKind::Download);
                if is_download {
                    if let Some(popup) = app.popup.as_mut() {
                        popup.complete_destination();
                    }
                } else if let Some(popup) = app.popup.as_mut() {
                    popup.next_field();
                }
            }
            KeyCode::Right | KeyCode::Left => {
                // ←/→ on a <select> field cycles its locked value
                // (Easy -> Medium -> Hard); on text fields it moves fields.
                let select_options: Option<Vec<String>> = app.popup.as_ref().and_then(|popup| {
                    app.data
                        .submission_form
                        .get(popup.field)
                        .and_then(|f| match &f.kind {
                            FieldKind::Select(options) => Some(options.clone()),
                            _ => None,
                        })
                });
                let forward = key.code == KeyCode::Right;
                if let Some(options) = select_options {
                    if let Some(popup) = app.popup.as_mut() {
                        popup.cycle_value_step(&options, forward);
                    }
                } else if forward {
                    if let Some(popup) = app.popup.as_mut() {
                        popup.next_field();
                    }
                } else if let Some(popup) = app.popup.as_mut() {
                    popup.previous_field();
                }
            }
            KeyCode::Down => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.next_field();
                }
            }
            KeyCode::Char(c) => {
                // Locked select fields accept no typing.
                let is_locked_select = app
                    .popup
                    .as_ref()
                    .and_then(|popup| app.data.submission_form.get(popup.field))
                    .is_some_and(|f| matches!(f.kind, FieldKind::Select(_)));
                if !is_locked_select {
                    if let Some(popup) = app.popup.as_mut() {
                        popup.push(c);
                    }
                }
            }
            _ => {}
        }
        return;
    }

    match app.input_mode {
        InputMode::Filter => match key.code {
            KeyCode::Esc => {
                app.clear_filter();
                app.input_mode = InputMode::Normal;
            }
            KeyCode::Enter => app.input_mode = InputMode::Normal,
            KeyCode::Backspace => app.filter_pop(),
            KeyCode::Char(c) => app.filter_push(c),
            _ => {}
        },
        InputMode::Normal => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.request_quit(),
            KeyCode::Char('r') => app.request_refresh(),
            KeyCode::Tab | KeyCode::Right => app.next_tab(),
            KeyCode::Left | KeyCode::BackTab => app.previous_tab(),
            KeyCode::Down | KeyCode::Char('j') => app.move_down(),
            KeyCode::Up | KeyCode::Char('k') => app.move_up(),
            KeyCode::Home | KeyCode::Char('g') => app.move_start(),
            KeyCode::Char('/') => app.enter_filter_mode(),
            KeyCode::Char('a') => app.open_account_popup(),
            KeyCode::Char('l') => app.toggle_language(),
            KeyCode::Char('s') => app.cycle_machine_sort(),
            KeyCode::Char('h') if app.tab == Tab::Machines => app.toggle_hide_pwned(),
            KeyCode::Char('f') => app.open_action_popup(PopupKind::Flag),
            KeyCode::Char('u') => app.open_action_popup(PopupKind::Upload),
            KeyCode::Char('d') => app.open_action_popup(PopupKind::Download),
            KeyCode::Char('w') => app.open_writeups_popup(),
            KeyCode::Char('v') if app.tab == Tab::Submissions => app.open_submission_form(),
            KeyCode::Char('i') if app.tab == Tab::Submissions => app.open_rules_popup(),
            KeyCode::Char('o') => app.toggle_downloads_view(),
            KeyCode::Char('c') if app.view == ViewMode::Downloads => {
                // Cancel the most recent active download from the overlay.
                if let Some(job) = app.download_jobs.iter().rev().find(|job| job.is_active()) {
                    job.request_cancel();
                    app.set_status(fmt_key(app.lang.t(Key::StatusCancelDownload), &[&job.vm]));
                }
            }
            KeyCode::Enter => app.open_selected_link(),
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn sample_data() -> TuiData {
        use crate::modules::releases::Release;
        use crate::modules::stats::ProfileWriteup;
        TuiData {
            stats: ProfileStats {
                username: "demouser".into(),
                accepted_writeups: vec![
                    ProfileWriteup {
                        vm: "Economists".into(),
                        language: "English".into(),
                        url: "https://example.com/economists.md".into(),
                    },
                    ProfileWriteup {
                        vm: "Za1".into(),
                        language: "English".into(),
                        url: "https://example.com/za1.md".into(),
                    },
                    ProfileWriteup {
                        vm: "Fuxa".into(),
                        language: String::new(),
                        url: "https://example.com/fuxa.md".into(),
                    },
                ],
                ..Default::default()
            },
            progress: vec![("Total VMs".into(), 166, 371)],
            pending: vec!["Fuxa".to_string(), "Liar".to_string(), "Rooted".to_string()],
            catalog: vec![
                Machine {
                    name: "Fuxa".into(),
                    creator: "0xM4r10".into(),
                    size: "0.5 Gb".into(),
                    difficulty: "beginner".into(),
                    os: "linux".into(),
                    tested: String::new(),
                    status: "PWNED".into(),
                },
                Machine {
                    name: "Nebula1".into(),
                    creator: "Sublarge".into(),
                    size: "1.3 Gb".into(),
                    difficulty: "advanced".into(),
                    os: "linux".into(),
                    tested: String::new(),
                    status: "TO HACK".into(),
                },
                Machine {
                    name: "Arcane".into(),
                    creator: "asya2ross".into(),
                    size: "0.8 Gb".into(),
                    difficulty: "intermediate".into(),
                    os: "linux".into(),
                    tested: String::new(),
                    status: "DONE".into(),
                },
            ],
            releases: vec![
                Release {
                    date: "03-Sept".into(),
                    name: "Arcane".into(),
                    os: "linux".into(),
                    released: true,
                },
                Release {
                    date: "09-Sept".into(),
                    name: "INVERNADERO_1.0".into(),
                    os: "linux".into(),
                    released: false,
                },
            ],
            submissions: vec![],
            submission_form: vec![],
            rules: None,
        }
    }

    fn app() -> AppState {
        AppState::new(sample_data())
    }

    #[test]
    fn tab_navigation_resets_selection() {
        let mut state = app();
        state.next_tab();
        assert_eq!(state.tab, Tab::Writeups);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.next_tab();
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
        state.previous_tab();
        state.previous_tab();
        assert_eq!(state.tab, Tab::Stats);
    }

    #[test]
    fn filter_narrows_lists_and_clears() {
        let mut state = app();
        state.filter_push('f');
        state.filter_push('u');
        assert_eq!(state.visible_pending().len(), 1);
        assert_eq!(state.visible_pending()[0].as_str(), "Fuxa");
        state.clear_filter();
        assert_eq!(state.visible_pending().len(), 3);
    }

    #[test]
    fn selection_clamps_to_visible_rows() {
        let mut state = app();
        state.next_tab(); // Writeups
        state.next_tab(); // Pending (3 rows)
        state.set_visible_rows(2);
        state.move_down();
        state.move_down();
        assert_eq!(state.selected, 2);
        assert_eq!(state.scroll, 1);
        state.move_up();
        state.move_up();
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
        // Cannot run past the end.
        for _ in 0..10 {
            state.move_down();
        }
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn status_expires_after_lifetime() {
        let mut state = app();
        state.set_status("Data refreshed.");
        assert!(state.status.is_some());

        // Not yet expired.
        state.tick();
        assert!(state.status.is_some());

        // Simulate expiry passing.
        state.status_expiry = Some(std::time::Instant::now() - Duration::from_secs(1));
        state.tick();
        assert!(state.status.is_none());
        assert!(state.status_expiry.is_none());
    }

    #[test]
    fn request_refresh_queues_request_only() {
        let mut state = app();
        assert!(state.fetching.is_none());
        state.request_refresh();
        // The label is owned by the event loop; the request alone triggers it.
        assert!(state.refresh_requested);
        assert!(state.fetching.is_none());
        assert!(state.should_fetch(false));

        // While a fetch is running (label set), further requests are ignored.
        state.refresh_requested = false;
        state.fetching = Some("Refreshing data...".to_string());
        state.request_refresh();
        assert!(!state.refresh_requested);
        assert!(!state.should_fetch(false));
    }

    #[test]
    fn machines_tab_filters_and_selects() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab();
        assert_eq!(state.tab, Tab::Machines);
        assert_eq!(state.visible_machines().len(), 3);

        assert_eq!(state.selected_machine_name().as_deref(), Some("Fuxa"));
        state.filter_push('n');
        state.filter_push('e');
        state.filter_push('b');
        assert_eq!(state.visible_machines().len(), 1);
        assert_eq!(state.selected_machine_name().as_deref(), Some("Nebula1"));
    }

    #[test]
    fn popup_flow_queues_action() {
        let mut state = app();
        // Flags are gated to the Machines tab.
        state.next_tab();
        state.next_tab();
        state.next_tab();
        assert_eq!(state.tab, Tab::Machines);
        // Select the TO HACK machine — writable popup.
        state.move_down();
        assert_eq!(state.selected_machine_name().as_deref(), Some("Nebula1"));

        state.open_action_popup(PopupKind::Flag);
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Flag);
        assert_eq!(popup.vm, "Nebula1");
        assert_eq!(popup.buffers.len(), 2, "flag popup has user+root fields");
        assert!(popup.notice.is_none(), "TO HACK machines have no notice");

        state.popup.as_mut().unwrap().buffers[0].push_str("flag{abc}");
        state.confirm_popup();
        assert!(state.popup.is_none());
        let action = state.pending_action.take().unwrap();
        assert_eq!(action.vm, "Nebula1");
        assert_eq!(action.values, vec![(0, "flag{abc}".to_string())]);
        assert_eq!(action.kind, PopupKind::Flag);
    }

    #[test]
    fn flag_popup_reflects_machine_status() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab(); // Machines

        // Fuxa (PWNED, row 0): read-only info popup.
        state.open_action_popup(PopupKind::Flag);
        let popup = state.popup.as_ref().unwrap();
        assert!(popup.readonly);
        assert!(popup.notice.is_none());
        assert!(popup.buffers.is_empty());
        state.popup = None;

        // Nebula1 (TO HACK, row 1): plain writable popup.
        state.move_down();
        state.open_action_popup(PopupKind::Flag);
        let popup = state.popup.as_ref().unwrap();
        assert!(!popup.readonly);
        assert!(popup.notice.is_none());
        state.popup = None;

        // Arcane (DONE, row 2): writable popup with the "one remains" notice.
        state.move_down();
        state.open_action_popup(PopupKind::Flag);
        let popup = state.popup.as_ref().unwrap();
        assert!(!popup.readonly);
        assert_eq!(
            popup.notice.as_deref(),
            Some("One flag already submitted — one remains.")
        );
    }

    #[test]
    fn readonly_popup_cannot_submit() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab(); // Fuxa (PWNED) selected

        state.open_action_popup(PopupKind::Flag);
        assert!(state.popup.as_ref().unwrap().readonly);

        // Typing is swallowed entirely.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()),
        );
        assert_eq!(state.popup.as_ref().unwrap().buffers.len(), 0);

        // Enter just closes it — no action is ever queued.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert!(state.popup.is_none());
        assert!(state.pending_action.is_none());
    }

    #[test]
    fn dual_flag_popup_queues_both_values() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab(); // Machines
        state.move_down(); // Nebula1 (TO HACK)

        state.open_action_popup(PopupKind::Flag);
        // Fill the user flag, then hop to the root field (Tab) and fill it.
        state.popup.as_mut().unwrap().buffers[0].push_str("flag{user}");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(state.popup.as_ref().unwrap().field, 1);
        state.popup.as_mut().unwrap().buffers[1].push_str("flag{root}");
        state.confirm_popup();

        let action = state.pending_action.take().unwrap();
        assert_eq!(
            action.values,
            vec![(0, "flag{user}".to_string()), (1, "flag{root}".to_string())]
        );
    }

    #[test]
    fn build_flag_report_labels_original_fields() {
        use crate::modules::flag::FlagVerdict;

        // User accepted, Root rejected.
        let report = super::build_flag_report(
            crate::i18n::Lang::En,
            "Arcane",
            vec![(0, FlagVerdict::Correct), (1, FlagVerdict::Wrong)],
        );
        assert_eq!(report.title, " Flag results — Arcane ");
        assert_eq!(report.entries[0].0, ReportKind::Success);
        assert_eq!(report.entries[0].1, "User flag: ✓ ACCEPTED");
        assert_eq!(report.entries[1].0, ReportKind::Failure);
        assert_eq!(report.entries[1].1, "Root flag: ✗ REJECTED");
        assert!(report.changed);
        assert_eq!(report.status, "[+] User ✓ · Root ✗");

        // Root field only (index 1) keeps its Root label — not shifted.
        let report = super::build_flag_report(
            crate::i18n::Lang::En,
            "Arcane",
            vec![(1, FlagVerdict::Wrong)],
        );
        assert_eq!(report.entries[0].1, "Root flag: ✗ REJECTED");
        assert!(!report.changed);

        // Lone accepted flag keeps the celebratory footer.
        let report = super::build_flag_report(
            crate::i18n::Lang::En,
            "Arcane",
            vec![(0, FlagVerdict::Correct)],
        );
        assert_eq!(report.status, "[✓] You hacked Arcane!");

        // All rejected -> no refresh.
        let report = super::build_flag_report(
            crate::i18n::Lang::En,
            "Arcane",
            vec![(0, FlagVerdict::Wrong), (1, FlagVerdict::Wrong)],
        );
        assert!(!report.changed);
    }

    #[test]
    fn report_persists_until_dismissed_then_refreshes() {
        use crate::modules::flag::FlagVerdict;

        let mut state = app();
        state.report = Some(super::build_flag_report(
            crate::i18n::Lang::En,
            "Arcane",
            vec![(0, FlagVerdict::Correct)],
        ));
        state.pending_refresh_after_close = true;

        // Any other key is swallowed while the report is open.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()),
        );
        assert!(state.report.is_some());

        // Dismissal queues the deferred refresh (Option A).
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert!(state.report.is_none());
        assert!(state.refresh_requested);

        // Without changes, closing never refreshes.
        state.report = Some(super::build_flag_report(
            crate::i18n::Lang::En,
            "Arcane",
            vec![(0, FlagVerdict::Wrong)],
        ));
        state.pending_refresh_after_close = false;
        state.refresh_requested = false;
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );
        assert!(!state.refresh_requested);
    }

    #[test]
    fn releases_tab_lists_and_filters() {
        let mut state = app();
        for _ in 0..4 {
            state.next_tab();
        }
        assert_eq!(state.tab, Tab::Releases);
        assert_eq!(state.visible_releases().len(), 2);
        assert_eq!(state.visible_releases()[0].name, "Arcane");

        state.filter_push('i');
        state.filter_push('n');
        state.filter_push('v');
        assert_eq!(state.visible_releases().len(), 1);
        assert_eq!(state.visible_releases()[0].name, "INVERNADERO_1.0");
        assert!(!state.visible_releases()[0].released);
    }

    #[test]
    fn o_key_toggles_downloads_view() {
        let mut state = app();
        assert_eq!(state.view, ViewMode::Normal);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::empty()),
        );
        assert_eq!(state.view, ViewMode::Downloads);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::empty()),
        );
        assert_eq!(state.view, ViewMode::Normal);
    }

    #[test]
    fn download_popup_flow_and_gate() {
        let mut state = app();
        assert_eq!(state.tab, super::Tab::Stats, "initial tab");

        // 'd' is gated to the Machines tab.
        state.next_tab(); // Writeups
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::empty()),
        );
        assert!(state.popup.is_none());

        // On Machines it opens with a destination field.
        state.next_tab(); // Pending
        state.next_tab(); // Machines
        assert_eq!(state.tab, super::Tab::Machines, "before d");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::empty()),
        );
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Download);
        assert!(!popup.buffers[0].is_empty(), "destination prefilled");

        // Typing a path and confirming queues the download action.
        state.popup.as_mut().unwrap().buffers[0] = "/tmp/vm-lab".to_string();
        state.confirm_popup();
        let action = state.pending_action.take().unwrap();
        assert_eq!(action.kind, PopupKind::Download);
        assert_eq!(action.values, vec![(0, "/tmp/vm-lab".to_string())]);
    }

    #[test]
    fn quit_warns_once_while_downloads_are_active() {
        let mut state = app();
        state.download_jobs = vec![std::sync::Arc::new(crate::tui::downloads::DownloadJob {
            vm: "Xslib".into(),
            state: std::sync::Arc::new(std::sync::Mutex::new(
                crate::tui::downloads::DownloadState::default(),
            )),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })];

        // First q warns instead of quitting.
        state.request_quit();
        assert!(!state.quit);
        assert!(state
            .status
            .as_deref()
            .unwrap()
            .contains("q again to abort"));

        // Second q quits (abort handled by the event loop).
        state.request_quit();
        assert!(state.quit);
    }

    #[test]
    fn action_keys_are_tab_gated() {
        // 'f' on Pending -> blocked; 'u' on Pending -> allowed.
        let mut state = app();
        state.next_tab(); // Writeups
        state.next_tab(); // Pending
        state.popup = None;
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::empty()),
        );
        assert!(
            state.popup.is_none(),
            "flag popup must be blocked on Pending"
        );
        assert!(state.status.is_some());

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::empty()),
        );
        assert!(state.popup.is_some(), "upload popup allowed on Pending");
        state.popup = None;

        // 'u' on Machines -> blocked; 'f' on Machines -> allowed.
        state.next_tab(); // Machines
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::empty()),
        );
        assert!(
            state.popup.is_none(),
            "upload popup must be blocked on Machines"
        );
        assert!(state.status.is_some());

        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::empty()),
        );
        assert!(state.popup.is_some(), "flag popup allowed on Machines");
    }

    #[test]
    fn popup_rejects_empty_input_and_double_open() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab(); // Machines
        state.open_action_popup(PopupKind::Flag);
        state.confirm_popup(); // empty buffers -> cancelled, no action
        assert!(state.popup.is_none());
        assert!(state.pending_action.is_none());

        state.open_action_popup(PopupKind::Flag);
        assert!(state.popup.is_some());
        state.open_action_popup(PopupKind::Flag); // ignored while open
        assert_eq!(state.popup.as_ref().unwrap().kind, PopupKind::Flag);
    }

    #[test]
    fn destination_completion_lists_and_completes() {
        let base = std::env::temp_dir().join(format!("hmv-tui-compl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("alpha")).unwrap();
        std::fs::create_dir_all(base.join("alphabet")).unwrap();
        std::fs::create_dir_all(base.join("beta")).unwrap();
        std::fs::write(base.join("file.txt"), "x").unwrap(); // files never complete

        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab(); // Machines
        state.open_action_popup(PopupKind::Download);
        state.popup.as_mut().unwrap().buffers[0] = format!("{}/", base.display());

        // Tab with a trailing separator: lists every directory, input kept.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.completions, ["alpha", "alphabet", "beta"]);
        assert_eq!(popup.buffers[0], format!("{}/", base.display()));

        // Partial input completes the shared prefix of the candidates.
        state.popup.as_mut().unwrap().buffers[0] = format!("{}/al", base.display());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.buffers[0], format!("{}/alpha", base.display()));
        assert_eq!(popup.completions, ["alpha", "alphabet"]);

        // Single match: completes fully with the trailing separator.
        state.popup.as_mut().unwrap().buffers[0] = format!("{}/b", base.display());
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.buffers[0], format!("{}/beta/", base.display()));

        // Typing clears the stale listing.
        state.popup.as_mut().unwrap().push('x');
        assert!(state.popup.as_ref().unwrap().completions.is_empty());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn loading_state_shows_placeholder() {
        let state = AppState::loading();
        assert_eq!(state.fetching, Some("Loading data...".to_string()));
        assert!(state.data.stats.accepted_writeups.is_empty());
        assert!(state.data.pending.is_empty());
        // Lists stay safe to render with no data.
        assert!(state.visible_writeups().is_empty());
        assert!(state.visible_pending().is_empty());
    }

    #[test]
    fn handle_key_maps_normal_mode_keys() {
        let mut state = app();
        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(KeyCode::Tab, crossterm::event::KeyModifiers::empty()),
        );
        assert_eq!(state.tab, Tab::Writeups);

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(
                KeyCode::Char('j'),
                crossterm::event::KeyModifiers::empty(),
            ),
        );
        assert_eq!(state.selected, 1);

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(
                KeyCode::Char('/'),
                crossterm::event::KeyModifiers::empty(),
            ),
        );
        assert_eq!(state.input_mode, InputMode::Filter);

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(
                KeyCode::Char('x'),
                crossterm::event::KeyModifiers::empty(),
            ),
        );
        assert_eq!(state.filter, "x");

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::empty()),
        );
        assert_eq!(state.input_mode, InputMode::Normal);
        assert!(state.filter.is_empty());

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(
                KeyCode::Char('q'),
                crossterm::event::KeyModifiers::empty(),
            ),
        );
        assert!(state.quit);
    }

    #[test]
    fn unconfigured_opens_config_popup() {
        let state = AppState::unconfigured(Some("demouser"));
        assert!(state.needs_config);
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Config);
        assert_eq!(popup.buffers[0], "demouser");
        assert!(popup.buffers[1].is_empty(), "password never prefilled");
        assert!(popup.notice.is_some());
        assert!(state.data.stats.accepted_writeups.is_empty());
    }

    #[test]
    fn config_popup_requires_both_fields() {
        let mut state = AppState::unconfigured(None);
        state.popup.as_mut().unwrap().buffers[0] = "someuser".into();
        state.confirm_popup();

        // Popup stays open, nothing queued, error shown.
        assert!(state.pending_action.is_none());
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Config);
        assert_eq!(popup.buffers[0], "someuser");

        // Both fields filled -> credentials queued in field order.
        state.popup.as_mut().unwrap().buffers[1] = "hunter2".into();
        state.confirm_popup();
        let action = state.pending_action.take().unwrap();
        assert_eq!(action.kind, PopupKind::Config);
        assert_eq!(
            action.values,
            vec![(0, "someuser".to_string()), (1, "hunter2".to_string())]
        );
    }

    #[test]
    fn config_popup_esc_quits() {
        let mut state = AppState::unconfigured(None);
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );
        assert!(state.quit);
        assert!(state.popup.is_none());
    }

    #[test]
    fn machines_size_sort_cycles() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab(); // Machines
        assert_eq!(state.machine_sort, MachineSort::Default);
        fn names(state: &AppState) -> Vec<&str> {
            state
                .visible_machines()
                .iter()
                .map(|m| m.name.as_str())
                .collect()
        }
        // Site order: Fuxa (0.5 Gb), Nebula1 (1.3 Gb), Arcane (0.8 Gb).
        assert_eq!(names(&state), ["Fuxa", "Nebula1", "Arcane"]);

        // s -> smallest first.
        state.cycle_machine_sort();
        assert_eq!(state.machine_sort, MachineSort::SizeAsc);
        assert_eq!(names(&state), ["Fuxa", "Arcane", "Nebula1"]);

        // s -> largest first.
        state.cycle_machine_sort();
        assert_eq!(state.machine_sort, MachineSort::SizeDesc);
        assert_eq!(names(&state), ["Nebula1", "Arcane", "Fuxa"]);

        // s -> back to site order.
        state.cycle_machine_sort();
        assert_eq!(state.machine_sort, MachineSort::Default);
        assert_eq!(names(&state), ["Fuxa", "Nebula1", "Arcane"]);

        // Sorting composes with the filter.
        state.cycle_machine_sort(); // size ↑
        for character in ['n', 'e', 'b'] {
            state.filter_push(character);
        }
        assert_eq!(names(&state), ["Nebula1"]);
    }

    #[test]
    fn size_sort_is_gated_to_machines_tab() {
        let mut state = app();
        assert_eq!(state.tab, Tab::Stats);
        state.cycle_machine_sort();
        assert_eq!(state.machine_sort, MachineSort::Default);
        assert!(state.status.is_some());
    }

    #[test]
    fn account_popup_flow() {
        let mut state = app();

        // 'a' opens the account popup carrying the active username.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
        );
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Account);
        assert_eq!(popup.vm, "demouser");

        // Esc closes without side effects.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );
        assert!(state.popup.is_none());
        assert!(!state.pending_logout);

        // l queues a logout.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::empty()),
        );
        assert!(state.pending_logout);
        assert!(state.popup.is_none());

        // Enter opens the switch popup prefilled with the current account.
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
        );
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Config);
        assert_eq!(popup.buffers[0], "demouser");
        assert!(state.pending_action.is_none());
    }

    #[test]
    fn account_popup_is_gated_while_unconfigured() {
        let mut state = AppState::unconfigured(None);
        state.open_account_popup();
        // The config popup stays the modal: no account popup may replace it.
        assert_eq!(state.popup.as_ref().unwrap().kind, PopupKind::Config);
        assert!(!state.pending_logout);
    }

    #[test]
    fn selected_url_only_on_writeups_tab() {
        let mut state = app();
        assert!(state.selected_writeup_url().is_none()); // Stats tab
        state.next_tab();
        assert_eq!(state.tab, Tab::Writeups);
        // Row 0: Economists.
        assert_eq!(
            state.selected_writeup_url(),
            Some("https://example.com/economists.md")
        );
        // Filter 'z' narrows to Za1.
        state.filter_push('z');
        state.reset_list_position_for_test();
        assert_eq!(
            state.selected_writeup_url(),
            Some("https://example.com/za1.md")
        );
        state.next_tab(); // Pending
        assert!(state.selected_writeup_url().is_none());
    }

    #[test]
    fn set_data_preserves_selection_by_name() {
        let mut state = app();
        state.next_tab(); // Writeups
        state.next_tab(); // Pending
        state.next_tab(); // Machines

        // Select Nebula1 (row 1 of the catalog).
        state.move_down();

        assert_eq!(state.selected_machine_name().as_deref(), Some("Nebula1"));

        // A fresh catalog with a different order and extra rows keeps the
        // selection on Nebula1.
        let mut data = sample_data();
        data.catalog.insert(0, data.catalog[2].clone()); // Arcane first
        data.catalog.push(Machine {
            name: "Zephyr".into(),
            creator: "someone".into(),
            size: "1.0 Gb".into(),
            difficulty: "beginner".into(),
            os: "windows".into(),
            tested: String::new(),
            status: "TO HACK".into(),
        });
        state.set_data(data);
        assert_eq!(state.selected_machine_name().as_deref(), Some("Nebula1"));
        assert!(state.selected > 0);
    }

    #[test]
    fn set_data_resets_when_selection_missing() {
        let mut state = app();
        state.next_tab(); // Writeups
        state.next_tab(); // Pending
        state.next_tab(); // Machines
        state.move_down();
        state.move_down(); // Arcane (row 2)
        assert_eq!(state.selected_machine_name().as_deref(), Some("Arcane"));

        // New data without Arcane: fall back to the top of the list.
        let mut data = sample_data();
        data.catalog.retain(|m| m.name != "Arcane");
        state.set_data(data);
        assert_eq!(state.selected, 0);
        assert_eq!(state.selected_machine_name().as_deref(), Some("Fuxa"));
    }

    #[test]
    fn busy_fetch_gates_network_actions() {
        let mut state = app();
        state.next_tab(); // Writeups
        state.next_tab(); // Pending
        state.next_tab(); // Machines
        state.fetching = Some("Refreshing data...".to_string());
        assert!(
            state.popup.is_none(),
            "flag popup is blocked while fetching"
        );
        state.open_writeups_popup();
        assert!(
            state.pending_writeups.is_none(),
            "writeups fetch is blocked while fetching"
        );
        state.open_account_popup();
        assert!(
            state.popup.is_none(),
            "account popup is blocked while fetching"
        );
        state.request_refresh();
        assert!(
            !state.refresh_requested,
            "refresh is blocked while fetching"
        );

        // Idle again: the same actions go through.
        state.fetching = None;
        state.open_action_popup(PopupKind::Flag);
        assert_eq!(state.popup.as_ref().map(|p| p.kind), Some(PopupKind::Flag));
    }

    #[test]
    fn hide_pwned_toggles_and_filters() {
        let mut state = app();
        state.next_tab(); // Writeups
        state.next_tab(); // Pending
        state.next_tab(); // Machines

        // Catalog sample: Fuxa (PWNED), Nebula1 (TO HACK), Arcane (DONE).
        assert_eq!(state.visible_machines().len(), 3);

        // `h` hides the PWNED machine and resets the selection.
        state.move_down(); // Nebula1
        state.toggle_hide_pwned();
        assert!(state.hide_pwned);
        let names: Vec<&str> = state
            .visible_machines()
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(names, ["Nebula1", "Arcane"]);
        assert_eq!(state.selected, 0, "toggle resets the selection");

        // Text filter still applies on top of the hide: "ar" matches
        // Arcane (name) and Nebula1 (creator "Sublarge"), but not the
        // hidden PWNED Fuxa.
        state.filter_push('a');
        state.filter_push('r');
        assert_eq!(state.visible_machines().len(), 2);
        state.clear_filter();

        // Second `h` shows everything again.
        state.toggle_hide_pwned();
        assert!(!state.hide_pwned);
        assert_eq!(state.visible_machines().len(), 3);

        // Gated to the Machines tab like the other machine actions.
        state.previous_tab(); // Pending
        state.previous_tab(); // Writeups
        state.previous_tab(); // Stats
        assert_eq!(state.tab, Tab::Stats);
        state.toggle_hide_pwned();
        assert!(!state.hide_pwned);
    }
}
