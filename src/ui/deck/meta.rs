use crate::app::PlaybackState;
use crate::ui::model::UiModel;
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub(super) fn render_meta_details(frame: &mut Frame, area: Rect, app: &UiModel<'_>, full_deck: bool) {
    let (status_text, status_style) = match app.player.state {
        PlaybackState::Playing => ("PLAYING", theme::playing()),
        PlaybackState::FadingOut { .. } => (
            "FADING...",
            Style::default()
                .fg(theme::warm())
                .add_modifier(Modifier::BOLD),
        ),
        PlaybackState::Connecting => (
            "TUNING...",
            Style::default()
                .fg(theme::warm())
                .add_modifier(Modifier::BOLD),
        ),
        PlaybackState::Paused => ("PAUSED", theme::neon()),
        PlaybackState::Error(_) => ("OFFLINE / ERROR", theme::error()),
        PlaybackState::Stopped => ("STOPPED", theme::dim()),
    };

    let station = app.now_playing();
    let genre = station
        .map(|s| s.genre.clone())
        .unwrap_or_else(|| "N/A".to_string());
    let country = station
        .map(|s| s.country.clone())
        .unwrap_or_else(|| "N/A".to_string());

    let filled = (app.player.buffer_percent / 10) as usize;
    let empty = 10 - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(" ▶ ", status_style),
        Span::styled(status_text, status_style),
        Span::styled("   GENRE ", theme::dim()),
        Span::styled(genre, theme::cyan()),
        Span::styled("   ORIGIN ", theme::dim()),
        Span::styled(country, theme::cyan()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" BUFFER ", theme::dim()),
        Span::styled(
            format!("[{}] ", bar),
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{}% ", app.player.buffer_percent), theme::cyan()),
        Span::styled(format!("({}s)", app.player.buffer_seconds), theme::dim()),
    ]));

    let title = if full_deck {
        " SIGNAL STATUS "
    } else {
        " SIGNAL "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(title, theme::title()));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
