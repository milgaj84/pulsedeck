use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::{AppNotice, InputMode, LayoutMode, PlaybackState, VisualizerMode};
use crate::ui::model::UiModel;

/// Render the bottom control bar: playback status + volume + keybinds.
pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    render_status_bar(frame, chunks[0], app);
    render_keybinds(frame, chunks[1], app);
}

/// Top row of controls: playback state + station + volume.
fn render_status_bar(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let mut spans: Vec<Span> = Vec::new();

    match (&app.player.state, app.now_playing()) {
        (PlaybackState::Playing, Some(station)) => {
            status_chip(&mut spans, "▶", "PLAY", theme::playing());
            spans.push(Span::styled(station.name.as_str(), theme::cyan()));
            if let Some(ref track) = app.player.current_track {
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

    if let Some(remaining) = app.sleep_timer.remaining(std::time::Instant::now()) {
        let secs = remaining.as_secs();
        spans.push(Span::styled("  │  ", theme::dim()));
        spans.push(Span::styled(
            format!("\u{1F4A4} {:02}:{:02}", secs / 60, secs % 60),
            theme::dim(),
        ));
    }

    if let Some(ref notice) = app.notice.current {
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

fn volume_label(app: &UiModel<'_>) -> String {
    if app.muted {
        "VOL MUTE ".to_string()
    } else {
        format!("VOL {:>3}% ", app.volume)
    }
}

fn volume_bar_fill(app: &UiModel<'_>) -> String {
    let filled = if app.muted {
        0
    } else {
        app.volume as usize / 5
    };

    "█".repeat(filled)
}

fn volume_bar_empty(app: &UiModel<'_>) -> String {
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
        LayoutMode::Split => "SPLIT VIEW",
        LayoutMode::LeftOnly => "LIBRARY FOCUS",
        LayoutMode::RightOnly => "SIGNAL FOCUS",
    }
}

fn visualizer_label(visualizer_mode: VisualizerMode) -> &'static str {
    match visualizer_mode {
        VisualizerMode::Spectrum => "RTA",
        VisualizerMode::RealOscilloscope => "REAL OSC",
        VisualizerMode::SimOscilloscope => "SIM OSC",
    }
}

/// Bottom row: keyboard shortcut hints, mode-aware.
fn render_keybinds(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let paragraph = Paragraph::new(vec![footer_line(app)]).style(Style::default().bg(theme::bg()));

    frame.render_widget(paragraph, area);
}

fn footer_line(app: &UiModel<'_>) -> Line<'static> {
    if app.show_help() {
        return hint_line(
            &[("h/?/Esc/q", "Close help")],
            Some("full control reference"),
        );
    }
    if app.show_station_details() {
        return hint_line(
            &[("i/Esc/q", "Close details")],
            Some("selected station info"),
        );
    }
    if app.show_recent_tracks() {
        let suffix = if app.library.settings.save_history {
            "persistent track history"
        } else {
            "session track list"
        };
        return hint_line(&[("g/Esc/q", "Close recent tracks")], Some(suffix));
    }

    if app.show_sleep_timer() {
        return hint_line(
            &[
                ("↑/+ ↓/-", "±5 min"),
                ("1-6", "Presets"),
                ("0/c", "Off"),
                ("t/Esc", "Close"),
            ],
            Some("sleep timer"),
        );
    }

    match app.input_mode {
        InputMode::Search => hint_line(
            &[
                ("Space", "Audition"),
                ("Enter", "Save+Play"),
                ("Esc", "Back"),
                ("↑↓", "Results"),
            ],
            Some("worldwide station search"),
        ),
        InputMode::CommandPalette => hint_line(
            &[("↑↓", "Select"), ("Enter", "Run"), ("Esc", "Close")],
            Some("command palette"),
        ),
        InputMode::Normal => normal_mode_footer(app),
        InputMode::SleepTimer => normal_mode_footer(app),
        InputMode::LibraryFilter => normal_mode_footer(app),
    }
}

fn normal_mode_footer(app: &UiModel<'_>) -> Line<'static> {
    if matches!(app.player.state, PlaybackState::Error(_)) {
        return hint_line(
            &[
                ("r", "Retry"),
                ("s", "Stop"),
                (",", "Audio Output"),
                ("/", "Search"),
            ],
            Some("recover playback"),
        );
    }

    if app.visible_count() == 0 {
        return hint_line(
            &[
                ("/", "Search"),
                (",", "Settings"),
                ("h", "Help"),
                ("q", "Quit"),
            ],
            Some("start by finding a station"),
        );
    }

    if app.notice.current.is_some() {
        return hint_line(
            &[
                ("u", "Undo"),
                ("Enter", "Play"),
                ("/", "Search"),
                ("h", "Help"),
            ],
            Some("last action available"),
        );
    }

    match app.player.state {
        PlaybackState::Playing
        | PlaybackState::Paused
        | PlaybackState::Connecting
        | PlaybackState::FadingOut { .. } => hint_line(
            &[
                ("Space", "Pause/Stop"),
                ("s", "Stop"),
                ("+/-", "Volume"),
                ("v", "Visualizer"),
                ("i", "Details"),
                ("g", "Tracks"),
            ],
            Some("listening"),
        ),
        PlaybackState::Stopped | PlaybackState::Error(_) => hint_line(
            &[
                ("Enter", "Play"),
                ("/", "Search"),
                ("i", "Details"),
                ("b", "Layout"),
                (",", "Settings"),
                ("h", "Help"),
            ],
            None,
        ),
    }
}

fn hint_line(
    hints: &[(&'static str, &'static str)],
    suffix: Option<&'static str>,
) -> Line<'static> {
    let mut spans = Vec::new();

    for (idx, (key, label)) in hints.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled("  ", theme::dim()));
        }
        spans.push(Span::styled(" [", theme::dim()));
        spans.push(Span::styled(*key, theme::cyan()));
        spans.push(Span::styled("] ", theme::dim()));
        spans.push(Span::styled(*label, theme::dim()));
    }

    if let Some(suffix) = suffix {
        spans.push(Span::styled("  ", theme::dim()));
        spans.push(Span::styled(
            suffix,
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::ITALIC),
        ));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_labels_use_user_facing_focus_terms() {
        assert_eq!(layout_label(LayoutMode::Split), "SPLIT VIEW");
        assert_eq!(layout_label(LayoutMode::LeftOnly), "LIBRARY FOCUS");
        assert_eq!(layout_label(LayoutMode::RightOnly), "SIGNAL FOCUS");
    }

    #[test]
    fn visualizer_labels_drop_scope_jargon() {
        assert_eq!(visualizer_label(VisualizerMode::Spectrum), "RTA");
        assert_eq!(
            visualizer_label(VisualizerMode::RealOscilloscope),
            "REAL OSC"
        );
        assert_eq!(visualizer_label(VisualizerMode::SimOscilloscope), "SIM OSC");
    }
}
