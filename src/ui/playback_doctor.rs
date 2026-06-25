use crate::app::doctor_suggestions::suggest_actions;
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

    let mut lines = vec![
        row("State", playback_state_label(&app.player.state)),
        row("Station", station),
        row("Track", track),
        row("URL", url),
        row("Output", &app.diagnostics.output_device),
        row(
            "Song info",
            if app.diagnostics.metadata_enabled {
                "On"
            } else {
                "Off"
            },
        ),
        row(
            "Decoder",
            decoder_state_label(&app.diagnostics.decoder_state),
        ),
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
    ];

    lines.extend(exclusion_diagnostics_lines(app));
    lines.extend(suggestion_lines(app));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Actions: ", theme::dim()),
        Span::styled(action_hint.to_string(), theme::cyan()),
    ]));

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

fn exclusion_diagnostics_lines(app: &UiModel<'_>) -> Vec<Line<'static>> {
    let has_exclusions = !app.exclude_tags.is_empty() || !app.exclude_countries.is_empty();
    if !app.discover_results_empty || !has_exclusions {
        return vec![];
    }

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled("── Discover Exclusions ──", theme::cyan())),
        Line::from(Span::styled(
            "Exclusion lists may be filtering all discover candidates",
            theme::dim(),
        )),
    ];

    if !app.exclude_tags.is_empty() {
        let tags = app.exclude_tags.join(", ");
        lines.push(row("Tags", &tags));
    }

    if !app.exclude_countries.is_empty() {
        let countries = app.exclude_countries.join(", ");
        lines.push(row("Countries", &countries));
    }

    lines
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

fn suggestion_lines(app: &UiModel<'_>) -> Vec<Line<'static>> {
    let suggestions = suggest_actions(app.diagnostics);
    if suggestions.is_empty() {
        return vec![];
    }

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled("── Suggestions ──", theme::cyan())),
    ];

    for hint in suggestions {
        lines.push(Line::from(Span::styled(
            format!("  💡 {hint}"),
            theme::text(),
        )));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::favorites::Library;
    use crate::radio::Station;
    use crate::recommend::ScoredStation;

    #[test]
    fn doctor_overlay_rejects_tiny_area() {
        assert!(doctor_area_is_compact(Rect::new(0, 0, 63, 18)));
        assert!(doctor_area_is_compact(Rect::new(0, 0, 64, 17)));
        assert!(!doctor_area_is_compact(Rect::new(0, 0, 64, 18)));
    }

    #[test]
    fn playback_state_label_formats_all_states() {
        assert_eq!(playback_state_label(&PlaybackState::Stopped), "Stopped");
        assert_eq!(
            playback_state_label(&PlaybackState::Connecting),
            "Connecting"
        );
        assert_eq!(playback_state_label(&PlaybackState::Playing), "Playing");
        assert_eq!(
            playback_state_label(&PlaybackState::FadingOut {
                current_volume: 0.5
            }),
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

    #[test]
    fn test_exclusion_section_shown_when_empty_results_and_exclusions_present() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.config.discover.exclude_tags = vec![
            "politics".to_string(),
            "news".to_string(),
            "sports".to_string(),
        ];
        app.config.discover.exclude_countries = vec!["US".to_string(), "GB".to_string()];

        let model = UiModel::from(&app);
        let lines = exclusion_diagnostics_lines(&model);

        assert!(!lines.is_empty(), "section should be rendered");

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect::<Vec<_>>()
            .join(" ");

        assert!(text.contains("Discover Exclusions"));
        assert!(text.contains("Exclusion lists may be filtering all discover candidates"));
        assert!(text.contains("politics, news, sports"));
        assert!(text.contains("US, GB"));
    }

    #[test]
    fn test_exclusion_section_hidden_when_results_non_empty() {
        let mut app = App::new(Library::in_memory(vec![]));
        app.config.discover.exclude_tags = vec!["politics".to_string()];
        app.config.discover.exclude_countries = vec!["US".to_string()];
        app.discover_results = vec![ScoredStation {
            station: Station::basic("Test", "http://test", "Jazz", "DE", 128),
            score: 5,
        }];

        let model = UiModel::from(&app);
        let lines = exclusion_diagnostics_lines(&model);

        assert!(lines.is_empty(), "section should not be rendered");
    }

    #[test]
    fn test_exclusion_section_hidden_when_no_exclusions() {
        let app = App::new(Library::in_memory(vec![]));

        let model = UiModel::from(&app);
        let lines = exclusion_diagnostics_lines(&model);

        assert!(
            lines.is_empty(),
            "section should not be rendered when no exclusions configured"
        );
    }
}
