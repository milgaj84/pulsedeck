use super::critical;
use super::theme;
use super::theme::ThemeName;
use crate::app::SettingRow;
use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const MIN_SETTINGS_WIDTH: u16 = 60;
const MIN_SETTINGS_HEIGHT: u16 = 18;

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let popup_area = super::centered_rect(54, 64, area);

    if settings_area_is_compact(popup_area) {
        frame.render_widget(Clear, popup_area);
        super::render_boundary_warning(
            frame,
            popup_area,
            "Settings Too Compact",
            format!(
                "Expand terminal or close settings (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" PulseDeck Settings ", theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear());

    let inner_area = block.inner(popup_area);
    let (content_area, alert_area) =
        critical::split_overlay_alert_area(inner_area, &app.player.state);
    frame.render_widget(block, popup_area);

    let constraints = std::iter::once(Constraint::Length(1))
        .chain(SettingRow::ALL.iter().map(|_| Constraint::Length(2)))
        .chain([Constraint::Length(2), Constraint::Min(0)])
        .collect::<Vec<_>>();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(content_area);

    for (offset, row) in SettingRow::ALL.iter().copied().enumerate() {
        render_setting_row(frame, chunks[offset + 1], app, row);
    }

    render_selected_description(frame, chunks[SettingRow::COUNT + 1], app);
    render_footer(frame, chunks[SettingRow::COUNT + 2]);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.player.state);
    }
}

fn settings_area_is_compact(area: Rect) -> bool {
    area.width < MIN_SETTINGS_WIDTH || area.height < MIN_SETTINGS_HEIGHT
}

fn render_setting_row(frame: &mut Frame, area: Rect, app: &UiModel<'_>, row: SettingRow) {
    let is_selected = app.overlays.selected_setting_idx == row.index();
    let active_style = Style::default()
        .fg(theme::accent_secondary())
        .add_modifier(Modifier::BOLD);
    let normal_style = Style::default().fg(theme::text().fg.unwrap());
    let active_bg = Style::default().bg(theme::surface_color());

    let cursor_style = active_style;
    let label_style = if is_selected {
        active_style
    } else {
        normal_style
    };

    let mut spans = vec![Span::styled(
        setting_row_cursor_symbol(is_selected),
        cursor_style,
    )];
    spans.extend(setting_row_spans(app, row, label_style));

    let mut paragraph = Paragraph::new(Line::from(spans));
    if is_selected {
        paragraph = paragraph.style(active_bg);
    }
    frame.render_widget(paragraph, area);
}

fn render_selected_description(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let row = SettingRow::from_index(app.overlays.selected_setting_idx)
        .unwrap_or(SettingRow::Notifications);
    let paragraph = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Hint: ", theme::cyan()),
            Span::styled(setting_description(row), theme::dim()),
        ]),
    ]);
    frame.render_widget(paragraph, area);
}

fn setting_row_cursor_symbol(is_selected: bool) -> &'static str {
    if is_selected {
        " >  "
    } else {
        "    "
    }
}

fn setting_description(row: SettingRow) -> &'static str {
    match row {
        SettingRow::Notifications => "Show current track changes while you listen.",
        SettingRow::AutoplayLast => "Start the previous station automatically on launch.",
        SettingRow::OutputDevice => {
            "Choose Default or a detected speaker, headset, pulse, or pipewire device."
        }
        SettingRow::Theme => {
            "Change PulseDeck's color palette instantly; settings save automatically."
        }
        SettingRow::StreamMetadata => {
            "Request stream song-title metadata when stations support it."
        }
        SettingRow::SaveHistory => {
            "Save played tracks to history.json so they persist across restarts."
        }
    }
}

fn setting_row_spans(app: &UiModel<'_>, row: SettingRow, label_style: Style) -> Vec<Span<'static>> {
    match row {
        SettingRow::Notifications => checkbox_row(
            app.library.settings.notifications_enabled,
            "Desktop Song Notifications",
            label_style,
        ),
        SettingRow::AutoplayLast => checkbox_row(
            app.library.settings.autoplay_last,
            "Autoplay Last Played Station on Boot",
            label_style,
        ),
        SettingRow::OutputDevice => vec![
            icon_span("[ audio ] "),
            Span::styled("Audio Output: ", label_style),
            Span::styled(
                audio_output_label(app.library.settings.output_device_name.as_deref()),
                Style::default()
                    .fg(theme::highlight())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" (Space/Right forward, Left back)", theme::dim()),
        ],
        SettingRow::Theme => {
            let current_theme = ThemeName::from_key(&app.library.settings.theme);
            vec![
                icon_span("[ theme ] "),
                Span::styled("Theme: ", label_style),
                Span::styled(
                    format!("{} ", current_theme.label()),
                    Style::default()
                        .fg(theme::highlight())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("(Space/Right forward, Left back)", theme::dim()),
            ]
        }
        SettingRow::StreamMetadata => checkbox_row(
            app.library.settings.stream_metadata_enabled,
            "Stream Song Info Metadata",
            label_style,
        ),
        SettingRow::SaveHistory => checkbox_row(
            app.library.settings.save_history,
            "Save Track History (g Overlay)",
            label_style,
        ),
    }
}

fn audio_output_label(value: Option<&str>) -> String {
    crate::audio::output_device_display_name(value)
}

fn checkbox_row(enabled: bool, label: &'static str, label_style: Style) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            if enabled { "[x] " } else { "[ ] " },
            Style::default()
                .fg(if enabled {
                    theme::accent_secondary()
                } else {
                    theme::dim().fg.unwrap()
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label, label_style),
    ]
}

fn icon_span(icon: &'static str) -> Span<'static> {
    Span::styled(
        icon,
        Style::default()
            .fg(theme::highlight())
            .add_modifier(Modifier::BOLD),
    )
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let footer_line = Line::from(vec![
        Span::styled(
            "  Up/Down or j/k",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Move  |  ", theme::dim()),
        Span::styled(
            "Space/Right/l",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Next  |  ", theme::dim()),
        Span::styled(
            "Left/h",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Previous  |  ", theme::dim()),
        Span::styled(
            "Esc/Comma",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Close  |  saved automatically", theme::dim()),
    ]);
    let footer = Paragraph::new(vec![Line::from(""), footer_line]).alignment(Alignment::Center);
    frame.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_output_label_defaults_to_default() {
        assert_eq!(
            audio_output_label(None),
            crate::audio::DEFAULT_OUTPUT_DEVICE_LABEL
        );
        assert_eq!(
            audio_output_label(Some("BlueZ Headphones")),
            "BlueZ Headphones"
        );
    }

    #[test]
    fn selected_row_uses_cursor() {
        assert_eq!(setting_row_cursor_symbol(true), " >  ");
        assert_eq!(setting_row_cursor_symbol(false), "    ");
    }

    #[test]
    fn setting_descriptions_cover_all_rows() {
        for row in SettingRow::ALL {
            assert!(!setting_description(row).is_empty());
        }
    }

    #[test]
    fn settings_overlay_rejects_tiny_area() {
        assert!(settings_area_is_compact(Rect::new(0, 0, 59, 18)));
        assert!(settings_area_is_compact(Rect::new(0, 0, 60, 17)));
    }

    #[test]
    fn settings_overlay_accepts_minimum_area() {
        assert!(!settings_area_is_compact(Rect::new(0, 0, 60, 18)));
    }
}
