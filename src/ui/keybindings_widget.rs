use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::keybindings::{format_key_description, format_mode_name, InputMode, KeyBinding};

use super::theme;

const OVERLAY_WIDTH_PCT: u16 = 65;
const OVERLAY_HEIGHT_PCT: u16 = 70;
const TITLE: &str = " Keybindings ";

/// Mode ordering for grouping bindings in the overlay.
const MODE_ORDER: &[InputMode] = &[
    InputMode::Normal,
    InputMode::Search,
    InputMode::CommandPalette,
    InputMode::SleepTimer,
    InputMode::LibraryFilter,
];

/// Render the keybindings overlay grouped by input mode.
pub fn render_keybindings_overlay(
    frame: &mut Frame,
    area: Rect,
    bindings: &[KeyBinding],
    scroll_offset: usize,
) {
    let popup_area = super::centered_rect(OVERLAY_WIDTH_PCT, OVERLAY_HEIGHT_PCT, area);
    frame.render_widget(Clear, popup_area);

    let block = overlay_block();
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let lines = build_grouped_lines(bindings);
    let visible_height = inner.height as usize;
    let items = build_visible_items(&lines, scroll_offset, visible_height);
    let list = List::new(items);
    frame.render_widget(list, inner);
}

fn overlay_block() -> Block<'static> {
    Block::default()
        .title(Span::styled(TITLE, theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear())
}

/// Build all display lines grouped by mode with section headers.
fn build_grouped_lines(bindings: &[KeyBinding]) -> Vec<DisplayLine> {
    let mut lines = Vec::new();
    for mode in MODE_ORDER {
        let mode_bindings: Vec<&KeyBinding> =
            bindings.iter().filter(|b| &b.mode == mode).collect();
        if mode_bindings.is_empty() {
            continue;
        }
        lines.push(DisplayLine::Header(format_mode_name(mode).to_string()));
        for binding in mode_bindings {
            lines.push(DisplayLine::Binding(format_binding_line(binding)));
        }
    }
    lines
}

/// Format a single binding as `"{key} [{modifiers}] → {action}"`.
fn format_binding_line(binding: &KeyBinding) -> String {
    let key_desc = format_key_description(&binding.key, &binding.modifiers);
    let action_name = action_display_name(&binding.action);
    format!("  {key_desc} → {action_name}")
}

/// Convert an Action to a human-readable display name using Debug format.
fn action_display_name(action: &crate::action::Action) -> String {
    format!("{:?}", action)
}

/// Build visible ListItem slice from display lines with scroll offset.
fn build_visible_items(
    lines: &[DisplayLine],
    offset: usize,
    visible_height: usize,
) -> Vec<ListItem<'static>> {
    lines
        .iter()
        .skip(offset)
        .take(visible_height)
        .map(|line| match line {
            DisplayLine::Header(text) => ListItem::new(Span::styled(
                format!("── {text} ──"),
                Style::default()
                    .fg(theme::highlight())
                    .add_modifier(Modifier::BOLD),
            )),
            DisplayLine::Binding(text) => {
                ListItem::new(Span::styled(text.clone(), theme::text()))
            }
        })
        .collect()
}

/// Internal representation of a line in the overlay.
enum DisplayLine {
    Header(String),
    Binding(String),
}

/// Total number of display lines for the given bindings (for scroll bounds).
#[allow(dead_code)]
pub fn total_lines(bindings: &[KeyBinding]) -> usize {
    build_grouped_lines(bindings).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::keybindings::{KeySpec, Modifier as KeyMod};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn render_keybindings_overlay_produces_non_empty_content() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let bindings = crate::keybindings::defaults::default_bindings();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_keybindings_overlay(frame, area, &bindings, 0);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buffer);
        assert!(content.contains("Keybindings"), "Title should appear");
        assert!(content.contains("Normal"), "Normal mode header should appear");
    }

    #[test]
    fn build_grouped_lines_groups_by_mode() {
        let bindings = vec![
            binding(KeySpec::Char('q'), vec![], Action::Quit, InputMode::Normal),
            binding(
                KeySpec::Char('j'),
                vec![],
                Action::NextStation,
                InputMode::Normal,
            ),
            binding(
                KeySpec::Named(crate::keybindings::NamedKey::Esc),
                vec![],
                Action::ExitSearch,
                InputMode::Search,
            ),
        ];

        let lines = build_grouped_lines(&bindings);

        // Should have: Normal header, 2 bindings, Search header, 1 binding = 5 lines
        assert_eq!(lines.len(), 5);
        assert!(matches!(&lines[0], DisplayLine::Header(h) if h == "Normal"));
        assert!(matches!(&lines[3], DisplayLine::Header(h) if h == "Search"));
    }

    #[test]
    fn format_binding_line_contains_key_and_action() {
        let binding = binding(
            KeySpec::Char('q'),
            vec![KeyMod::Ctrl],
            Action::Quit,
            InputMode::Normal,
        );

        let line = format_binding_line(&binding);

        assert!(line.contains("Ctrl+q"), "Should contain key description");
        assert!(line.contains("Quit"), "Should contain action name");
        assert!(line.contains("→"), "Should contain arrow separator");
    }

    #[test]
    fn total_lines_counts_headers_and_bindings() {
        let bindings = vec![
            binding(KeySpec::Char('q'), vec![], Action::Quit, InputMode::Normal),
            binding(
                KeySpec::Named(crate::keybindings::NamedKey::Esc),
                vec![],
                Action::ExitSearch,
                InputMode::Search,
            ),
        ];

        // Normal header + 1 binding + Search header + 1 binding = 4
        assert_eq!(total_lines(&bindings), 4);
    }

    #[test]
    fn empty_bindings_produce_no_lines() {
        assert_eq!(total_lines(&[]), 0);
    }

    #[test]
    fn build_visible_items_respects_scroll_offset() {
        let bindings = vec![
            binding(KeySpec::Char('a'), vec![], Action::Quit, InputMode::Normal),
            binding(KeySpec::Char('b'), vec![], Action::Stop, InputMode::Normal),
            binding(KeySpec::Char('c'), vec![], Action::VolumeUp, InputMode::Normal),
        ];

        let lines = build_grouped_lines(&bindings);
        // Header + 3 bindings = 4 lines total
        assert_eq!(lines.len(), 4);

        let items = build_visible_items(&lines, 2, 2);
        assert_eq!(items.len(), 2);
    }

    // -- Helpers --

    fn binding(
        key: KeySpec,
        modifiers: Vec<KeyMod>,
        action: Action,
        mode: InputMode,
    ) -> KeyBinding {
        KeyBinding {
            key,
            modifiers,
            action,
            mode,
        }
    }

    fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
        let area = buf.area;
        let mut output = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                output.push_str(buf.cell((x, y)).map_or("", |c| c.symbol()));
            }
        }
        output
    }
}
