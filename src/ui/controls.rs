use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{App, AppNotice, InputMode, PlaybackState};

/// Render the bottom control bar: playback status + volume + keybinds.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    render_status_bar(frame, chunks[0], app);
    render_keybinds(frame, chunks[1], app);
}

/// Top row of controls: playback state + station + volume.
fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    // Playback icon + station name
    match (&app.playback, app.now_playing()) {
        (PlaybackState::Playing, Some(station)) => {
            spans.push(Span::styled(" ▶ ", theme::playing()));
            spans.push(Span::styled(station.name.as_str(), theme::cyan()));
            if let Some(ref track) = app.current_track {
                spans.push(Span::styled(" ♫ ", theme::playing()));
                spans.push(Span::styled(
                    track.as_str(),
                    Style::default()
                        .fg(theme::accent_secondary())
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
        (PlaybackState::Paused, Some(station)) => {
            spans.push(Span::styled(" ⏸ ", theme::neon()));
            spans.push(Span::styled(station.name.as_str(), theme::dim()));
        }
        (PlaybackState::Connecting, _) => {
            spans.push(Span::styled(" ◌ ", theme::neon()));
            spans.push(Span::styled(
                "Connecting...",
                Style::default().fg(theme::warm()),
            ));
        }
        (PlaybackState::Error(e), _) => {
            spans.push(Span::styled(" ✗ ", theme::error()));
            spans.push(Span::styled(e.as_str(), theme::error()));
        }
        _ => {
            spans.push(Span::styled(" ■ ", theme::dim()));
            spans.push(Span::styled("Stopped", theme::dim()));
        }
    }

    // Spacer
    spans.push(Span::styled("  ", theme::dim()));

    // Volume bar
    let vol_label = if app.muted {
        "Vol: MUTE ".to_string()
    } else {
        format!("Vol: {}% ", app.volume)
    };
    spans.push(Span::styled(vol_label, theme::dim()));

    // Visual volume bar
    let filled = if app.muted {
        0
    } else {
        app.volume as usize / 5
    };
    let empty = 20 - filled;
    spans.push(Span::styled("█".repeat(filled), theme::vol_filled()));
    spans.push(Span::styled("░".repeat(empty), theme::vol_empty()));

    if let Some(ref notice) = app.notice {
        spans.push(Span::styled("  |  ", theme::dim()));
        match notice {
            AppNotice::Info(message) => {
                spans.push(Span::styled(message.as_str(), theme::playing()));
            }
            AppNotice::Error(message) => {
                spans.push(Span::styled("Save warning: ", theme::error()));
                spans.push(Span::styled(message.as_str(), theme::error()));
            }
        }
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(vec![line]).style(Style::default().bg(theme::bg()));

    frame.render_widget(paragraph, area);
}

/// Bottom row: keyboard shortcut hints, mode-aware.
fn render_keybinds(frame: &mut Frame, area: Rect, app: &App) {
    let hints = match app.input_mode {
        InputMode::Search => Line::from(vec![
            Span::styled(" [", theme::dim()),
            Span::styled("Enter", theme::cyan()),
            Span::styled("] Add to Library+Play  [", theme::dim()),
            Span::styled("Esc", theme::cyan()),
            Span::styled("] Back  [", theme::dim()),
            Span::styled("↑↓", theme::cyan()),
            Span::styled("] Results  ", theme::dim()),
            Span::styled(
                "Type to search...",
                Style::default()
                    .fg(theme::accent_secondary())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
        InputMode::Normal => Line::from(vec![
            Span::styled(" [", theme::dim()),
            Span::styled("Enter", theme::cyan()),
            Span::styled("] Play  [", theme::dim()),
            Span::styled("Space", theme::cyan()),
            Span::styled("] Pause  [", theme::dim()),
            Span::styled("f", theme::cyan()),
            Span::styled("] Remove  [", theme::dim()),
            Span::styled("/", theme::cyan()),
            Span::styled("] Search  [", theme::dim()),
            Span::styled("b", theme::cyan()),
            Span::styled("] Layout  [", theme::dim()),
            Span::styled(",", theme::cyan()),
            Span::styled("] Config  [", theme::dim()),
            Span::styled("h", theme::cyan()),
            Span::styled("] Help  [", theme::dim()),
            Span::styled("q", theme::cyan()),
            Span::styled("] Quit", theme::dim()),
        ]),
    };

    let paragraph = Paragraph::new(vec![hints]).style(Style::default().bg(theme::bg()));

    frame.render_widget(paragraph, area);
}
