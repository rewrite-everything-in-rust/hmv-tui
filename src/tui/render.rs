//! Pure state -> widget rendering for the dashboard.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, LineGauge, List, ListItem, ListState, Paragraph, Row, Table, TableState,
    Tabs,
};
use ratatui::Frame;

use super::{
    ActionReport, AppState, InputMode, Popup, PopupKind, ReportKind, Tab, ViewMode,
    WriteupsPopup, downloads::Phase,
};

// Nord theme palette (https://www.nordtheme.com/docs/colors-and-palettes).
const NORD1: Color = Color::Rgb(0x3B, 0x42, 0x52); // polar night (dim bg)
const NORD6: Color = Color::Rgb(0xEC, 0xEF, 0xF4); // snow storm (bright text)
const NORD7: Color = Color::Rgb(0x8F, 0xBC, 0xBB); // frost (teal)
const NORD8: Color = Color::Rgb(0x88, 0xC0, 0xD0); // frost (accent blue)
const NORD10: Color = Color::Rgb(0x5E, 0x81, 0xAC); // frost (links)
const NORD11: Color = Color::Rgb(0xBF, 0x61, 0x6A); // aurora red
const NORD13: Color = Color::Rgb(0xEB, 0xCB, 0x8B); // aurora yellow
const NORD14: Color = Color::Rgb(0xA3, 0xBE, 0x8C); // aurora green
const NORD15: Color = Color::Rgb(0xB4, 0x8E, 0xAD); // aurora purple

const ACCENT: Color = NORD8; // titles, active tab, gauges, borders
const WARN: Color = NORD13; // notices, pending count, non-final states
const OK: Color = NORD14; // success, PWNED, beginner difficulty
const BAD: Color = NORD11; // failures, advanced difficulty
const FROST: Color = NORD7; // was cyan: windows OS, READ format
const PURPLE: Color = NORD15; // was magenta: WATCH format, UPCOMING
const BRIGHT: Color = NORD6; // was white: names, primary text
const LINK: Color = NORD10; // writeup URLs
const HL_BG: Color = NORD1; // selected-row background

