pub mod controls;
pub mod critical;
pub mod deck;
pub mod header;
pub mod help;
pub mod search;
pub mod settings;
pub mod stations;
pub mod theme;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::{App, InputMode, LayoutMode};

/// Render the entire UI. Root layout composition.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Fill background with pure black
    let bg = Block::default().style(Style::default().bg(theme::bg()));
    frame.render_widget(bg, size);

    let is_searching = app.input_mode == InputMode::Search;

    // Main vertical layout: header | separator | main content split | separator | controls
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Length(1), // separator
            Constraint::Min(5),    // main content split
            Constraint::Length(1), // separator
            Constraint::Length(2), // controls (status + keybinds)
        ])
        .split(size);

    header::render(frame, chunks[0], app);
    render_separator(frame, chunks[1]);

    // Render main content area depending on active LayoutMode
    match app.layout_mode {
        LayoutMode::Split => {
            let content_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(chunks[2]);

            let left_area = content_chunks[0];
            let right_area = content_chunks[1];

            // Render Right Column: Tape Deck / History
            deck::render(frame, right_area, app);

            // Render Left Column: Search Bar (if searching) + Station List
            if is_searching {
                let left_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(0)])
                    .split(left_area);
                search::render(frame, left_chunks[0], app);
                stations::render(frame, left_chunks[1], app);
            } else {
                stations::render(frame, left_area, app);
            }
        }
        LayoutMode::LeftOnly => {
            // Render Left Column across the entire content split area (Closed Bento)
            if is_searching {
                let left_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(0)])
                    .split(chunks[2]);
                search::render(frame, left_chunks[0], app);
                stations::render(frame, left_chunks[1], app);
            } else {
                stations::render(frame, chunks[2], app);
            }
        }
        LayoutMode::RightOnly => {
            // Render Bento Tape Deck across the entire content split area (Full Bento)
            deck::render(frame, chunks[2], app);
        }
    }

    render_separator(frame, chunks[3]);
    controls::render(frame, chunks[4], app);

    // Draw help modal floating overlay if enabled
    if app.show_help {
        help::render(frame, size, app);
    }

    // Draw settings modal floating overlay if enabled
    if app.show_settings {
        settings::render(frame, size, app);
    }
}

/// Render a horizontal neon separator line.
fn render_separator(frame: &mut Frame, area: Rect) {
    let width = area.width as usize;
    let line_str = "═".repeat(width);
    let sep = Paragraph::new(Line::from(Span::styled(
        line_str,
        Style::default().fg(theme::accent()).bg(theme::bg()),
    )));
    frame.render_widget(sep, area);
}

/// Centered rect generation helper shared by overlays (Help, Settings).
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
