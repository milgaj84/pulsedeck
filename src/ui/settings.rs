use super::critical;
use super::theme;
use super::theme::ThemeName;
use crate::app::{App, SettingRow};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    // Elegant config popup at 54% width, 64% height to fit all setting rows
    let popup_area = super::centered_rect(54, 64, area);

    // Clear background
    frame.render_widget(Clear, popup_area);

    // Beautiful rounded block with glowing highlight border
    let block = Block::default()
        .title(Span::styled(
            " ✦ PulseDeck Config Console ✦ ",
            theme::title(),
        ))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(theme::bg()));

    let inner_area = block.inner(popup_area);
    let (content_area, alert_area) = critical::split_overlay_alert_area(inner_area, &app.playback);
    frame.render_widget(block, popup_area);

    let constraints = std::iter::once(Constraint::Length(1))
        .chain(SettingRow::ALL.iter().map(|_| Constraint::Length(2)))
        .chain(std::iter::once(Constraint::Min(0)))
        .collect::<Vec<_>>();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(content_area);

    for (offset, row) in SettingRow::ALL.iter().copied().enumerate() {
        render_setting_row(frame, chunks[offset + 1], app, row);
    }

    render_footer(frame, chunks[SettingRow::COUNT + 1]);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.playback);
    }
}

fn render_setting_row(frame: &mut Frame, area: Rect, app: &App, row: SettingRow) {
    let is_selected = app.selected_setting_idx == row.index();
    let keep_snippets = app.library.settings.keep_snippets;
    let duration_dimmed = row == SettingRow::MinSongDuration && keep_snippets;

    let active_style = Style::default()
        .fg(theme::accent_secondary())
        .add_modifier(Modifier::BOLD);
    let normal_style = Style::default().fg(theme::text().fg.unwrap());
    let active_bg = Style::default().bg(theme::surface_color());

    let cursor_style = if duration_dimmed {
        theme::dim()
    } else {
        active_style
    };
    let label_style = if duration_dimmed {
        theme::dim()
    } else if is_selected {
        active_style
    } else {
        normal_style
    };

    let mut spans = vec![Span::styled(
        setting_row_cursor_symbol(is_selected, duration_dimmed),
        cursor_style,
    )];

    spans.extend(setting_row_spans(app, row, label_style));

    let mut paragraph = Paragraph::new(Line::from(spans));
    if is_selected {
        paragraph = paragraph.style(selected_setting_row_style(duration_dimmed, active_bg));
    }
    frame.render_widget(paragraph, area);
}

fn setting_row_cursor_symbol(is_selected: bool, is_disabled: bool) -> &'static str {
    match (is_selected, is_disabled) {
        (true, true) => " ◦  ",
        (true, false) => " ▸  ",
        (false, _) => "    ",
    }
}

fn selected_setting_row_style(is_disabled: bool, active_bg: Style) -> Style {
    if is_disabled {
        Style::default().bg(theme::bg())
    } else {
        active_bg
    }
}

fn setting_row_spans(app: &App, row: SettingRow, label_style: Style) -> Vec<Span<'static>> {
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
        SettingRow::RecordingDir => vec![
            icon_span("[ 🗁 ] "),
            Span::styled("Tape Capture Folder: ", label_style),
            Span::styled(
                format!("{} ", app.library.settings.recording_dir),
                Style::default()
                    .fg(theme::highlight())
                    .add_modifier(Modifier::UNDERLINED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("(Space/Right forward, Left back)", theme::dim()),
        ],
        SettingRow::KeepSnippets => checkbox_row(
            app.library.settings.keep_snippets,
            "Keep Partial Snippets & Commercial Ads",
            label_style,
        ),
        SettingRow::MinSongDuration => {
            let duration_dimmed = app.library.settings.keep_snippets;
            let duration_fg = if duration_dimmed {
                theme::dim().fg.unwrap()
            } else {
                theme::highlight()
            };
            vec![
                Span::styled(
                    "[ ⏱ ] ",
                    Style::default()
                        .fg(duration_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("Min Song Duration: ", label_style),
                Span::styled(
                    format!("{}s ", app.library.settings.min_song_duration_secs),
                    Style::default()
                        .fg(duration_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if duration_dimmed {
                        "(disabled — keeping all)"
                    } else {
                        "(Space/Right forward, Left back)"
                    },
                    theme::dim(),
                ),
            ]
        }
        SettingRow::Theme => {
            let current_theme = ThemeName::from_key(&app.library.settings.theme);
            vec![
                icon_span("[ 🎨 ] "),
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
    }
}

fn checkbox_row(enabled: bool, label: &'static str, label_style: Style) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            if enabled { "[ ▣ ] " } else { "[ ▢ ] " },
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
            "  j/k",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Navigate  •  ", theme::dim()),
        Span::styled(
            "Left/Right",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Back/Forward  •  ", theme::dim()),
        Span::styled(
            "h/l",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Back/Forward  •  ", theme::dim()),
        Span::styled(
            "Space",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Toggle / Forward  •  ", theme::dim()),
        Span::styled(
            "Esc/ ,",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Exit Config", theme::dim()),
    ]);
    let footer = Paragraph::new(vec![Line::from(""), footer_line]).alignment(Alignment::Center);
    frame.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_selected_row_uses_soft_cursor() {
        assert_eq!(setting_row_cursor_symbol(true, true), " ◦  ");
        assert_eq!(setting_row_cursor_symbol(true, false), " ▸  ");
        assert_eq!(setting_row_cursor_symbol(false, true), "    ");
        assert_eq!(setting_row_cursor_symbol(false, false), "    ");
    }

    #[test]
    fn disabled_selected_row_skips_active_background() {
        let active_bg = Style::default().bg(Color::Red);

        assert_eq!(selected_setting_row_style(false, active_bg), active_bg);

        let disabled_style = selected_setting_row_style(true, active_bg);
        assert_ne!(disabled_style, active_bg);
        assert_eq!(disabled_style.bg, Some(theme::bg()));
    }
}
