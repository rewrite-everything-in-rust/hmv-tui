//! Pure state -> widget rendering for the dashboard.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Row, Table, TableState, Tabs,
};
use ratatui::Frame;

use super::{AppState, InputMode, Tab};

const ACCENT: Color = Color::Rgb(117, 206, 122);
const WARN: Color = Color::Rgb(255, 212, 130);

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
    }

    draw_footer(frame, footer, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &AppState) {
    let stats = &app.data.stats;
    let pending_count = app.data.pending.len();
    let line = Line::from(vec![
        Span::styled(" HMV-CLI", Style::new().fg(ACCENT).bold()),
        Span::styled(" dashboard", Style::new().dim()),
        Span::raw("  ·  "),
        Span::styled(
            format!("{} ", stats.username),
            Style::new().fg(Color::White).bold(),
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
            Style::new().fg(Color::Cyan),
        ),
        Span::raw("  ·  "),
        Span::styled(
            format!("{pending_count} pending"),
            Style::new().fg(if pending_count > 0 { WARN } else { Color::Green }),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &AppState) {
    let titles: Vec<&str> = Tab::ALL.iter().map(|t| t.title()).collect();
    let index = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .select(index)
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

    let rows = app.data.progress.len() as u16;
    if rows > 0 {
        let gauge_area = Rect {
            x: right.x + 2,
            y: right.y + 2,
            width: right.width.saturating_sub(4),
            height: right.height.saturating_sub(2),
        };

        let inner: Vec<Constraint> = app
            .data
            .progress
            .iter()
            .map(|_| Constraint::Length(2))
            .collect();
        let slots = Layout::vertical(inner).split(gauge_area);

        for ((label, value, total), slot) in app.data.progress.iter().zip(slots.iter()) {
            let ratio = if *total == 0 {
                0.0
            } else {
                (*value as f64) / (*total as f64)
            };
            let gauge = Gauge::default()
                .label(format!("{label}: {value} / {total}"))
                .ratio(ratio.clamp(0.0, 1.0))
                .gauge_style(Style::new().fg(ACCENT).bg(Color::DarkGray));
            frame.render_widget(gauge, *slot);
        }
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
            .bg(Color::DarkGray)
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
                Span::styled(name, Style::new().fg(Color::White).bold()),
                Span::styled("  — pwned, writeup not submitted", Style::new().dim()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .bg(Color::DarkGray)
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

fn draw_footer(frame: &mut Frame, area: Rect, app: &AppState) {
    let status = app
        .status
        .clone()
        .map(|s| format!("  {s}"))
        .unwrap_or_default();

    let keys = match app.input_mode {
        InputMode::Filter => "Enter confirm · Esc clear & exit filter",
        InputMode::Normal => "Tab tabs · ↑↓/jk move · / filter · Enter open · r refresh · q quit",
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(keys, Style::new().dim()),
            Span::styled(status, Style::new().fg(WARN)),
        ])),
        area,
    );
}

/// How many table rows fit in `height` (border 2 + header 1).
fn visible_rows_in(height: u16, _len: usize) -> usize {
    height.saturating_sub(3) as usize
}