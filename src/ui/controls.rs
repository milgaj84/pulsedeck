use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{App, AppNotice, InputMode, LayoutMode, PlaybackState};

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

    if app.local_playback_path.is_some() {
        render_local_tape_status(&mut spans, app);
    } else {
        match (&app.playback, app.now_playing()) {
            (PlaybackState::Playing, Some(station)) => {
                status_chip(&mut spans, "▶", "PLAY", theme::playing());
                spans.push(Span::styled(station.name.as_str(), theme::cyan()));
                if let Some(ref track) = app.current_track {
                    spans.push(Span::styled("  ♫ ", theme::playing()));
                    spans.push(Span::styled(
                        track.as_str(),
                        Style::default()
                            .fg(theme::accent_secondary())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            (PlaybackState::FadingOut { .. }, Some(station)) => {
                status_chip(&mut spans, "◒", "FADE", Style::default().fg(theme::warm()));
                spans.push(Span::styled(station.name.as_str(), theme::dim()));
            }
            (PlaybackState::Paused, Some(station)) => {
                status_chip(&mut spans, "⏸", "PAUSE", theme::neon());
                spans.push(Span::styled(station.name.as_str(), theme::dim()));
            }
            (PlaybackState::Connecting, _) => {
                status_chip(&mut spans, "◌", "TUNE", Style::default().fg(theme::warm()));
                spans.push(Span::styled(
                    "Connecting...",
                    Style::default().fg(theme::warm()),
                ));
            }
            (PlaybackState::Error(e), _) => {
                status_chip(&mut spans, "✗", "ERROR", theme::error());
                spans.push(Span::styled(e.as_str(), theme::error()));
            }
            _ => {
                status_chip(&mut spans, "■", "STOP", theme::dim());
                spans.push(Span::styled("Select a station", theme::dim()));
            }
        }
    }

    spans.push(Span::styled("  │  ", theme::dim()));
    spans.push(Span::styled(volume_label(app), theme::dim()));
    spans.push(Span::styled(volume_bar_fill(app), theme::vol_filled()));
    spans.push(Span::styled(volume_bar_empty(app), theme::vol_empty()));

    spans.push(Span::styled("  │  ", theme::dim()));
    spans.push(Span::styled(layout_label(app.layout_mode), theme::dim()));
    spans.push(Span::styled("  ", theme::dim()));
    spans.push(Span::styled(
        visualizer_label(app.visualizer_mode),
        theme::dim(),
    ));

    if let Some(ref notice) = app.notice {
        spans.push(Span::styled("  │  ", theme::dim()));
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

fn status_chip(spans: &mut Vec<Span>, icon: &'static str, label: &'static str, style: Style) {
    spans.push(Span::styled(" ", theme::dim()));
    spans.push(Span::styled(icon, style));
    spans.push(Span::styled(" ", theme::dim()));
    spans.push(Span::styled(label, style.add_modifier(Modifier::BOLD)));
    spans.push(Span::styled("  ", theme::dim()));
}

fn render_local_tape_status(spans: &mut Vec<Span>, app: &App) {
    match app.playback {
        PlaybackState::Playing => status_chip(spans, "▶", "TAPE", theme::playing()),
        PlaybackState::Paused => status_chip(spans, "⏸", "TAPE", theme::neon()),
        PlaybackState::FadingOut { .. } => {
            status_chip(spans, "◒", "TAPE", Style::default().fg(theme::warm()));
        }
        PlaybackState::Connecting => {
            status_chip(spans, "◌", "TAPE", Style::default().fg(theme::warm()));
        }
        PlaybackState::Error(_) => status_chip(spans, "✗", "TAPE", theme::error()),
        PlaybackState::Stopped => status_chip(spans, "■", "TAPE", theme::dim()),
    }

    if let Some(track) = app.current_track.as_ref() {
        spans.push(Span::styled(
            track.clone(),
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        ));
    } else if let Some(path) = app.local_playback_path.as_ref() {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Local tape");
        spans.push(Span::styled(name.to_string(), theme::cyan()));
    } else {
        spans.push(Span::styled("Local tape", theme::dim()));
    }
}

fn volume_label(app: &App) -> String {
    if app.muted {
        "VOL MUTE ".to_string()
    } else {
        format!("VOL {:>3}% ", app.volume)
    }
}

fn volume_bar_fill(app: &App) -> String {
    let filled = if app.muted {
        0
    } else {
        app.volume as usize / 5
    };

    "█".repeat(filled)
}

fn volume_bar_empty(app: &App) -> String {
    let filled = if app.muted {
        0
    } else {
        app.volume as usize / 5
    };
    let empty = 20 - filled;

    "░".repeat(empty)
}

fn layout_label(layout_mode: LayoutMode) -> &'static str {
    match layout_mode {
        LayoutMode::Split => "LAYOUT SPLIT",
        LayoutMode::LeftOnly => "LAYOUT LIBRARY",
        LayoutMode::RightOnly => "LAYOUT DECK",
    }
}

fn visualizer_label(visualizer_mode: usize) -> &'static str {
    match visualizer_mode {
        0 => "SCOPE RTA",
        1 => "SCOPE OSC",
        _ => "SCOPE SIM",
    }
}

/// Bottom row: keyboard shortcut hints, mode-aware.
fn render_keybinds(frame: &mut Frame, area: Rect, app: &App) {
    let hints = match app.input_mode {
        InputMode::TapeRename => Line::from(vec![
            Span::styled(" [", theme::dim()),
            Span::styled("type", theme::cyan()),
            Span::styled("] Rename tape  [", theme::dim()),
            Span::styled("Enter", theme::cyan()),
            Span::styled("] Save  [", theme::dim()),
            Span::styled("Backspace", theme::cyan()),
            Span::styled("] Edit  [", theme::dim()),
            Span::styled("Esc", theme::cyan()),
            Span::styled("] Cancel", theme::dim()),
        ]),
        InputMode::TapeMove => Line::from(vec![
            Span::styled(" [", theme::dim()),
            Span::styled("type", theme::cyan()),
            Span::styled("] Target folder  [", theme::dim()),
            Span::styled("Enter", theme::cyan()),
            Span::styled("] Move  [", theme::dim()),
            Span::styled("Backspace", theme::cyan()),
            Span::styled("] Edit  [", theme::dim()),
            Span::styled("Esc", theme::cyan()),
            Span::styled("] Cancel", theme::dim()),
        ]),
        InputMode::TapeFilter => Line::from(vec![
            Span::styled(" [", theme::dim()),
            Span::styled("type", theme::cyan()),
            Span::styled("] Filter tapes  [", theme::dim()),
            Span::styled("Backspace", theme::cyan()),
            Span::styled("] Edit  [", theme::dim()),
            Span::styled("Enter", theme::cyan()),
            Span::styled("] Play/Open  [", theme::dim()),
            Span::styled("Esc", theme::cyan()),
            Span::styled("] Clear  [", theme::dim()),
            Span::styled("Ctrl+r", theme::cyan()),
            Span::styled("] Refresh", theme::dim()),
        ]),
        InputMode::Search => Line::from(vec![
            Span::styled(" [", theme::dim()),
            Span::styled("Space", theme::cyan()),
            Span::styled("] Audition  [", theme::dim()),
            Span::styled("Enter", theme::cyan()),
            Span::styled("] Save+Play  [", theme::dim()),
            Span::styled("Esc", theme::cyan()),
            Span::styled("] Back  [", theme::dim()),
            Span::styled("↑↓", theme::cyan()),
            Span::styled("] Results  ", theme::dim()),
            Span::styled(
                "worldwide station search",
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
            Span::styled("b", theme::cyan()),
            Span::styled("] Layout  [", theme::dim()),
            Span::styled("v", theme::cyan()),
            Span::styled("] Scope  [", theme::dim()),
            Span::styled("/", theme::cyan()),
            Span::styled("] Search  [", theme::dim()),
            Span::styled("f", theme::cyan()),
            Span::styled("] Remove  [", theme::dim()),
            Span::styled("u", theme::cyan()),
            Span::styled("] Undo  [", theme::dim()),
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
