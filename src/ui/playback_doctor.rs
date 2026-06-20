use crate::app::{DecoderState, PlaybackState};
use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::theme;

const MIN_DOCTOR_WIDTH: u16 = 64;
const MIN_DOCTOR_HEIGHT: u16 = 18;

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let popup_area = super::centered_rect(72, 72, area);
    frame.render_widget(Clear, popup_area);

    if doctor_area_is_compact(popup_area) {
        super::render_boundary_warning(
            frame,
            popup_area,
            "Playback Doctor Too Compact",
            format!(
                "Expand terminal or close doctor (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    let block = Block::default()
        .title(Span::styled(" Playback Doctor ", theme::title()))
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::highlight()))
        .style(theme::clear());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let station = app.now_playing().map(|s| s.name.as_str()).unwrap_or("N/A");
    let url = app.player.playing_url.as_deref().unwrap_or("N/A");
    let track = app.player.current_track.as_deref().unwrap_or("N/A");
    let last_error = app.diagnostics.last_error.as_deref().unwrap_or("N/A");
    let action_hint = match &app.player.state {
        PlaybackState::Error(error) => crate::app::playback_error_action_hint(error),
        _ => "r retry  s stop  , output  / search  Esc close",
    };

    let lines = vec![
        row("State", playback_state_label(&app.player.state)),
        row("Station", station),
        row("Track", track),
        row("URL", url),
        row("Output", &app.diagnostics.output_device),
        row(
            "Song info",
            if app.diagnostics.metadata_enabled { "On" } else { "Off" },
        ),
        row("Decoder", decoder_state_label(&app.diagnostics.decoder_state)),
        row(
            "Buffer",
            &format!(
                "{}% / {}s",
                app.diagnostics.buffer_percent, app.diagnostics.buffer_seconds
            ),
        ),
        row(
            "Reconnects",
            &format!(
                "{} / {}",
                app.diagnostics.reconnect_attempts, app.diagnostics.reconnect_limit
            ),
        ),
        row(
            "Last event",
            app.diagnostics.last_event.as_deref().unwrap_or("N/A"),
        ),
        row("Last error", last_error),
        row(
            "Last recovery",
            app.diagnostics.last_recovery.as_deref().unwrap_or("N/A"),
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("Actions: ", theme::dim()),
            Span::styled(action_hint.to_string(), theme::cyan()),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .style(theme::clear())
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn doctor_area_is_compact(area: Rect) -> bool {
    area.width < MIN_DOCTOR_WIDTH || area.height < MIN_DOCTOR_HEIGHT
}

fn row(label: &'static str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:>13}: "), theme::dim()),
        Span::styled(value.to_string(), theme::text()),
    ])
}

pub(crate) fn playback_state_label(state: &PlaybackState) -> &'static str {
    match state {
        PlaybackState::Stopped => "Stopped",
        PlaybackState::Connecting => "Connecting",
        PlaybackState::Playing => "Playing",
        PlaybackState::FadingOut { .. } => "Fading out",
        PlaybackState::Paused => "Paused",
        PlaybackState::Error(_) => "Error",
    }
}

fn decoder_state_label(state: &DecoderState) -> &'static str {
    match state {
        DecoderState::Idle => "Idle",
        DecoderState::Connecting => "Connecting",
        DecoderState::Probing => "Probing",
        DecoderState::Playing => "Playing",
        DecoderState::Ended => "Ended",
        DecoderState::Failed => "Failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_overlay_rejects_tiny_area() {
        assert!(doctor_area_is_compact(Rect::new(0, 0, 63, 18)));
        assert!(doctor_area_is_compact(Rect::new(0, 0, 64, 17)));
        assert!(!doctor_area_is_compact(Rect::new(0, 0, 64, 18)));
    }

    #[test]
    fn playback_state_label_formats_all_states() {
        assert_eq!(playback_state_label(&PlaybackState::Stopped), "Stopped");
        assert_eq!(playback_state_label(&PlaybackState::Connecting), "Connecting");
        assert_eq!(playback_state_label(&PlaybackState::Playing), "Playing");
        assert_eq!(
            playback_state_label(&PlaybackState::FadingOut { current_volume: 0.5 }),
            "Fading out"
        );
        assert_eq!(playback_state_label(&PlaybackState::Paused), "Paused");
        assert_eq!(
            playback_state_label(&PlaybackState::Error("boom".to_string())),
            "Error"
        );
    }

    #[test]
    fn decoder_state_label_formats_all_states() {
        assert_eq!(decoder_state_label(&DecoderState::Idle), "Idle");
        assert_eq!(decoder_state_label(&DecoderState::Connecting), "Connecting");
        assert_eq!(decoder_state_label(&DecoderState::Probing), "Probing");
        assert_eq!(decoder_state_label(&DecoderState::Playing), "Playing");
        assert_eq!(decoder_state_label(&DecoderState::Ended), "Ended");
        assert_eq!(decoder_state_label(&DecoderState::Failed), "Failed");
    }
}
