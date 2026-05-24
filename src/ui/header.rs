use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::theme;
use crate::app::{App, PlaybackState};

/// Render the header area: DriftFM logo + now-playing info.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_logo(frame, chunks[0]);
    render_now_playing(frame, chunks[1], app);
}

/// ASCII-styled DriftFM logo.
fn render_logo(frame: &mut Frame, area: Rect) {
    let logo = vec![Line::from(vec![
        Span::styled("  ░▒▓ ", theme::dim()),
        Span::styled("D R I F T", theme::neon()),
        Span::styled(" F M", theme::cyan()),
        Span::styled(" ▓▒░", theme::dim()),
    ])];

    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(theme::bg()));

    let paragraph = Paragraph::new(logo).block(block).alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
}

/// Now-playing status on the right side of the header.
fn render_now_playing(frame: &mut Frame, area: Rect, app: &App) {
    let content = match (&app.playback, app.now_playing()) {
        (PlaybackState::Playing, Some(station)) => Line::from(vec![
            Span::styled("▶ ", theme::playing()),
            Span::styled(&station.name, theme::cyan()),
            Span::styled(" ◉ LIVE", theme::playing()),
        ]),
        (PlaybackState::Paused, Some(station)) => Line::from(vec![
            Span::styled("⏸ ", theme::neon()),
            Span::styled(&station.name, theme::dim()),
            Span::styled(" (paused)", theme::dim()),
        ]),
        (PlaybackState::Connecting, _) => Line::from(vec![
            Span::styled("◌ ", theme::neon()),
            Span::styled("Connecting...", Style::default().fg(theme::warm())),
        ]),
        (PlaybackState::Error(e), _) => Line::from(vec![
            Span::styled("✗ ", theme::error()),
            Span::styled(e.as_str(), theme::error()),
        ]),
        _ => Line::from(vec![
            Span::styled("◇ ", theme::dim()),
            Span::styled("Select a station", theme::dim()),
        ]),
    };

    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(theme::bg()));

    let paragraph = Paragraph::new(vec![content])
        .block(block)
        .alignment(Alignment::Right);

    frame.render_widget(paragraph, area);
}
