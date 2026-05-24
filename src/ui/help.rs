use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Table, Row, Cell, Clear};
use crate::app::App;
use super::theme;

pub fn render(frame: &mut Frame, area: Rect, _app: &App) {
    // A popup rect centering at 60% horizontal, 65% vertical
    let popup_area = super::centered_rect(60, 65, area);

    // Clear the background of the popup area
    frame.render_widget(Clear, popup_area);

    // Beautiful rounded deep purple block with neon pink border
    let block = Block::default()
        .title(Span::styled(" ✦ DriftFM Control HUD ✦ ", theme::title()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD))
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(theme::bg()));

    // Define table rows for key mappings
    let header_row = Row::new(vec![
        Cell::from(Span::styled("Hotkey", Style::default().fg(theme::highlight()).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Action Description", Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD))),
    ])
    .bottom_margin(1);

    let rows = vec![
        Row::new(vec![
            Cell::from(Span::styled(" ▸ Navigation ", Style::default().fg(theme::dim().fg.unwrap()).add_modifier(Modifier::UNDERLINED))),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Up / k  ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Highlight previous station", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Down / j", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Highlight next station", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Enter   ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Play highlighted station", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Tab     ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Cycle genre categories forward", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Shift+Tab", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Cycle genre categories backward", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled(" ▸ Playback ", Style::default().fg(theme::dim().fg.unwrap()).add_modifier(Modifier::UNDERLINED))),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Space   ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Toggle Pause / Play", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  s       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Stop playback stream", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  + / -   ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Adjust volume (scales wave)", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  m       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Mute/unmute stream volume", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  r       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Toggle Tape capture recording", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled(" ▸ Search ", Style::default().fg(theme::dim().fg.unwrap()).add_modifier(Modifier::UNDERLINED))),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  /       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Open interactive catalog search", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Enter   ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("In search: add highlighted station and play", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  f       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("In library: remove highlighted station", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled(" ▸ Layout Toggle ", Style::default().fg(theme::dim().fg.unwrap()).add_modifier(Modifier::UNDERLINED))),
            Cell::from(""),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  b       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Cycle layout modes (Split ↔ Closed ↔ Full Bento)", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  p       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Cycle Right deck pages (Reels ↔ History)", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  v       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Toggle Visualizer (Spectrum ↔ Osc ↔ Sim)", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  ,       ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Toggle Config / Settings menu", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  h / ?   ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Toggle this Help HUD", theme::text())),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("  Esc     ", Style::default().fg(theme::highlight()))),
            Cell::from(Span::styled("Dismiss help/search overlay or quit app", theme::text())),
        ]),
    ];

    let widths = [
        Constraint::Percentage(30),
        Constraint::Percentage(70),
    ];

    let table = Table::new(rows, widths)
        .header(header_row)
        .block(block);

    frame.render_widget(table, popup_area);
}
