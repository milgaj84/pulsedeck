use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Clear, Row, Table, Tabs};

use super::{critical, theme};

const MIN_HELP_WIDTH: u16 = 60;
const MIN_HELP_HEIGHT: u16 = 14;

/// Help tab categories.
const TAB_LABELS: &[&str] = &[
    " Playback ",
    " Library ",
    " Search ",
    " Visuals ",
    " Settings ",
    " App ",
];

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let popup_area = super::centered_rect(75, 70, area);

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
    frame.render_widget(block, popup_area);

    let (content_area, alert_area) =
        critical::split_overlay_alert_area(inner_area, &app.player.state);

    // Split content into tabs header + table body.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(content_area);

    let tab_area = chunks[0];
    let table_area = chunks[1];

    // Determine active tab from tick count (user cycles with Tab inside help,
    // but for now we use tick_count / 300 to auto-rotate — or just default to 0).
    let active_tab = help_tab_index(app);

    let tabs = Tabs::new(TAB_LABELS.iter().map(|t| Span::raw(*t)).collect::<Vec<_>>())
        .select(active_tab)
        .style(theme::dim())
        .highlight_style(
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled("│", theme::dim()));

    frame.render_widget(tabs, tab_area);

    let rows = rows_for_tab(active_tab);
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

    let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
    let table = Table::new(rows, widths).header(header_row);
    frame.render_widget(table, table_area);

    if let Some(alert_area) = alert_area {
        critical::render_engine_fault_banner(frame, alert_area, &app.player.state);
    }
}

fn help_tab_index(app: &UiModel<'_>) -> usize {
    // Help tab is stored in the navigation help_tab field.
    // For simplicity, use nav.library_filter_genre_snapshot as a proxy
    // that doesn't collide in Help mode — or just show tab 0.
    // We'll use a simple approach: the help tab index is stored nowhere yet,
    // so default to tab 0 (Playback). Users see all shortcuts via Tab/BackTab
    // cycling which we'll handle via the help_tab field.
    app.nav.help_tab_index
}

fn rows_for_tab(tab: usize) -> Vec<Row<'static>> {
    match tab {
        0 => playback_rows(),
        1 => library_rows(),
        2 => search_rows(),
        3 => visuals_rows(),
        4 => settings_rows(),
        5 => app_rows(),
        _ => playback_rows(),
    }
}

fn playback_rows() -> Vec<Row<'static>> {
    vec![
        shortcut("Enter", "Play selected station"),
        shortcut("Space", "Pause / resume playback"),
        shortcut("s", "Stop playback"),
        shortcut("r", "Retry current stream after error"),
        shortcut("+ / -", "Volume up / down"),
        shortcut("m", "Mute / unmute"),
        shortcut("Alt+1–5", "Play station from preset slot 1–5"),
        shortcut("Ctrl+1–5", "Assign playing station to slot 1–5"),
        shortcut("t", "Open sleep timer panel"),
    ]
}

fn library_rows() -> Vec<Row<'static>> {
    vec![
        shortcut("Up/Down · j/k", "Move selection"),
        shortcut("Tab / Shift+Tab", "Change genre category"),
        shortcut("Ctrl+l", "Library filter (fuzzy search saved stations)"),
        shortcut("*", "Toggle favorite (★ pin to top)"),
        shortcut("{n}G / {n}Enter", "Jump to row number (vim-style)"),
        shortcut("f", "Remove selected station"),
        shortcut("u", "Undo most recent removal"),
        shortcut("i", "Station details"),
        shortcut("d", "Playback Doctor diagnostics"),
        shortcut("g", "Recent tracks / listening history"),
        shortcut("e", "Export Library to M3U"),
    ]
}

fn search_rows() -> Vec<Row<'static>> {
    vec![
        shortcut("/ · Ctrl+f · F3", "Open worldwide station search"),
        shortcut(": · Ctrl+p", "Open command palette"),
        shortcut("Type", "Search by name, tag, city, or country"),
        shortcut("Prefixes", "tag: · country: · lang: · codec:"),
        shortcut("Aliases", "genre: · cc: · language: · format: · station:"),
        shortcut("Space", "Audition highlighted result (no save)"),
        shortcut("Ctrl+Enter", "Audition (alt, if terminal supports it)"),
        shortcut("Enter", "Save + play highlighted result"),
        shortcut("Esc", "Leave search without saving"),
        shortcut("Ctrl/Alt +/-/m", "Volume/mute while in search"),
    ]
}

fn visuals_rows() -> Vec<Row<'static>> {
    vec![
        shortcut("b", "Cycle: Split / Library Focus / Signal Focus"),
        shortcut("v", "Cycle: RTA / Real Osc / Sim Osc"),
        shortcut("g", "Recent stream-provided track titles"),
    ]
}

fn settings_rows() -> Vec<Row<'static>> {
    vec![
        shortcut(",", "Open settings panel"),
        shortcut("Up/Down · j/k", "Move setting selection"),
        shortcut("Space / Right / l", "Advance highlighted setting"),
        shortcut("Left / h", "Step highlighted setting back"),
        section("Options"),
        shortcut("Notifications", "Desktop track-change notifications"),
        shortcut("Auto-resume", "Play last station on startup"),
        shortcut("Song History", "Persist track history to disk"),
        shortcut("Audio Output", "Default, pulse, pipewire, or device"),
        shortcut("Theme", "Retrowave, Catppuccin ×4, Terminal"),
        shortcut("Metadata", "Toggle ICY now-playing metadata"),
    ]
}

fn app_rows() -> Vec<Row<'static>> {
    vec![
        shortcut("h / ?", "Show / hide this help"),
        shortcut("Tab / Shift+Tab", "Switch help tab"),
        shortcut("q / Esc", "Quit, or close overlay first"),
        section("Sleep Timer (inside panel)"),
        shortcut("Up / + , Down / -", "Add or subtract 5 minutes"),
        shortcut("1–6", "Jump to 15 / 30 / 45 / 60 / 90 / 120 min"),
        shortcut("0 / c", "Turn sleep timer off"),
    ]
}

fn section(label: &'static str) -> Row<'static> {
    Row::new(vec![
        Cell::from(Span::styled(
            format!("▸ {label}"),
            Style::default()
                .fg(theme::dim().fg.unwrap_or_default())
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

fn help_area_is_compact(area: Rect) -> bool {
    area.width < MIN_HELP_WIDTH || area.height < MIN_HELP_HEIGHT
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

    #[test]
    fn rows_for_each_tab_are_non_empty() {
        for tab in 0..6 {
            assert!(!rows_for_tab(tab).is_empty());
        }
    }
}
