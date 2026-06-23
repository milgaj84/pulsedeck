use crate::app::command_label;
use crate::ui::model::UiModel;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use super::theme;

const MIN_COMMAND_PALETTE_WIDTH: u16 = 52;
const MIN_COMMAND_PALETTE_HEIGHT: u16 = 9;
const MAX_VISIBLE_COMMANDS: usize = 7;

pub fn render(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let popup_area = super::centered_rect(58, 36, area);

    if command_palette_area_is_compact(popup_area) {
        frame.render_widget(Clear, popup_area);
        super::render_boundary_warning(
            frame,
            popup_area,
            "Command Palette Too Compact",
            format!(
                "Expand terminal or close palette (overlay: {}x{})",
                popup_area.width, popup_area.height
            ),
        );
        return;
    }

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(" ⌘ Command Palette ", theme::title()))
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear());
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    render_input(frame, chunks[0], &app.command_palette.query);
    render_commands(frame, chunks[1], app);
    render_hint(frame, chunks[2]);
}

fn render_input(frame: &mut Frame, area: Rect, query: &str) {
    let input = if query.is_empty() {
        Line::from(vec![
            Span::styled(": ", theme::cyan()),
            Span::styled("type a command", theme::dim()),
            Span::styled("█", Style::default().fg(theme::highlight())),
        ])
    } else {
        Line::from(vec![
            Span::styled(": ", theme::cyan()),
            Span::styled(query.to_string(), theme::text()),
            Span::styled("█", Style::default().fg(theme::highlight())),
        ])
    };

    frame.render_widget(Paragraph::new(vec![input]), area);
}

fn render_commands(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let commands = app.command_palette_commands();
    let selected = app
        .command_palette
        .selected
        .min(commands.len().saturating_sub(1));
    let visible = commands.iter().take(MAX_VISIBLE_COMMANDS);

    let items = if commands.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No matching commands",
            theme::dim(),
        )))]
    } else {
        visible
            .enumerate()
            .map(|(idx, command)| {
                let marker = if idx == selected { "▸ " } else { "  " };
                let style = if idx == selected {
                    theme::selected()
                } else {
                    theme::text()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(marker, theme::cyan()),
                    Span::styled(command_label(*command), style),
                ]))
            })
            .collect()
    };

    let list = List::new(items).highlight_symbol("");
    let mut state = ListState::default();
    if !commands.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_hint(frame: &mut Frame, area: Rect) {
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("↑/↓", theme::cyan()),
        Span::styled(" select  ", theme::dim()),
        Span::styled("Enter", theme::cyan()),
        Span::styled(" run  ", theme::dim()),
        Span::styled("Esc", theme::cyan()),
        Span::styled(" close", theme::dim()),
    ]));
    frame.render_widget(hint, area);
}

fn command_palette_area_is_compact(area: Rect) -> bool {
    area.width < MIN_COMMAND_PALETTE_WIDTH || area.height < MIN_COMMAND_PALETTE_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_palette_rejects_tiny_area() {
        assert!(command_palette_area_is_compact(Rect::new(0, 0, 40, 10)));
        assert!(command_palette_area_is_compact(Rect::new(0, 0, 60, 8)));
        assert!(!command_palette_area_is_compact(Rect::new(0, 0, 60, 10)));
    }
}
