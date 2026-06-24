pub mod command_palette;
pub mod controls;
pub mod critical;
pub mod deck;
pub mod header;
pub mod help;
pub mod mini;
pub mod model;
pub mod playback_doctor;
pub mod recent_tracks;
pub mod search;
pub mod settings;
pub mod sleep_timer;
pub mod station_details;
pub mod stations;
pub mod theme;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{ActiveOverlay, App, DisplayMode, InputMode, LayoutMode};
use model::UiModel;

const MIN_REQUIRED_WIDTH: u16 = 80;
const MIN_REQUIRED_HEIGHT: u16 = 24;

/// Render the entire UI. Root layout composition.
pub fn draw(frame: &mut Frame, app: &App) {
    let model = UiModel::from(app);
    draw_model(frame, &model);
}

fn draw_model(frame: &mut Frame, app: &UiModel<'_>) {
    let size = frame.area();

    // Fill background with the active theme before any layout work.
    let bg = Block::default().style(theme::clear());
    frame.render_widget(bg, size);

    if app.display_mode == DisplayMode::Mini {
        mini::render(frame, size, app);
        return;
    }

    if is_compact_terminal(size) {
        render_compact_terminal_warning(frame, size);
        return;
    }

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

            // Render Right Column: Signal Deck
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
            // Render Signal Deck across the entire content split area (Full Bento)
            deck::render(frame, chunks[2], app);
        }
    }

    render_separator(frame, chunks[3]);
    controls::render(frame, chunks[4], app);

    match app.overlays.active {
        ActiveOverlay::StationDetails => station_details::render(frame, size, app),
        ActiveOverlay::RecentTracks => recent_tracks::render(frame, size, app),
        ActiveOverlay::Help => help::render(frame, size, app),
        ActiveOverlay::Settings => settings::render(frame, size, app),
        ActiveOverlay::PlaybackDoctor => playback_doctor::render(frame, size, app),
        ActiveOverlay::SleepTimer => sleep_timer::render(frame, size, app),
        ActiveOverlay::None => {}
    }

    if app.input_mode == InputMode::CommandPalette {
        command_palette::render(frame, size, app);
    }
}

pub fn is_compact_terminal(size: Rect) -> bool {
    size.width < MIN_REQUIRED_WIDTH || size.height < MIN_REQUIRED_HEIGHT
}

fn render_compact_terminal_warning(frame: &mut Frame, area: Rect) {
    render_boundary_warning(
        frame,
        area,
        "Console Screen Too Compact",
        format!(
            "Expand terminal to at least {}x{} (current: {}x{})",
            MIN_REQUIRED_WIDTH, MIN_REQUIRED_HEIGHT, area.width, area.height
        ),
    );
}

pub fn render_boundary_warning(frame: &mut Frame, area: Rect, title: &str, detail: String) {
    let diagnostic_area = centered_rect(90, 40, area);
    let warning = Paragraph::new(vec![
        Line::from(Span::styled(title.to_string(), theme::title())),
        Line::from(""),
        Line::from(Span::styled(detail, theme::dim())),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::border())
            .border_type(ratatui::widgets::BorderType::Rounded)
            .style(theme::clear()),
    );

    frame.render_widget(warning, diagnostic_area);
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

/// Shared overlay chrome rendering: centered popup with title, rounded border,
/// and background clear. Returns the inner area for content rendering.
///
/// Use this in overlay renderers to eliminate boilerplate:
/// ```ignore
/// let inner = render_overlay_chrome(frame, area, " Title ", 70, 60);
/// // render content into `inner`
/// ```
#[allow(dead_code)]
pub fn render_overlay_chrome(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    width_pct: u16,
    height_pct: u16,
) -> Rect {
    use ratatui::widgets::Clear;

    let popup_area = centered_rect(width_pct, height_pct, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Span::styled(title, theme::title()))
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(theme::accent_secondary())
                .add_modifier(Modifier::BOLD),
        )
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(theme::clear());

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_terminal_rejects_width_below_minimum() {
        assert!(is_compact_terminal(Rect::new(0, 0, 79, 24)));
    }

    #[test]
    fn compact_terminal_rejects_height_below_minimum() {
        assert!(is_compact_terminal(Rect::new(0, 0, 80, 23)));
    }

    #[test]
    fn compact_terminal_accepts_exact_minimum() {
        assert!(!is_compact_terminal(Rect::new(0, 0, 80, 24)));
    }

    #[test]
    fn compact_terminal_accepts_larger_terminal() {
        assert!(!is_compact_terminal(Rect::new(0, 0, 120, 40)));
    }
}
