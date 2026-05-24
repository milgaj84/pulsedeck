use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Clear, Row, Table};

use super::theme;

pub fn render(frame: &mut Frame, area: Rect, _app: &App) {
    // Keep the help compact so it still fits when terminal fonts are large.
    let popup_area = super::centered_rect(68, 58, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ✦ DriftFM Help ✦ ", theme::title()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(theme::bg()));

    let header_row = Row::new(vec![
        Cell::from(Span::styled("Key", Style::default().fg(theme::highlight()).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Action", Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD))),
    ]);

    let rows = vec![
        section("Playback"),
        shortcut("Enter", "Play selected station; in search: add to Library + play"),
        shortcut("Space", "Pause / resume"),
        shortcut("s", "Stop playback"),
        shortcut("+ / -", "Volume up / down"),
        shortcut("m", "Mute / unmute"),
        shortcut("r", "Start / stop tape recording"),
        section("Library"),
        shortcut("↑↓ / j k", "Move selection"),
        shortcut("Tab / Shift+Tab", "Change genre category"),
        shortcut("f", "Remove selected station from Library"),
        section("Search"),
        shortcut("/", "Open worldwide station search"),
        shortcut("Type", "Filter/search by station, tag, city, or country"),
        shortcut("↑↓", "Move through search results"),
        shortcut("Enter", "Save highlighted result to Library and play it"),
        shortcut("Esc", "Leave search without adding"),
        section("Views & App"),
        shortcut("b", "Cycle layout"),
        shortcut("p", "Cycle deck page"),
        shortcut("v", "Cycle visualizer"),
        shortcut(",", "Open settings"),
        shortcut("h / ?", "Show / hide help"),
        shortcut("q / Esc", "Quit, or close overlay first"),
    ];

    let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
    let table = Table::new(rows, widths).header(header_row).block(block);

    frame.render_widget(table, popup_area);
}

fn section(label: &'static str) -> Row<'static> {
    Row::new(vec![
        Cell::from(Span::styled(
            format!("▸ {label}"),
            Style::default()
                .fg(theme::dim().fg.unwrap())
                .add_modifier(Modifier::UNDERLINED),
        )),
        Cell::from(""),
    ])
}

fn shortcut(key: &'static str, action: &'static str) -> Row<'static> {
    Row::new(vec![
        Cell::from(Span::styled(key, Style::default().fg(theme::highlight()))),
        Cell::from(Span::styled(action, theme::text())),
    ])
}
