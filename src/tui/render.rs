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
    downloads::Phase, ActionReport, AppState, InputMode, Popup, PopupKind, ReportKind, Tab,
    ViewMode, WriteupsPopup,
};
use crate::i18n::Key;

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
        Tab::Submissions => draw_submissions(frame, body, app),
    }

    draw_footer(frame, footer, app);

    if app.view == ViewMode::Downloads && app.popup.is_none() && app.report.is_none() {
        draw_downloads(frame, frame.area(), app);
    }
    if let Some(popup) = &app.popup {
        draw_popup(frame, frame.area(), popup, app);
    }
    if let Some(report) = &app.report {
        let lang = app.lang;
        draw_report(frame, frame.area(), report, &lang);
    }
    if app.writeups_popup.is_some() {
        let popup = app.writeups_popup.clone().unwrap();
        let lang = app.lang;
        draw_writeups_popup(frame, frame.area(), &popup, &lang);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &AppState) {
    let stats = &app.data.stats;
    let pending_count = app.data.pending.len();
    let line = Line::from(vec![
        Span::styled(" HackMyVM", Style::new().fg(ACCENT).bold()),
        Span::styled(
            format!(" {}", app.lang.t(Key::HeaderDashboard).trim()),
            Style::new().dim(),
        ),
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
    let titles: Vec<&str> = Tab::ALL
        .iter()
        .map(|t| match t {
            Tab::Stats => app.lang.t(Key::TabStats),
            Tab::Writeups => app.lang.t(Key::TabWriteups),
            Tab::Pending => app.lang.t(Key::TabPending),
            Tab::Machines => app.lang.t(Key::TabMachines),
            Tab::Releases => app.lang.t(Key::TabReleases),
            Tab::Submissions => app.lang.t(Key::TabSubmissions),
        })
        .collect();
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
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Fill(1)]).areas(area);

    let mut lines = vec![
        Line::from(Span::styled(
            app.lang.t(Key::StatsIdentity),
            Style::new().fg(ACCENT).bold(),
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsRank),
            stats.rank.as_deref().unwrap_or("-")
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsTitle),
            stats.title.as_deref().unwrap_or("-")
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsCountry),
            stats.country.as_deref().unwrap_or("-")
        )),
        Line::from(format!("{}{}", app.lang.t(Key::StatsLoved), stats.loved)),
        Line::from(""),
        Line::from(Span::styled(
            app.lang.t(Key::StatsAchievements),
            Style::new().fg(ACCENT).bold(),
        )),
        Line::from(format!("{}{}", app.lang.t(Key::StatsPoints), stats.points)),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsTotalRoots),
            stats.roots
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsTotalUsers),
            stats.users
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsFirstRoots),
            stats.first_roots
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsFirstUsers),
            stats.first_users
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsChallenges),
            stats.challenges
        )),
        Line::from(format!(
            "{}{}",
            app.lang.t(Key::StatsWriteups),
            stats.writeups
        )),
        Line::from(""),
        Line::from(Span::styled(
            super::fmt_key(
                app.lang.t(Key::StatsTrophies),
                &[&stats.trophies.len().to_string()],
            ),
            Style::new().fg(ACCENT).bold(),
        )),
    ];
    for chunk in stats.trophies.chunks(5) {
        lines.push(Line::from(format!("  {}", chunk.join("  "))));
    }

    frame.render_widget(Paragraph::new(lines), left);

    let title = Paragraph::new(Span::styled(
        app.lang.t(Key::StatsProgress),
        Style::new().fg(ACCENT).bold(),
    ));
    frame.render_widget(title, right);

    // One single-row gauge per metric: "Total VMs 167/371 (45%) ━━━━░░░░".
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
    .row_highlight_style(Style::new().bg(HL_BG).add_modifier(Modifier::BOLD))
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
        .highlight_style(Style::new().bg(HL_BG).add_modifier(Modifier::BOLD))
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
            " {} {}/{} ",
            app.lang.t(Key::TabWriteups),
            app.visible_writeups().len(),
            app.data.stats.accepted_writeups.len()
        ),
        Tab::Pending => format!(
            " {} {}/{} ",
            app.lang.t(Key::TabPending),
            app.visible_pending().len(),
            app.data.pending.len()
        ),
        Tab::Machines => format!(
            " {} {}/{}{}{} ",
            app.lang.t(Key::TabMachines),
            app.visible_machines().len(),
            app.data.catalog.len(),
            app.machine_sort.indicator(),
            if app.hide_pwned {
                app.lang.t(Key::PwnedHiddenIndicator)
            } else {
                ""
            }
        ),
        Tab::Releases => format!(
            " {} {}/{} ",
            app.lang.t(Key::TabReleases),
            app.visible_releases().len(),
            app.data.releases.len()
        ),
        Tab::Submissions => format!(
            " {} {}/{} ",
            app.lang.t(Key::TabSubmissions),
            app.visible_submissions().len(),
            app.data.submissions.len()
        ),
        Tab::Stats => String::new(),
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().dim());

    if app.input_mode == InputMode::Filter {
        block = block
            .title(Span::styled(
                format!("{}{}▏", app.lang.t(Key::FilterLabel), app.filter),
                Style::new().fg(WARN).bold(),
            ))
            .title_position(ratatui::widgets::block::Position::Top)
            .border_style(Style::new().fg(WARN));
    } else if !app.filter.is_empty() {
        block = block.title(Span::styled(
            format!("{}{} ", app.lang.t(Key::FilterLabel), app.filter),
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
    let header = Row::new([
        app.lang.t(Key::ColVm),
        app.lang.t(Key::ColDifficulty),
        app.lang.t(Key::ColCreator),
        app.lang.t(Key::ColSize),
        app.lang.t(Key::ColOs),
        app.lang.t(Key::ColCompat),
        app.lang.t(Key::ColStatus),
    ])
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
            let os_span = if m.os == "windows" {
                Span::styled(m.os.clone(), Style::new().fg(FROST))
            } else {
                Span::styled(m.os.clone(), Style::new().fg(WARN))
            };
            let compat = if m.tested.is_empty() {
                app.lang.t(Key::NoCompat).to_string()
            } else {
                m.tested.clone()
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
                os_span,
                Span::styled(compat, Style::new().dim()),
                status_span,
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(13),
            Constraint::Length(13),
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Length(18),
            Constraint::Fill(1),
        ],
    )
    .header(header)
    .row_highlight_style(Style::new().bg(HL_BG).add_modifier(Modifier::BOLD))
    .block(filter_block(app));

    let mut state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);

    app.set_visible_rows(visible_rows_in(area.height, visible.len()));
}

