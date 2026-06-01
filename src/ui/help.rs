use crate::app::App;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Clear, Row, Table};

use super::{critical, theme};

const MIN_HELP_WIDTH: u16 = 60;
const MIN_HELP_HEIGHT: u16 = 14;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
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
    let (content_area, alert_area) = critical::split_overlay_alert_area(inner_area, &app.playback);

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
        shortcut("Space", "Pause / resume"),
        shortcut("s", "Stop playback"),
        shortcut("+ / -", "Volume up / down"),
        shortcut("m", "Mute / unmute"),
        shortcut("r", "Arm / stop tape recording"),
        section("Library"),
        shortcut("↑↓ / j k", "Move selection"),
        shortcut("Tab / Shift+Tab", "Change genre category"),
        shortcut("f", "Remove selected station from Library"),
        shortcut("u", "Undo the most recent station removal"),
        section("Search"),
        shortcut("/", "Open worldwide station search"),
        shortcut("Type", "Search by station, tag, city, or country"),
        shortcut("Space", "Audition highlighted result without saving"),
        shortcut("Ctrl+Enter", "Audition too, when your terminal supports it"),
        shortcut("Enter", "Save highlighted result to Library and play it"),
        shortcut("Esc", "Leave search without adding"),
        shortcut("Ctrl/Alt +/-/m", "Volume/mute while staying in search"),
        section("Deck & Visuals"),
        shortcut("b", "Cycle Split / Library / Deck layout"),
        shortcut("p", "Switch Tape Deck / Local Tape Library"),
        shortcut("v", "Cycle RTA Spectrum / Real Osc / Sim Osc"),
        section("Local Tape Library"),
        shortcut("Enter", "Play selected local tape, or open selected archive row"),
        shortcut("Space", "Expand/collapse folders; pause/resume selected tape"),
        shortcut("Ctrl+r", "Refresh recorded tape archive"),
        shortcut("f / Delete", "Request delete for selected local tape"),
        shortcut("y / n", "Confirm or cancel pending tape delete"),
        section("Settings"),
        shortcut(",", "Open settings"),
        shortcut("↑↓ / j k", "Move setting selection"),
        shortcut("Space / → / l", "Advance highlighted setting"),
        shortcut("← / h", "Step highlighted setting back"),
        shortcut("Audio Output", "Choose Default, pulse, pipewire, or device"),
        section("App"),
        shortcut("h / ?", "Show / hide this help"),
        shortcut("q / Esc", "Quit, or close overlay first"),
    ];

    let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
    let table = Table::new(rows, widths).header(header_row);

    frame.render_widget(block, popup_area);
    frame.render_widget(table, content_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.playback);
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
