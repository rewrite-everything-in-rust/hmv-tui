//! Interactive dashboard: application state, input handling and the event
//! loop. Rendering lives in `render.rs`; all state transitions here are pure
//! and unit-tested.

pub mod render;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::modules::flag::FlagVerdict;
use crate::modules::machines::Machine;
use crate::modules::releases::Release;
use crate::modules::stats::{ProfileStats, ProfileWriteup};

/// What a popup asks the user for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Flag,
    Upload,
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
}

impl Popup {
    pub fn push(&mut self, c: char) {
        if let Some(buffer) = self.buffers.get_mut(self.field) {
            buffer.push(c);
        }
    }

    pub fn pop(&mut self) {
        if let Some(buffer) = self.buffers.get_mut(self.field) {
            buffer.pop();
        }
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
/// is true, a data refresh is queued for when the user closes it (Opsi A).
#[derive(Debug, Clone)]
pub struct ActionReport {
    pub title: String,
    pub entries: Vec<(ReportKind, String)>,
    pub changed: bool,
    /// Short footer summary (5s expiry), separate from the popup.
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Stats,
    Writeups,
    Pending,
    Machines,
    Releases,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Stats,
        Tab::Writeups,
        Tab::Pending,
        Tab::Machines,
        Tab::Releases,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Stats => "Stats",
            Tab::Writeups => "Writeups",
            Tab::Pending => "Pending",
            Tab::Machines => "Machines",
            Tab::Releases => "Releases",
        }
    }

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

pub struct AppState {
    pub tab: Tab,
    pub input_mode: InputMode,
    pub filter: String,
    pub selected: usize,
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
    /// Action result popup (persists until dismissed).
    pub report: Option<ActionReport>,
    /// Refresh queued for when the report popup closes (Opsi A).
    pub pending_refresh_after_close: bool,
    pub data: TuiData,
    /// Row budget reported by the renderer after layout.
    pub last_visible_rows: Option<usize>,
}

/// How long a status message stays visible in the footer.
const STATUS_LIFETIME: Duration = Duration::from_secs(5);

impl AppState {
    pub fn new(data: TuiData) -> Self {
        Self {
            tab: Tab::Stats,
            input_mode: InputMode::Normal,
            filter: String::new(),
            selected: 0,
            scroll: 0,
            quit: false,
            refresh_requested: false,
            fetching: None,
            status: None,
            status_expiry: None,
            popup: None,
            pending_action: None,
            report: None,
            pending_refresh_after_close: false,
            data,
            last_visible_rows: None,
        }
    }

    /// Entry state for `hmv tui`: draws immediately, then loads all data.
    pub fn loading() -> Self {
        let mut state = Self::new(TuiData::empty());
        state.fetching = Some("Loading data...".to_string());
        state
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

    pub fn set_data(&mut self, data: TuiData) {
        self.data = data;
        self.selected = 0;
        self.scroll = 0;
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
    /// matches name, difficulty, creator or status.
    pub fn visible_machines(&self) -> Vec<&Machine> {
        let needle = self.filter.to_lowercase();
        self.data
            .catalog
            .iter()
            .filter(|m| {
                needle.is_empty()
                    || m.name.to_lowercase().contains(&needle)
                    || m.difficulty.to_lowercase().contains(&needle)
                    || m.creator.to_lowercase().contains(&needle)
                    || m.status.to_lowercase().contains(&needle)
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
        if self.popup.is_some() {
            return;
        }
        let allowed = match kind {
            PopupKind::Flag => self.tab == Tab::Machines,
            PopupKind::Upload => self.tab == Tab::Pending,
        };
        if !allowed {
            self.set_status(match kind {
                PopupKind::Flag => {
                    "Flag submission is only available on the Machines tab."
                }
                PopupKind::Upload => {
                    "Writeup submission is only available on the Pending tab."
                }
            });
            return;
        }
        let Some(vm) = self.selected_machine_name() else {
            self.set_status("Nothing selected to act on.");
            return;
        };

        if kind == PopupKind::Flag {
            let status = self
                .selected_machine()
                .map(|m| m.status.to_uppercase())
                .unwrap_or_default();
            if status.contains("PWNED") || status.contains("DONE") && status == "PWNED" {
                // Fully completed machine: show an info box, no inputs.
                self.popup = Some(Popup {
                    kind,
                    vm,
                    buffers: Vec::new(),
                    field: 0,
                    notice: None,
                    readonly: true,
                });
                return;
            }
            let notice = if status.contains("DONE") {
                Some("One flag already submitted — one remains.".to_string())
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
        });
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
            self.set_status(format!("{} is already PWNED — nothing to submit.", popup.vm));
            return;
        }
        let values: Vec<(usize, String)> = popup
            .buffers
            .iter()
            .enumerate()
            .map(|(index, b)| (index, b.trim().to_string()))
            .filter(|(_, v)| !v.is_empty())
            .collect();

        if values.is_empty() {
            self.set_status("Cancelled — empty input.");
            return;
        }

        let kind_label = match popup.kind {
            PopupKind::Flag => {
                if values.len() > 1 {
                    "flags"
                } else {
                    "flag"
                }
            }
            PopupKind::Upload => "writeup URL",
        };
        self.set_status(format!("Queued {} for {}...", kind_label, popup.vm));
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
                Ok(_) => format!("Opened in browser: {url}"),
                Err(error) => format!("xdg-open failed: {error}"),
            });
        }
    }

