//! Interactive dashboard: application state, input handling and the event
//! loop. Rendering lives in `render.rs`; all state transitions here are pure
//! and unit-tested.

pub mod render;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::modules::stats::{ProfileStats, ProfileWriteup};

#[derive(Debug, Clone)]
pub struct TuiData {
    pub stats: ProfileStats,
    /// (label, pwned, total) rows for the progress gauges.
    pub progress: Vec<(String, u64, u64)>,
    /// Machines fully pwned (user+root flags) without an accepted writeup.
    pub pending: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Stats,
    Writeups,
    Pending,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Stats, Tab::Writeups, Tab::Pending];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Stats => "Stats",
            Tab::Writeups => "Writeups",
            Tab::Pending => "Pending",
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
    pub refresh_requested: bool,
    pub status: Option<String>,
    pub data: TuiData,
    /// Row budget reported by the renderer after layout.
    pub last_visible_rows: Option<usize>,
}

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
            status: None,
            data,
            last_visible_rows: None,
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

    fn row_count(&self) -> usize {
        match self.tab {
            Tab::Stats => 0,
            Tab::Writeups => self.visible_writeups().len(),
            Tab::Pending => self.visible_pending().len(),
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
            self.status = Some(match opened {
                Ok(_) => format!("Opened in browser: {url}"),
                Err(error) => format!("xdg-open failed: {error}"),
            });
        }
    }

    pub fn request_quit(&mut self) {
        self.quit = true;
    }

    pub fn request_refresh(&mut self) {
        self.refresh_requested = true;
        self.status = Some("Refreshing data...".to_string());
    }
}

/// Runs the TUI until the user quits. `refetch` rebuilds `TuiData` on `r`.
pub fn run(mut app: AppState, refetch: impl Fn() -> Result<TuiData>) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app, &refetch);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut AppState,
    refetch: &dyn Fn() -> Result<TuiData>,
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

        if app.refresh_requested {
            app.refresh_requested = false;
            match refetch() {
                Ok(data) => {
                    app.set_data(data);
                    app.status = Some("Data refreshed.".to_string());
                }
                Err(error) => app.status = Some(format!("Refresh failed: {error:#}")),
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