pub fn draw(frame: &mut Frame, app: &mut AppState) {
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    draw_tabs(frame, tabs, app);

    match app.tab {
        Tab::Stats => draw_stats(frame, body, app),
        Tab::Writeups => draw_writeups(frame, body, app),
        Tab::Pending => draw_pending(frame, body, app),
        Tab::Machines => draw_machines(frame, body, app),
        Tab::Releases => draw_releases(frame, body, app),
    }

    draw_footer(frame, footer, app);

    if app.view == ViewMode::Downloads && app.popup.is_none() && app.report.is_none() {
        draw_downloads(frame, frame.area(), app);
    }
    if let Some(popup) = &app.popup {
        draw_popup(frame, frame.area(), popup);
    }
    if let Some(report) = &app.report {
        draw_report(frame, frame.area(), report);
    }
    if app.writeups_popup.is_some() {
        if let Some(popup) = app.writeups_popup.clone() {
            draw_writeups_popup(frame, frame.area(), &popup);
        }
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &AppState) {
    let stats = &app.data.stats;
    let pending_count = app.data.pending.len();
    let line = Line::from(vec![
        Span::styled(" HackMyVM", Style::new().fg(ACCENT).bold()),
        Span::styled(" dashboard", Style::new().dim()),
        Span::raw("  ·  "),
        Span::styled(
            format!("{} ", stats.username),
            Style::new().fg(BRIGHT).bold(),
        ),
        Span::styled(
            stats.rank.clone().unwrap_or_default(),
            Style::new().fg(ACCENT),
        ),
        Span::raw("  ·  "),
        Span::styled(format!("{} pts", stats.points), Style::new().fg(WARN)),
        Span::raw("  ·  "),
        Span::styled(
            format!("{} writeups", stats.accepted_writeups.len()),
            Style::new().fg(FROST),
        ),
        Span::raw("  ·  "),
        Span::styled(
            format!("{pending_count} pending"),
            Style::new().fg(if pending_count > 0 { WARN } else { OK }),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &AppState) {
    let titles: Vec<&str> = Tab::ALL.iter().map(|t| t.title()).collect();
    let index = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .select(index)
        // Dim the inactive tabs so the highlighted one stands out.
        .style(Style::new().dim())
        .highlight_style(Style::new().fg(ACCENT).bold().underlined());
    frame.render_widget(tabs, area);
}

fn draw_stats(frame: &mut Frame, area: Rect, app: &AppState) {
    let stats = &app.data.stats;
    let [left, right] = Layout::horizontal([Constraint::Percentage(45), Constraint::Fill(1)])
        .areas(area);

    let mut lines = vec![
        Line::from(Span::styled("[ Identity ]", Style::new().fg(ACCENT).bold())),
        Line::from(format!("  Rank      : {}", stats.rank.as_deref().unwrap_or("-"))),
        Line::from(format!("  Title     : {}", stats.title.as_deref().unwrap_or("-"))),
        Line::from(format!("  Country   : {}", stats.country.as_deref().unwrap_or("-"))),
        Line::from(format!("  Loved     : {}", stats.loved)),
        Line::from(""),
        Line::from(Span::styled("[ Achievements ]", Style::new().fg(ACCENT).bold())),
        Line::from(format!("  Points      : {}", stats.points)),
        Line::from(format!("  Total Roots : {}", stats.roots)),
        Line::from(format!("  Total Users : {}", stats.users)),
        Line::from(format!("  First Roots : {}", stats.first_roots)),
        Line::from(format!("  First Users : {}", stats.first_users)),
        Line::from(format!("  Challenges  : {}", stats.challenges)),
        Line::from(format!("  Writeups    : {}", stats.writeups)),
        Line::from(""),
        Line::from(Span::styled(
            format!("[ Trophies ] ({})", stats.trophies.len()),
            Style::new().fg(ACCENT).bold(),
        )),
    ];
    for chunk in stats.trophies.chunks(5) {
        lines.push(Line::from(format!("  {}", chunk.join("  "))));
    }

    frame.render_widget(Paragraph::new(lines), left);

    let title = Paragraph::new(Span::styled("[ Progress ]", Style::new().fg(ACCENT).bold()));
    frame.render_widget(title, right);

    // One single-row gauge per metric: "Total VMs 167/371 (45%) ━━━━░░░░".
    // The old two-row Gauge buried its centered label inside the bar, which
    // rendered as unmarked block rows on wide terminals.
    let mut y = right.y + 2;
    let width = right.width.saturating_sub(4).max(16);
    for (label, value, total) in &app.data.progress {
        if y + 1 > right.bottom() {
            break;
        }
        let slot = Rect {
            x: right.x + 2,
            y,
            width,
            height: 1,
        };
        let ratio = if *total == 0 {
            0.0
        } else {
            (*value as f64) / (*total as f64)
        };
        let percent = (ratio * 100.0).round() as u64;
        let gauge = LineGauge::default()
            .label(Span::styled(
                format!("{label} {value}/{total} ({percent}%)"),
                Style::new().fg(BRIGHT),
            ))
            .ratio(ratio.clamp(0.0, 1.0))
            .filled_style(Style::new().fg(ACCENT));
        frame.render_widget(gauge, slot);
        y += 2; // gauge row + one blank row
    }
}

fn draw_writeups(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let visible = app.visible_writeups();
    let header = Row::new(["VM", "Language", "Link"])
        .style(Style::new().fg(ACCENT).bold())
        .bottom_margin(0);

    let rows: Vec<Row> = visible
        .iter()
        .map(|w| {
            Row::new([
                w.vm.clone(),
                if w.language.is_empty() {
                    "-".to_string()
                } else {
                    w.language.clone()
                },
                w.url.clone(),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::new()
            .bg(HL_BG)
            .add_modifier(Modifier::BOLD),
    )
    .block(filter_block(app));

    let mut state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);

    app.set_visible_rows(visible_rows_in(area.height, visible.len()));
}

fn draw_pending(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let visible = app.visible_pending();

    let items: Vec<ListItem> = visible
        .iter()
        .map(|vm| {
            let name = (*vm).clone();
            ListItem::new(Line::from(vec![
                Span::styled(" ● ", Style::new().fg(WARN)),
                Span::styled(name, Style::new().fg(BRIGHT).bold()),
                Span::styled("  — pwned, writeup not submitted", Style::new().dim()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .bg(HL_BG)
                .add_modifier(Modifier::BOLD),
        )
        .block(filter_block(app));

    let mut state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);

    if visible.is_empty() {
        let empty = Paragraph::new(Span::styled(
            "Nothing pending — every pwned machine has an accepted writeup!",
            Style::new().fg(ACCENT),
        ))
        .block(filter_block(app));
        frame.render_widget(empty, area);
    }

    app.set_visible_rows(visible_rows_in(area.height, visible.len()));
}

fn filter_block(app: &AppState) -> Block<'_> {
    let count_line = match app.tab {
        Tab::Writeups => format!(
            " Writeups {}/{} ",
            app.visible_writeups().len(),
            app.data.stats.accepted_writeups.len()
        ),
        Tab::Pending => format!(
            " Pending {}/{} ",
            app.visible_pending().len(),
            app.data.pending.len()
        ),
        Tab::Machines => format!(
            " Machines {}/{}{} ",
            app.visible_machines().len(),
            app.data.catalog.len(),
            app.machine_sort.indicator()
        ),
        Tab::Releases => format!(
            " Releases {}/{} ",
            app.visible_releases().len(),
            app.data.releases.len()
        ),
        Tab::Stats => String::new(),
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().dim());

    if app.input_mode == InputMode::Filter {
        block = block
            .title(Span::styled(
                format!(" filter: {}▏", app.filter),
                Style::new().fg(WARN).bold(),
            ))
            .title_position(ratatui::widgets::block::Position::Top)
            .border_style(Style::new().fg(WARN));
    } else if !app.filter.is_empty() {
        block = block.title(Span::styled(
            format!(" filter: {} ", app.filter),
            Style::new().fg(WARN),
        ));
    }

    if !count_line.is_empty() {
        block = block.title_bottom(Span::styled(count_line, Style::new().dim()));
    }
    block
}

fn draw_machines(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let visible = app.visible_machines();
    let header = Row::new(["VM", "Difficulty", "Creator", "Size", "Status"])
        .style(Style::new().fg(ACCENT).bold());

    let rows: Vec<Row> = visible
        .iter()
        .map(|m| {
            let diff = m.difficulty.to_uppercase();
            let diff_span = match diff.as_str() {
                "BEGINNER" => Span::styled(diff, Style::new().fg(OK)),
                "INTERMEDIATE" => Span::styled(diff, Style::new().fg(WARN)),
                "ADVANCED" => Span::styled(diff, Style::new().fg(BAD)),
                _ => Span::raw(diff),
            };
            let status = m.status.to_uppercase();
            let status_span = if status.contains("DONE") || status.contains("PWNED") {
                Span::styled(status, Style::new().fg(OK).bold())
            } else {
                Span::styled(status, Style::new().fg(WARN))
            };
            Row::new([
                Span::styled(m.name.clone(), Style::new().fg(BRIGHT)),
                diff_span,
                Span::raw(m.creator.clone()),
                Span::raw(m.size.clone()),
                status_span,
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::new()
            .bg(HL_BG)
            .add_modifier(Modifier::BOLD),
    )
    .block(filter_block(app));

    let mut state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);

    app.set_visible_rows(visible_rows_in(area.height, visible.len()));
}

fn draw_popup(frame: &mut Frame, area: Rect, popup: &Popup) {
    // Account overview: shows the active account with switch/logout actions.
    if popup.kind == PopupKind::Account {
        let box_area = popup_area(area, 56, 7);
        frame.render_widget(Clear, box_area);
        let username = if popup.vm.is_empty() { "-" } else { &popup.vm };
        let lines = vec![
            Line::from(Span::styled(
                format!("Logged in as {username}"),
                Style::new().fg(OK).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Enter switch account · l logout · Esc close",
                Style::new().dim(),
            )),
        ];
        let block = Block::bordered()
            .title(Span::styled(
                format!(" Account — {username} "),
                Style::new().fg(OK).bold(),
            ))
            .border_style(Style::new().fg(OK));
        frame.render_widget(Paragraph::new(lines).block(block), box_area);
        return;
    }

    // Read-only info box for already-PWNED machines: no fields, no submit.
    if popup.readonly {
        let box_area = popup_area(area, 56, 7);
        frame.render_widget(Clear, box_area);
        let lines = vec![
            Line::from(Span::styled(
                "User & root flags are in.",
                Style::new().fg(OK),
            )),
            Line::from(Span::styled(
                "Resubmission is disabled.",
                Style::new().dim(),
            )),
            Line::from(""),
            Line::from(Span::styled("Enter / Esc close", Style::new().dim())),
        ];
        let block = Block::bordered()
            .title(Span::styled(
                format!(" ✓ Already PWNED — {} ", popup.vm),
                Style::new().fg(OK).bold(),
            ))
            .border_style(Style::new().fg(OK));
        frame.render_widget(Paragraph::new(lines).block(block), box_area);
        return;
    }

    let fields = popup.buffers.len();
    let height = if fields > 1 { 10 } else { 7 };
    let height = if popup.notice.is_some() { height + 1 } else { height };
    let box_area = popup_area(area, 74, height);
    frame.render_widget(Clear, box_area);

    let (title, prompts, hint): (String, Vec<&str>, &str) = match popup.kind {
        PopupKind::Flag => (
            format!(" Submit flags — {} ", popup.vm),
            vec!["User flag:", "Root flag:"],
            "Enter send both · ↑↓/Tab switch field · Esc cancel",
        ),
        PopupKind::Upload => (
            format!(" Submit writeup — {} ", popup.vm),
            vec!["Writeup URL:"],
            "Enter send · Esc cancel",
        ),
        PopupKind::Download => (
            format!(" Download — {} ", popup.vm),
            vec!["Save to:"],
            "Enter start · Esc cancel",
        ),
        PopupKind::Config => (
            " Configure HackMyVM ".to_string(),
            vec!["Username:", "Password:"],
            "Enter save & connect · ↑↓/Tab switch field · Esc quit",
        ),
        PopupKind::Account => unreachable!("rendered by the account branch above"),
    };

    let mut lines = Vec::new();
    if let Some(notice) = &popup.notice {
        lines.push(Line::from(Span::styled(
            format!("⚠ {notice}"),
            Style::new().fg(WARN).bold(),
        )));
        lines.push(Line::from(""));
    }
    for (index, prompt) in prompts.iter().enumerate() {
        let active = index == popup.field;
        let marker = if active { "▏" } else { "" };
        let raw = popup.buffers.get(index).map(String::as_str).unwrap_or("");
        let buffer = if popup.kind == PopupKind::Config && index == 1 {
            // Mask the password field.
            "•".repeat(raw.chars().count())
        } else {
            raw.to_string()
        };
        let style = if active {
            Style::new().fg(BRIGHT).add_modifier(Modifier::BOLD)
        } else {
            Style::new().dim()
        };
        lines.push(Line::from(Span::styled(
            format!("{prompt} {buffer}{marker}"),
            style,
        )));
        if index + 1 < prompts.len() {
            lines.push(Line::from(""));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(hint, Style::new().dim())));

    let block = Block::bordered()
        .title(Span::styled(title, Style::new().fg(WARN).bold()))
        .border_style(Style::new().fg(WARN));

    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_releases(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let visible = app.visible_releases();
    let header = Row::new(["Date", "OS", "VM", "Status"])
        .style(Style::new().fg(ACCENT).bold());

    let rows: Vec<Row> = visible
        .iter()
        .map(|r| {
            let os_span = if r.os == "windows" {
                Span::styled(r.os.clone(), Style::new().fg(FROST))
            } else {
                Span::styled(r.os.clone(), Style::new().fg(WARN))
            };
            let status_span = if r.released {
                Span::styled("RELEASED", Style::new().fg(OK).bold())
            } else {
                Span::styled("UPCOMING", Style::new().fg(PURPLE).bold())
            };
            Row::new([
                Span::styled(r.date.clone(), Style::new().dim()),
                os_span,
                Span::styled(r.name.clone(), Style::new().fg(BRIGHT).bold()),
                status_span,
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(24),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .row_highlight_style(
        Style::new()
            .bg(HL_BG)
            .add_modifier(Modifier::BOLD),
    )
    .block(filter_block(app));

    let mut state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);

    app.set_visible_rows(visible_rows_in(area.height, visible.len()));
}

fn draw_report(frame: &mut Frame, area: Rect, report: &ActionReport) {
    let height = (report.entries.len() as u16 + 4).clamp(5, 12);
    let width = 60;
    let box_area = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, box_area);

    let mut lines = vec![Line::from("")];
    for (kind, text) in &report.entries {
        let span = match kind {
            ReportKind::Success => Span::styled(text.clone(), Style::new().fg(OK).bold()),
            ReportKind::Failure => Span::styled(text.clone(), Style::new().fg(BAD).bold()),
            ReportKind::Info => Span::styled(text.clone(), Style::new().fg(WARN)),
        };
        lines.push(Line::from(format!("  {span}")));
        lines.push(Line::from(""));
    }
    let footer_hint = if report.changed {
        "Data will refresh on close · Enter / Esc close"
    } else {
        "Enter / Esc close"
    };
    lines.push(Line::from(Span::styled(footer_hint, Style::new().dim())));

    let block = Block::bordered()
        .title(Span::styled(
            report.title.clone(),
            Style::new().fg(ACCENT).bold(),
        ))
        .border_style(Style::new().fg(ACCENT));

    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_writeups_popup(frame: &mut Frame, area: Rect, popup: &WriteupsPopup) {
    let rows: Vec<Row> = popup
        .entries
        .iter()
        .map(|w| {
            let lang_style = if w.language.contains("English") {
                Style::new().fg(OK)
            } else {
                Style::new().fg(WARN)
            };
            let format_style = if w.format.contains("Read") {
                Style::new().fg(FROST)
            } else {
                Style::new().fg(PURPLE)
            };
            Row::new(vec![
                Span::styled(w.date.clone(), Style::new().dim()),
                Span::styled(w.author.clone(), Style::new().fg(BRIGHT).bold()),
                Span::styled(w.language.clone(), lang_style),
                Span::styled(w.format.to_uppercase(), format_style),
                Span::styled(w.url.clone(), Style::new().fg(LINK).dim()),
            ])
        })
        .collect();

    let height = (popup.entries.len() as u16 + 3).clamp(6, 18);
    // Nearly full terminal width so long writeup links stay readable.
    let width = area.width.saturating_sub(4).max(60);
    let box_area = popup_area(area, width, height);
    frame.render_widget(Clear, box_area);

    let header = Row::new(["Date", "Author (Poet)", "Language", "Format", "Link"])
        .style(Style::new().fg(ACCENT).bold());
    let hint = Row::new([" ", " ", " ", " ", "Enter open link · jk select · Esc close"])
        .style(Style::new().dim());

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .footer(hint)
    .row_highlight_style(
        Style::new()
            .bg(HL_BG)
            .add_modifier(Modifier::BOLD),
    )
    .block(
        Block::bordered()
            .title(Span::styled(
                format!(" Writeups — {} ", popup.vm),
                Style::new().fg(ACCENT).bold(),
            ))
            .border_style(Style::new().fg(ACCENT)),
    );

    let mut state = TableState::default().with_selected(Some(popup.selected));
    frame.render_stateful_widget(table, box_area, &mut state);
}

fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

fn draw_downloads(frame: &mut Frame, area: Rect, app: &AppState) {
    let jobs = app.download_jobs.len();
    let height = (jobs as u16 + 5).clamp(6, 16);
    let box_area = popup_area(area, 96, height);
    frame.render_widget(Clear, box_area);

    let mut lines: Vec<Line> = Vec::new();
    if jobs == 0 {
        lines.push(Line::from(Span::styled(
            "No downloads yet — press d on a machine.",
            Style::new().dim(),
        )));
    }

    let selected_index = if app.view == ViewMode::Downloads {
        app.download_selected()
    } else {
        usize::MAX
    };

    for (index, job) in app.download_jobs.iter().enumerate() {
        // Lock once and read everything through the guard: `is_active()`
        // and `download_selected()` would re-lock the same non-reentrant
        // std::Mutex while the guard is alive — self-deadlock.
        let state = job.state.lock().unwrap();
        let active = matches!(state.phase, Phase::Resolving | Phase::Downloading);
        let selected = index == selected_index;
        let marker = if selected && active {
            Span::styled("c ", Style::new().fg(WARN).bold())
        } else {
            Span::raw("  ")
        };

        let line = match state.phase {
            Phase::Resolving => Line::from(vec![
                marker,
                Span::styled(
                    format!("… {}  resolving MEGA link…", job.vm),
                    Style::new().dim(),
                ),
            ]),
            Phase::Downloading => {
                let ratio = if state.total > 0 {
                    state.downloaded as f64 / state.total as f64
                } else {
                    0.0
                };
                let filled = (ratio * 24.0).round() as usize;
                let bar = format!(
                    "[{}{}]",
                    "█".repeat(filled),
                    "░".repeat(24usize.saturating_sub(filled))
                );
                Line::from(vec![
                    marker,
                    Span::styled(format!("↓ {:<12}", job.vm), Style::new().fg(ACCENT).bold()),
                    Span::styled(format!("{bar} "), Style::new().fg(ACCENT)),
                    Span::styled(
                        format!(
                            "{}/{} · {}/s",
                            super::downloads::fmt_bytes(state.downloaded),
                            super::downloads::fmt_bytes(state.total),
                            super::downloads::fmt_bytes(state.speed_bps),
                        ),
                        Style::new().fg(BRIGHT),
                    ),
                ])
            }
            Phase::Done => Line::from(vec![
                Span::raw("  "),
                Span::styled("✓ ", Style::new().fg(OK).bold()),
                Span::styled(
                    format!("{} → {}", job.vm, state.message),
                    Style::new().fg(OK),
                ),
            ]),
            Phase::Failed => Line::from(vec![
                Span::raw("  "),
                Span::styled("✗ ", Style::new().fg(BAD).bold()),
                Span::styled(
                    format!("{}: {}", job.vm, state.message),
                    Style::new().fg(BAD),
                ),
            ]),
            Phase::Cancelled => Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("• {} cancelled", job.vm), Style::new().dim()),
            ]),
        };
        lines.push(line);
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "o close (downloads keep running) · c cancel latest · q quit warns while active",
        Style::new().dim(),
    )));

    let block = Block::bordered()
        .title(Span::styled(
            " Downloads ",
            Style::new().fg(WARN).bold(),
        ))
        .border_style(Style::new().fg(WARN));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &AppState) {
    let [keys_area, status_area] = Layout::horizontal([
        Constraint::Min(80),
        Constraint::Fill(1),
    ])
    .areas(area);

    let keys: String = if app.popup.is_some() {
        match app.popup.as_ref().map(|p| p.kind) {
            Some(PopupKind::Config) => "Enter save & connect · Esc quit".to_string(),
            Some(PopupKind::Account) => "Enter switch account · l logout · Esc close".to_string(),
            _ => "Enter send · ↑↓/Tab switch field · Esc cancel".to_string(),
        }
    } else {
        match app.input_mode {
            InputMode::Filter => "Enter confirm · Esc clear & exit filter".to_string(),
            InputMode::Normal => {
                let list_keys = match app.tab {
                    Tab::Stats => String::new(),
                    Tab::Writeups => "jk move · / filter · Enter open · ".to_string(),
                    Tab::Pending => "jk move · / filter · u writeup · ".to_string(),
                    Tab::Machines => "jk move · / filter · s size sort · f flag · d download · ".to_string(),
                    Tab::Releases => "jk move · / filter · ".to_string(),
                };
                let common = "a account · r refresh · q quit";
                if list_keys.is_empty() {
                    format!("Tab switch · {common}")
                } else {
                    format!("Tab · {list_keys}{common}")
                }
            }
        }
    };
    frame.render_widget(Paragraph::new(Span::styled(keys, Style::new().dim())), keys_area);

    let status = if let Some(label) = &app.fetching {
        Span::styled(format!("⟳ {label}"), Style::new().fg(WARN).bold())
    } else {
        match (&app.status, app.status_expiry) {
            (Some(message), Some(expiry)) if std::time::Instant::now() < expiry => {
                Span::styled(message.clone(), Style::new().fg(WARN))
            }
            _ => Span::raw(""),
        }
    };
    frame.render_widget(
        Paragraph::new(status).alignment(ratatui::layout::Alignment::Right),
        status_area,
    );
}

/// How many table rows fit in `height` (border 2 + header 1).
fn visible_rows_in(height: u16, _len: usize) -> usize {
    height.saturating_sub(3) as usize
}