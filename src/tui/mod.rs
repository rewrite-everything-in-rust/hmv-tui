//! Interactive dashboard: application state, input handling and the event
//! loop. Rendering lives in `render.rs`; all state transitions here are pure
//! and unit-tested.

pub mod render;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::modules::machines::Machine;
use crate::modules::stats::{ProfileStats, ProfileWriteup};

/// What a popup asks the user for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Flag,
    Upload,
}

/// A text-input popup bound to one machine.
#[derive(Debug, Clone)]
pub struct Popup {
    pub kind: PopupKind,
    pub vm: String,
    pub buffer: String,
}

/// A user action queued from a popup, executed by the host application.
#[derive(Debug, Clone)]
pub struct TuiAction {
    pub kind: PopupKind,
    pub vm: String,
    pub value: String,
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
}

impl TuiData {
    /// Placeholder shown while the first fetch is still running.
    pub fn empty() -> Self {
        Self {
            stats: ProfileStats::default(),
            progress: Vec::new(),
            pending: Vec::new(),
            catalog: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Stats,
    Writeups,
    Pending,
    Machines,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Stats, Tab::Writeups, Tab::Pending, Tab::Machines];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Stats => "Stats",
            Tab::Writeups => "Writeups",
            Tab::Pending => "Pending",
            Tab::Machines => "Machines",
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

    /// Opens the input popup for the given action on the selected machine.
    pub fn open_action_popup(&mut self, kind: PopupKind) {
        if self.popup.is_some() {
            return;
        }
        if let Some(vm) = self.selected_machine_name() {
            self.popup = Some(Popup {
                kind,
                vm,
                buffer: String::new(),
            });
        } else {
            self.set_status("Nothing selected to act on.");
        }
    }

    /// Confirms the popup: queues the action and closes the popup.
    pub fn confirm_popup(&mut self) {
        if let Some(popup) = self.popup.take() {
            let value = popup.buffer.trim().to_string();
            if value.is_empty() {
                self.set_status("Cancelled — empty input.");
                return;
            }
            let kind_label = match popup.kind {
                PopupKind::Flag => "flag",
                PopupKind::Upload => "writeup URL",
            };
            self.set_status(format!("Queued {} for {}...", kind_label, popup.vm));
            self.pending_action = Some(TuiAction {
                kind: popup.kind,
                vm: popup.vm,
                value,
            });
        }
    }

    fn row_count(&self) -> usize {
        match self.tab {
            Tab::Stats => 0,
            Tab::Writeups => self.visible_writeups().len(),
            Tab::Pending => self.visible_pending().len(),
            Tab::Machines => self.visible_machines().len(),
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

/// Runs the TUI until the user quits. `refetch` rebuilds `TuiData` on
/// demand; `run_action` executes a user action (flag/upload) and returns
/// `(footer message, data changed)`.
pub fn run(
    mut app: AppState,
    refetch: impl Fn() -> Result<TuiData>,
    run_action: impl Fn(TuiAction) -> Result<(String, bool)>,
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
    run_action: &dyn Fn(TuiAction) -> Result<(String, bool)>,
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
                Ok((message, changed)) => {
                    app.set_status(message);
                    if changed {
                        // Pwned status / accepted writeups may have changed.
                        app.refresh_requested = true;
                    }
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

    // Popup input mode captures everything first.
    if app.popup.is_some() {
        match key.code {
            KeyCode::Esc => {
                app.popup = None;
                app.set_status("Cancelled.");
            }
            KeyCode::Enter => app.confirm_popup(),
            KeyCode::Backspace => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.buffer.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(popup) = app.popup.as_mut() {
                    popup.buffer.push(c);
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

    fn sample_data() -> TuiData {
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
        assert_eq!(state.visible_machines().len(), 2);

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
        state.next_tab();
        state.next_tab(); // Pending tab — machine-centric
        assert_eq!(state.tab, Tab::Pending);
        assert_eq!(state.selected_machine_name().as_deref(), Some("Fuxa"));

        state.open_action_popup(PopupKind::Flag);
        let popup = state.popup.as_ref().unwrap();
        assert_eq!(popup.kind, PopupKind::Flag);
        assert_eq!(popup.vm, "Fuxa");

        state.popup.as_mut().unwrap().buffer.push_str("flag{abc}");
        state.confirm_popup();
        assert!(state.popup.is_none());
        let action = state.pending_action.take().unwrap();
        assert_eq!(action.vm, "Fuxa");
        assert_eq!(action.value, "flag{abc}");
        assert_eq!(action.kind, PopupKind::Flag);
    }

    #[test]
    fn popup_rejects_empty_input_and_double_open() {
        let mut state = app();
        state.next_tab();
        state.next_tab();
        state.next_tab();
        state.open_action_popup(PopupKind::Upload);
        state.confirm_popup(); // empty buffer -> cancelled, no action
        assert!(state.popup.is_none());
        assert!(state.pending_action.is_none());

        state.open_action_popup(PopupKind::Upload);
        assert!(state.popup.is_some());
        state.open_action_popup(PopupKind::Flag); // ignored while open
        assert_eq!(state.popup.as_ref().unwrap().kind, PopupKind::Upload);
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