fn draw_popup(frame: &mut Frame, area: Rect, popup: &Popup, app: &AppState) {
    let lang = &app.lang;
    // Account overview: shows the active account with switch/logout actions.
    if popup.kind == PopupKind::Account {
        let box_area = popup_area(area, 56, 7);
        frame.render_widget(Clear, box_area);
        let username = if popup.vm.is_empty() { "-" } else { &popup.vm };
        let lines = vec![
            Line::from(Span::styled(
                super::fmt_key(lang.t(Key::PopupLoggedInAs), &[username]),
                Style::new().fg(OK).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                lang.t(Key::PopupAccountHint),
                Style::new().dim(),
            )),
        ];
        let block = Block::bordered()
            .title(Span::styled(
                super::fmt_key(lang.t(Key::PopupAccountTitle), &[username]),
                Style::new().fg(OK).bold(),
            ))
            .border_style(Style::new().fg(OK));
        frame.render_widget(Paragraph::new(lines).block(block), box_area);
        return;
    }

    // Submission rules viewer: readonly text popup (`i` on Submissions).
    if popup.kind == PopupKind::Rules {
        let width = area.width.saturating_sub(8).max(100);
        let height = area.height.saturating_sub(4).max(20);
        let box_area = popup_area(area, width, height);
        frame.render_widget(Clear, box_area);
        let rules_lines: Vec<Line> = popup
            .buffers
            .first()
            .map(|text| {
                text.lines()
                    .map(|l| {
                        Line::from(Span::styled(
                            l.to_string(),
                            if l.starts_with(char::is_numeric) {
                                Style::new().fg(ACCENT).bold()
                            } else {
                                Style::new().fg(BRIGHT)
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let block = Block::bordered()
            .title(Span::styled(
                lang.t(Key::RulesTitle),
                Style::new().fg(ACCENT).bold(),
            ))
            .title_bottom(Span::styled(
                lang.t(Key::PopupPwnedClose),
                Style::new().dim(),
            ))
            .border_style(Style::new().fg(ACCENT));
        frame.render_widget(Paragraph::new(rules_lines).block(block), box_area);
        return;
    }

    // Read-only info box for already-PWNED machines: no fields, no submit.
    if popup.readonly {
        let box_area = popup_area(area, 56, 7);
        frame.render_widget(Clear, box_area);
        let lines = vec![
            Line::from(Span::styled(
                lang.t(Key::PopupPwnedLine1),
                Style::new().fg(OK),
            )),
            Line::from(Span::styled(
                lang.t(Key::PopupPwnedLine2),
                Style::new().dim(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                lang.t(Key::PopupPwnedClose),
                Style::new().dim(),
            )),
        ];
        let block = Block::bordered()
            .title(Span::styled(
                super::fmt_key(lang.t(Key::PopupPwnedTitle), &[&popup.vm]),
                Style::new().fg(OK).bold(),
            ))
            .border_style(Style::new().fg(OK));
        frame.render_widget(Paragraph::new(lines).block(block), box_area);
        return;
    }

    let fields = popup.buffers.len();
    let height = if fields > 1 { 10 } else { 7 };
    let height = if popup.notice.is_some() {
        height + 1
    } else {
        height
    };
    // Room for the path-completion listing (max 6 candidates + header +
    // overflow notice).
    let completion_lines = if popup.kind == PopupKind::Download && !popup.completions.is_empty() {
        1 + popup.completions.len().min(6) + usize::from(popup.completions.len() > 6)
    } else {
        0
    };
    let height = height + completion_lines as u16;
    let box_area = popup_area(area, 74, height);
    frame.render_widget(Clear, box_area);

    // Submit-your-VM form: dynamic fields from the cached scraped form.
    if popup.kind == PopupKind::SubmissionForm {
        let form_fields = &app.data.submission_form;
        let mut lines = Vec::new();
        if let Some(notice) = &popup.notice {
            lines.push(Line::from(Span::styled(
                format!("⚠ {notice}"),
                Style::new().fg(WARN).bold(),
            )));
            lines.push(Line::from(""));
        }
        let fallback_names: Vec<String> = crate::modules::submissions::fallback_form_fields()
            .iter()
            .map(|f| f.name.clone())
            .collect();
        for (index, field) in form_fields.iter().enumerate() {
            let active = index == popup.field;
            let marker = if active { "▏" } else { "" };
            let required_mark = if field.required { "*" } else { "" };
            let raw = popup.buffers.get(index).map(String::as_str).unwrap_or("");
            let style = if active {
                Style::new().fg(BRIGHT).add_modifier(Modifier::BOLD)
            } else {
                Style::new().dim()
            };
            // Prefer the translated label for known fallback field names;
            // scraped fields keep their site label.
            let label = match fallback_names.get(index).map(String::as_str) {
                Some("vmname") => lang.t(Key::SubmitName),
                Some("url") => lang.t(Key::SubmitUrl),
                Some("flaguser") => lang.t(Key::SubmitUserFlag),
                Some("flagroot") => lang.t(Key::SubmitRootFlag),
                Some("writeup") => lang.t(Key::SubmitWriteup),
                Some("tags") => lang.t(Key::SubmitTags),
                Some("notes") => lang.t(Key::SubmitNotes),
                Some("level") => lang.t(Key::SubmitLevel),
                _ => field.label.as_str(),
            };
            lines.push(Line::from(Span::styled(
                format!("{label}{required_mark} {raw}{marker}"),
                style,
            )));
        }
        let height = (form_fields.len() as u16 + 5).clamp(8, 22);
        let box_area = popup_area(area, area.width.saturating_sub(8).max(100), height);
        frame.render_widget(Clear, box_area);
        let block = Block::bordered()
            .title(Span::styled(
                lang.t(Key::SubmitFormTitle),
                Style::new().fg(WARN).bold(),
            ))
            .title_bottom(Span::styled(
                lang.t(Key::SubmitFormHint),
                Style::new().dim(),
            ))
            .border_style(Style::new().fg(WARN));
        frame.render_widget(Paragraph::new(lines).block(block), box_area);
        return;
    }

    let (title, prompts, hint): (String, Vec<&str>, &str) = match popup.kind {
        PopupKind::Flag => (
            super::fmt_key(lang.t(Key::PopupSubmitFlags), &[&popup.vm]),
            vec![lang.t(Key::PopupUserFlag), lang.t(Key::PopupRootFlag)],
            lang.t(Key::PopupHintFlag),
        ),
        PopupKind::Upload => (
            super::fmt_key(lang.t(Key::PopupSubmitWriteup), &[&popup.vm]),
            vec![lang.t(Key::PopupWriteupUrl)],
            lang.t(Key::PopupHintSend),
        ),
        PopupKind::Download => (
            super::fmt_key(lang.t(Key::PopupDownload), &[&popup.vm]),
            vec![lang.t(Key::PopupSaveTo)],
            lang.t(Key::PopupHintDownload),
        ),
        PopupKind::Config => (
            lang.t(Key::PopupConfigure).to_string(),
            vec![lang.t(Key::PopupUsername), lang.t(Key::PopupPassword)],
            lang.t(Key::PopupHintConfig),
        ),
        PopupKind::SubmissionForm => {
            unreachable!("rendered by the submission form branch above")
        }
        PopupKind::Account | PopupKind::Rules => {
            unreachable!("rendered by the branches above")
        }
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
    // Path-completion listing (Tab in the Download popup), zsh style.
    if popup.kind == PopupKind::Download && !popup.completions.is_empty() {
        lines.push(Line::from(Span::styled(
            lang.t(Key::PopupDirectories),
            Style::new().dim(),
        )));
        for name in popup.completions.iter().take(6) {
            lines.push(Line::from(Span::styled(
                format!("  {name}"),
                Style::new().fg(Color::Cyan),
            )));
        }
        let rest = popup.completions.len().saturating_sub(6);
        if rest > 0 {
            lines.push(Line::from(Span::styled(
                super::fmt_key(lang.t(Key::DirectoriesMore), &[&rest.to_string()]),
                Style::new().dim(),
            )));
        }
    }
    lines.push(Line::from(Span::styled(hint, Style::new().dim())));

    let block = Block::bordered()
        .title(Span::styled(title, Style::new().fg(WARN).bold()))
        .border_style(Style::new().fg(WARN));

    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_releases(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let visible = app.visible_releases();
    let header = Row::new([
        app.lang.t(Key::ColDate),
        app.lang.t(Key::ColOs),
        app.lang.t(Key::ColVm),
        app.lang.t(Key::ColStatus),
    ])
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
    .row_highlight_style(Style::new().bg(HL_BG).add_modifier(Modifier::BOLD))
    .block(filter_block(app));

    let mut state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(table, area, &mut state);

    app.set_visible_rows(visible_rows_in(area.height, visible.len()));
}

fn draw_submissions(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let [mine_area, queue_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    let username = app.data.stats.username.to_lowercase();

    // Top: this account's submissions (queue filtered by the account name).
    let mine: Vec<&crate::modules::submissions::QueueEntry> = app
        .visible_submissions()
        .into_iter()
        .filter(|e| e.user.eq_ignore_ascii_case(&username))
        .collect();
    let mine_header = Row::new([
        app.lang.t(Key::ColVm),
        app.lang.t(Key::ColLevel),
        app.lang.t(Key::ColStatus),
        app.lang.t(Key::ColDate),
    ])
    .style(Style::new().fg(ACCENT).bold());
    let mine_rows: Vec<Row> = mine
        .iter()
        .map(|e| {
            let status = e.status.to_uppercase();
            let status_span = if status.contains("ACCEPT") {
                Span::styled(status, Style::new().fg(OK).bold())
            } else if status.contains("REJECT") {
                Span::styled(status, Style::new().fg(BAD))
            } else {
                Span::styled(status, Style::new().fg(WARN))
            };
            Row::new([
                Span::styled(e.name.clone(), Style::new().fg(BRIGHT)),
                Span::raw(e.level.clone()),
                status_span,
                Span::styled(e.date.clone(), Style::new().dim()),
            ])
        })
        .collect();
    let mine_count = format!(
        " {} {}/{} ",
        app.lang.t(Key::TabSubmissions),
        mine.len(),
        app.data.submissions.len()
    );
    let mine_table = Table::new(
        mine_rows,
        [
            Constraint::Length(20),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Fill(1),
        ],
    )
    .header(mine_header)
    .row_highlight_style(Style::new().bg(HL_BG).add_modifier(Modifier::BOLD))
    .block(
        Block::bordered()
            .title(Span::styled(
                app.lang.t(Key::MySubmissionsTitle),
                Style::new().fg(ACCENT).bold(),
            ))
            .title_bottom(Span::styled(mine_count, Style::new().dim()))
            .border_style(Style::new().dim()),
    );
    let mut mine_state = TableState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(mine_table, mine_area, &mut mine_state);

    // Bottom: the whole queue with the submitting user visible.
    let queue_header = Row::new([
        app.lang.t(Key::ColUser),
        app.lang.t(Key::ColVm),
        app.lang.t(Key::ColStatus),
        app.lang.t(Key::ColLevel),
        app.lang.t(Key::ColDate),
    ])
    .style(Style::new().fg(ACCENT).bold());
    let queue_rows: Vec<Row> = app
        .data
        .submissions
        .iter()
        .map(|e| {
            let status = e.status.to_uppercase();
            let status_span = if status.contains("ACCEPT") {
                Span::styled(status, Style::new().fg(OK))
            } else {
                Span::styled(status, Style::new().fg(WARN))
            };
            Row::new([
                Span::styled(e.user.clone(), Style::new().fg(BRIGHT)),
                Span::raw(e.name.clone()),
                status_span,
                Span::raw(e.level.clone()),
                Span::styled(e.date.clone(), Style::new().dim()),
            ])
        })
        .collect();
    let queue_table = Table::new(
        queue_rows,
        [
            Constraint::Length(16),
            Constraint::Length(20),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Fill(1),
        ],
    )
    .header(queue_header)
    .block(
        Block::bordered()
            .title(Span::styled(
                app.lang.t(Key::QueueTitle),
                Style::new().fg(ACCENT).bold(),
            ))
            .border_style(Style::new().dim()),
    );
    frame.render_widget(queue_table, queue_area);

    app.set_visible_rows(visible_rows_in(mine_area.height, mine.len()));
}

fn draw_report(frame: &mut Frame, area: Rect, report: &ActionReport, lang: &crate::i18n::Lang) {
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
        lang.t(Key::ReportHintRefresh)
    } else {
        lang.t(Key::ReportHintClose)
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

fn draw_writeups_popup(
    frame: &mut Frame,
    area: Rect,
    popup: &WriteupsPopup,
    lang: &crate::i18n::Lang,
) {
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

    let header = Row::new([
        lang.t(Key::ColDate),
        lang.t(Key::ColAuthor),
        lang.t(Key::ColLanguage),
        lang.t(Key::ColFormat),
        lang.t(Key::ColLink),
    ])
    .style(Style::new().fg(ACCENT).bold());
    let hint =
        Row::new([" ", " ", " ", " ", lang.t(Key::WriteupsPopupHint)]).style(Style::new().dim());

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
    .row_highlight_style(Style::new().bg(HL_BG).add_modifier(Modifier::BOLD))
    .block(
        Block::bordered()
            .title(Span::styled(
                super::fmt_key(lang.t(Key::WriteupsPopupTitle), &[&popup.vm]),
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
            app.lang.t(Key::DownloadsEmpty),
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
                    super::fmt_key(app.lang.t(Key::DownloadsResolving), &[&job.vm]).to_string(),
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
        app.lang.t(Key::DownloadsHint),
        Style::new().dim(),
    )));

    let block = Block::bordered()
        .title(Span::styled(
            app.lang.t(Key::DownloadsTitle),
            Style::new().fg(WARN).bold(),
        ))
        .border_style(Style::new().fg(WARN));
    frame.render_widget(Paragraph::new(lines).block(block), box_area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &AppState) {
    let [keys_area, status_area] =
        Layout::horizontal([Constraint::Min(80), Constraint::Fill(1)]).areas(area);

    let keys: String = if app.popup.is_some() {
        match app.popup.as_ref().map(|p| p.kind) {
            Some(PopupKind::Config) => app.lang.t(Key::PopupHintConfig).to_string(),
            Some(PopupKind::Account) => app.lang.t(Key::PopupAccountHint).to_string(),
            _ => app.lang.t(Key::PopupHintFlag).to_string(),
        }
    } else {
        match app.input_mode {
            InputMode::Filter => app.lang.t(Key::FooterFilterMode).to_string(),
            InputMode::Normal => {
                let list_keys = match app.tab {
                    Tab::Stats => String::new(),
                    Tab::Writeups => format!(
                        "{} · {} · {} · ",
                        app.lang.t(Key::FooterJkMove),
                        app.lang.t(Key::FooterFilter),
                        app.lang.t(Key::FooterEnterOpen)
                    ),
                    Tab::Pending => format!(
                        "{} · {} · {} · ",
                        app.lang.t(Key::FooterJkMove),
                        app.lang.t(Key::FooterFilter),
                        app.lang.t(Key::FooterWriteup)
                    ),
                    Tab::Machines => format!(
                        "{} · {} · {} · {} · {} · {} · ",
                        app.lang.t(Key::FooterJkMove),
                        app.lang.t(Key::FooterFilter),
                        app.lang.t(Key::FooterSizeSort),
                        app.lang.t(Key::FooterHidePwned),
                        app.lang.t(Key::FooterFlag),
                        app.lang.t(Key::FooterDownload)
                    ),
                    Tab::Releases => format!(
                        "{} · {} · ",
                        app.lang.t(Key::FooterJkMove),
                        app.lang.t(Key::FooterFilter)
                    ),
                    Tab::Submissions => format!(
                        "{} · {} · {} · {} · ",
                        app.lang.t(Key::FooterJkMove),
                        app.lang.t(Key::FooterFilter),
                        app.lang.t(Key::FooterSubmitVm),
                        app.lang.t(Key::FooterRules)
                    ),
                };
                let common = format!(
                    "{} · {} · {}: {} · {}",
                    app.lang.t(Key::FooterAccount),
                    app.lang.t(Key::FooterRefresh),
                    app.lang.t(Key::FooterLangHint),
                    app.lang.other().label(),
                    app.lang.t(Key::FooterQuit)
                );
                if list_keys.is_empty() {
                    format!("{} · {}", app.lang.t(Key::FooterTabSwitch), common)
                } else {
                    format!(
                        "{} · {}{}",
                        app.lang.t(Key::FooterTabSwitch),
                        list_keys,
                        common
                    )
                }
            }
        }
    };
    frame.render_widget(
        Paragraph::new(Span::styled(keys, Style::new().dim())),
        keys_area,
    );

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
