use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Clear, Row, Table};

use super::{critical, theme};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Keep the help compact so it still fits when terminal fonts are large.
    let popup_area = super::centered_rect(70, 60, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ✦ PulseDeck Controls ✦ ", theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(theme::bg()));

    let inner_area = block.inner(popup_area);
    let (content_area, alert_area) = critical::split_overlay_alert_area(inner_area, &app.playback);

    let header_row = Row::new(vec![
        Cell::from(Span::styled(
            "Key",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Action",
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )),
    ]);

    let rows = vec![
        section("Playback"),
        shortcut(
            "Enter",
            "Play selected station; in search: save to Library + play",
        ),
        shortcut("Space", "Pause / resume"),
        shortcut("s", "Stop playback"),
        shortcut("+ / -", "Volume up / down"),
        shortcut("m", "Mute / unmute"),
        shortcut("r", "Arm / stop tape recording"),
        section("Library"),
        shortcut("↑↓ / j k", "Move selection"),
        shortcut("Tab / Shift+Tab", "Change genre category"),
        shortcut("f", "Remove selected station from Library"),
        shortcut("u", "Undo the most recent station removal"),
        section("Search"),
        shortcut("/", "Open worldwide station search"),
        shortcut("Type", "Search by station, tag, city, or country"),
        shortcut("Space", "Audition highlighted result without saving"),
        shortcut("Ctrl+Enter", "Audition too, when your terminal supports it"),
        shortcut("Enter", "Save highlighted result to Library and play it"),
        shortcut("Esc", "Leave search without adding"),
        shortcut("Ctrl/Alt +/-/m", "Volume/mute while staying in search"),
        section("Deck & Visuals"),
        shortcut("b", "Cycle Split / Library / Deck layout"),
        shortcut("p", "Switch Tape Deck / Tape History"),
        shortcut("v", "Cycle RTA Spectrum / Real Osc / Sim Osc"),
        section("Settings"),
        shortcut(",", "Open settings"),
        shortcut("↑↓ / j k", "Move setting selection"),
        shortcut("Space / → / l", "Advance highlighted setting"),
        shortcut("← / h", "Step highlighted setting back"),
        shortcut("Audio Output", "Choose Default, pulse, pipewire, or device"),
        section("App"),
        shortcut("h / ?", "Show / hide this help"),
        shortcut("q / Esc", "Quit, or close overlay first"),
    ];

    let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
    let table = Table::new(rows, widths).header(header_row);

    frame.render_widget(block, popup_area);
    frame.render_widget(table, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.playback);
    }
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
