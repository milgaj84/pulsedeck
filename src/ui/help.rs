use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Clear, Row, Table};

use super::{critical, theme};

const MIN_HELP_WIDTH: u16 = 60;
const MIN_HELP_HEIGHT: u16 = 14;

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    // Keep the help compact so it still fits when terminal fonts are large.
    let popup_area = super::centered_rect(70, 60, area);

    if help_area_is_compact(popup_area) {
        frame.render_widget(Clear, popup_area);
        super::render_boundary_warning(
            frame,
            popup_area,
            "Controls Overlay Too Compact",
            format!(
                "Expand terminal or close help (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ✦ PulseDeck Controls ✦ ", theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear());

    let inner_area = block.inner(popup_area);
    let (content_area, alert_area) =
        critical::split_overlay_alert_area(inner_area, &app.player.state);

    let header_row = Row::new(vec![
        Cell::from(Span::styled(
            "Key",
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled(
            "Action",
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )),
    ]);

    let rows = vec![
        section("Playback"),
        shortcut(
            "Enter",
            "Play selected station; in search: save to Library + play",
        ),
        shortcut("Space", "Pause / resume; in search: preview without saving"),
        shortcut("s", "Stop playback"),
        shortcut("r", "Retry the current stream after an error"),
        shortcut("+ / -", "Volume up / down"),
        shortcut("m", "Mute / unmute"),
        section("Library"),
        shortcut("Up/Down / j k", "Move selection"),
        shortcut("Tab / Shift+Tab", "Change genre category"),
        shortcut("e", "Export Library to M3U format"),
        shortcut("f", "Remove selected station from Library"),
        shortcut("u", "Undo the most recent station removal"),
        shortcut("i", "Show selected station details"),
        shortcut("d", "Open Playback Doctor diagnostics"),
        section("Search"),
        shortcut("/ / Ctrl+f / F3", "Open worldwide station search"),
        shortcut(": / Ctrl+p", "Open command palette"),
        shortcut("Type", "Search by station, tag, city, or country"),
        shortcut("Prefixes", "tag:ambient / country:BA / lang:english / codec:mp3"),
        shortcut("Aliases", "genre: / cc: / language: / format: / station:"),
        shortcut("Space", "Audition highlighted result without saving"),
        shortcut("Ctrl+Enter", "Audition too, when your terminal supports it"),
        shortcut("Enter", "Save highlighted result to Library and play it"),
        shortcut("Esc", "Leave search without adding"),
        shortcut("Ctrl/Alt +/-/m", "Volume/mute while staying in search"),
        section("Visuals & Context"),
        shortcut("b", "Cycle Split / Library Focus / Signal Focus view"),
        shortcut("v", "Cycle RTA / Real Osc / Sim Osc visualizer"),
        shortcut("g", "Show recent stream-provided track titles"),
        section("Settings"),
        shortcut(",", "Settings: output, theme, autoplay, metadata, history"),
        shortcut("Up/Down / j k", "Move setting selection"),
        shortcut("Space / Right / l", "Advance highlighted setting"),
        shortcut("Left / h", "Step highlighted setting back"),
        shortcut("Audio Output", "Choose Default, pulse, pipewire, or device"),
        shortcut("Metadata", "Toggle ICY now-playing metadata requests"),
        section("Sleep Timer"),
        shortcut("t", "Open / close the sleep timer panel"),
        shortcut("Up / + , Down / -", "Add or subtract 5 minutes"),
        shortcut("1 - 6", "Jump to 15 / 30 / 45 / 60 / 90 / 120 min"),
        shortcut("0 / c", "Turn the sleep timer off"),
        section("App"),
        shortcut("h / ?", "Show / hide this help"),
        shortcut("q / Esc", "Quit, or close overlay first"),
    ];

    let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
    let table = Table::new(rows, widths).header(header_row);

    frame.render_widget(block, popup_area);
    frame.render_widget(table, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.player.state);
    }
}

fn help_area_is_compact(area: Rect) -> bool {
    area.width < MIN_HELP_WIDTH || area.height < MIN_HELP_HEIGHT
}

fn section(label: &'static str) -> Row<'static> {
    Row::new(vec![
        Cell::from(Span::styled(
            format!("▸ {label}"),
            Style::default()
                .fg(theme::dim().fg.unwrap())
                .add_modifier(Modifier::UNDERLINED),
        )),
        Cell::from(""),
    ])
}

fn shortcut(key: &'static str, action: &'static str) -> Row<'static> {
    Row::new(vec![
        Cell::from(Span::styled(key, Style::default().fg(theme::highlight()))),
        Cell::from(Span::styled(action, theme::text())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_overlay_rejects_tiny_area() {
        assert!(help_area_is_compact(Rect::new(0, 0, 59, 14)));
        assert!(help_area_is_compact(Rect::new(0, 0, 60, 13)));
    }

    #[test]
    fn help_overlay_accepts_minimum_area() {
        assert!(!help_area_is_compact(Rect::new(0, 0, 60, 14)));
    }
}
