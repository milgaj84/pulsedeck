use crate::app::{App, LayoutMode};
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders};

mod cassette;
mod meta;
mod visualizer;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(" 📡 Signal Deck ", theme::title()));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let full_deck = app.layout_mode == LayoutMode::RightOnly;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(cassette::DECK_ART_HEIGHT),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(inner_area);

    cassette::render_cassette(frame, chunks[0], app);
    meta::render_meta_details(frame, chunks[1], app, full_deck);
    visualizer::render_oscilloscope(frame, chunks[2], app);
}