    pub fn request_quit(&mut self) {
        self.quit = true;
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
pub fn build_flag_report(vm: &str, results: Vec<(usize, FlagVerdict)>) -> ActionReport {
    let mut entries = Vec::new();
    let mut compact = Vec::new();
    let mut changed = false;

    for (field, verdict) in results {
        let label = if field == 0 { "User flag" } else { "Root flag" };
        let short = if field == 0 { "User" } else { "Root" };
        match verdict {
            FlagVerdict::Correct => {
                entries.push((ReportKind::Success, format!("{label}: ✓ ACCEPTED")));
                compact.push(format!("{short} ✓"));
                changed = true;
            }
            FlagVerdict::Wrong => {
                entries.push((ReportKind::Failure, format!("{label}: ✗ REJECTED")));
                compact.push(format!("{short} ✗"));
            }
            FlagVerdict::MachineNotFound => {
                entries.push((ReportKind::Failure, format!("Machine '{vm}' not found")));
                compact.push("machine not found".to_string());
            }
            FlagVerdict::Unknown(body) => {
                let body: String = body.chars().take(60).collect();
                entries.push((ReportKind::Info, format!("Unknown response: {body}")));
                compact.push("unknown".to_string());
            }
        }
    }

    let status = if entries.len() == 1 && changed {
        format!("[✓] You hacked {vm}!")
    } else {
        let marker = if changed { "+" } else { "!" };
        format!("[{marker}] {}", compact.join(" · "))
    };

    ActionReport {
        title: format!(" Flag results — {vm} "),
        entries,
        changed,
        status,
    }
}

/// Runs the TUI until the user quits. `refetch` rebuilds `TuiData` on
/// demand; `run_action` executes a user action (flag/upload) and returns an
/// `ActionReport` for the result popup.
pub fn run(
    mut app: AppState,
    refetch: impl Fn() -> Result<TuiData>,
    run_action: impl Fn(TuiAction) -> Result<ActionReport>,
) -> Result<()> {
    let mut terminal = ratatui::init();
    // Kick off the first load (and any pending request) before looping.
    let mut pending_fetch = app.fetching.is_some();
    let result = event_loop(&mut terminal, &mut app, &refetch, &run_action, &mut pending_fetch);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
    refetch: &dyn Fn() -> Result<TuiData>,
    run_action: &dyn Fn(TuiAction) -> Result<ActionReport>,
    pending_fetch: &mut bool,
) -> Result<()> {
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

        // User actions from popups (flag submission, writeup upload).
        if let Some(action) = app.pending_action.take() {
            let label = match action.kind {
                PopupKind::Flag => format!("Submitting flag for {}...", action.vm),
                PopupKind::Upload => format!("Submitting writeup for {}...", action.vm),
            };
            app.fetching = Some(label);
            terminal.draw(|frame| crate::tui::render::draw(frame, app))?;

            match run_action(action) {
                Ok(report) => {
                    // Footer shows a 5s summary; the popup persists.
                    app.set_status(report.status.clone());
                    app.pending_refresh_after_close = report.changed;
                    app.report = Some(report);
                }
                Err(error) => app.set_status(format!("Action failed: {error:#}")),
            }
            app.fetching = None;
        }

        if app.should_fetch(*pending_fetch) {
            *pending_fetch = false;
            app.refresh_requested = false;
            app.fetching = Some("Refreshing data...".to_string());
            // Draw immediately so the `⟳ <label>` shows while the blocking
            // fetch runs, instead of freezing silently.
            terminal.draw(|frame| crate::tui::render::draw(frame, app))?;

            let result = refetch();
            app.fetching = None;
            match result {
                Ok(data) => {
                    app.set_data(data);
                    app.set_status("Data refreshed.");
                }
                Err(error) => app.set_status(format!("Fetch failed: {error:#}")),
            }
        }

        if app.quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut AppState, key: crossterm::event::KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.request_quit();
        return;
    }

    // Result report popup captures everything until dismissed. Closing it
    // with `changed` set queues the deferred refresh (Opsi A).
    if app.report.is_some() {
        match key.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q') => {
                if app.close_report() {
                    app.refresh_requested = true;
                }
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
                app.popup = None;
                app.set_status("Cancelled.");
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
            KeyCode::Down | KeyCode::Tab => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.next_field();
                }
            }
            KeyCode::Char(c) => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.push(c);
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
            KeyCode::Char('f') => app.open_action_popup(PopupKind::Flag),
            KeyCode::Char('u') => app.open_action_popup(PopupKind::Upload),
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
            pending: vec![
                "Fuxa".to_string(),
                "Liar".to_string(),
                "Rooted".to_string(),
            ],
            catalog: vec![
                Machine {
                    name: "Fuxa".into(),
                    creator: "0xM4r10".into(),
                    size: "0.5 Gb".into(),
                    difficulty: "beginner".into(),
                    os: "linux".into(),
                    status: "PWNED".into(),
                },
                Machine {
                    name: "Nebula1".into(),
                    creator: "Sublarge".into(),
                    size: "1.3 Gb".into(),
                    difficulty: "advanced".into(),
                    os: "linux".into(),
                    status: "TO HACK".into(),
                },
                Machine {
                    name: "Arcane".into(),
                    creator: "asya2ross".into(),
                    size: "1.9 Gb".into(),
                    difficulty: "intermediate".into(),
                    os: "linux".into(),
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
        state
            .popup
            .as_mut()
            .unwrap()
            .buffers[0]
            .push_str("flag{user}");
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(state.popup.as_ref().unwrap().field, 1);
        state
            .popup
            .as_mut()
            .unwrap()
            .buffers[1]
            .push_str("flag{root}");
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
            "Arcane",
            vec![
                (0, FlagVerdict::Correct),
                (1, FlagVerdict::Wrong),
            ],
        );
        assert_eq!(report.title, " Flag results — Arcane ");
        assert_eq!(report.entries[0].0, ReportKind::Success);
        assert_eq!(report.entries[0].1, "User flag: ✓ ACCEPTED");
        assert_eq!(report.entries[1].0, ReportKind::Failure);
        assert_eq!(report.entries[1].1, "Root flag: ✗ REJECTED");
        assert!(report.changed);
        assert_eq!(report.status, "[+] User ✓ · Root ✗");

        // Root field only (index 1) keeps its Root label — not shifted.
        let report = super::build_flag_report("Arcane", vec![(1, FlagVerdict::Wrong)]);
        assert_eq!(report.entries[0].1, "Root flag: ✗ REJECTED");
        assert!(!report.changed);

        // Lone accepted flag keeps the celebratory footer.
        let report = super::build_flag_report("Arcane", vec![(0, FlagVerdict::Correct)]);
        assert_eq!(report.status, "[✓] You hacked Arcane!");

        // All rejected -> no refresh.
        let report = super::build_flag_report(
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

        // Dismissal queues the deferred refresh (Opsi A).
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert!(state.report.is_none());
        assert!(state.refresh_requested);

        // Without changes, closing never refreshes.
        state.report = Some(super::build_flag_report(
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
    fn action_keys_are_tab_gated() {
        // 'f' on Pending -> blocked; 'u' on Pending -> allowed.
        let mut state = app();
        state.next_tab();
        state.next_tab(); // Pending
        state.popup = None;
        handle_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::empty()),
        );
        assert!(state.popup.is_none(), "flag popup must be blocked on Pending");
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
        assert!(state.popup.is_none(), "upload popup must be blocked on Machines");
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
            crossterm::event::KeyEvent::new(KeyCode::Char('j'), crossterm::event::KeyModifiers::empty()),
        );
        assert_eq!(state.selected, 1);

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(KeyCode::Char('/'), crossterm::event::KeyModifiers::empty()),
        );
        assert_eq!(state.input_mode, InputMode::Filter);

        super::handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(KeyCode::Char('x'), crossterm::event::KeyModifiers::empty()),
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
            crossterm::event::KeyEvent::new(KeyCode::Char('q'), crossterm::event::KeyModifiers::empty()),
        );
        assert!(state.quit);
    }

    #[test]
    fn selected_url_only_on_writeups_tab() {
        let mut state = app();
        assert!(state.selected_writeup_url().is_none()); // Stats tab
        state.next_tab();
        assert_eq!(state.tab, Tab::Writeups);
        assert_eq!(
            state.selected_writeup_url(),
            Some("https://example.com/economists.md")
        );
        state.filter_push('z');
        state.reset_list_position_for_test();
        assert_eq!(
            state.selected_writeup_url(),
            Some("https://example.com/za1.md")
        );
        state.next_tab(); // Pending
        assert!(state.selected_writeup_url().is_none());
    }
}