use super::theme;
use crate::app::{App, LayoutMode, PlaybackState, RecordingState};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

const CASSETTE_INNER_WIDTH: usize = 44;
#[cfg(test)]
const CASSETTE_REEL_CELL_WIDTH: usize = 10;
const CASSETTE_TAPE_WIDTH: usize = 4;
const CASSETTE_HEIGHT: u16 = 9;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let title_text = match app.active_deck_page {
        0 => " 📼 Tape Deck ",
        _ => " 📼 Tape History ",
    };

    // Outer block with desaturated deep purple border and custom retro neon title
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(title_text, theme::title()));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    match app.active_deck_page {
        0 => {
            let full_deck = app.layout_mode == LayoutMode::RightOnly;

            // Split the inner area vertically: stable cassette, status strip, framed visualizer.
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(CASSETTE_HEIGHT),
                    Constraint::Length(5),
                    Constraint::Min(0),
                ])
                .split(inner_area);

            render_cassette(frame, chunks[0], app);
            render_meta_details(frame, chunks[1], app, full_deck);
            render_oscilloscope(frame, chunks[2], app);
        }
        _ => {
            render_history(frame, inner_area, app);
        }
    }
}

fn render_cassette(frame: &mut Frame, area: Rect, app: &App) {
    let lines = build_cassette_lines(
        CASSETTE_INNER_WIDTH,
        app.tick_count,
        &app.playback,
        app.recording_state,
    );

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

#[derive(Debug, Clone)]
struct CassetteSegment {
    text: String,
    style: Style,
}

fn build_cassette_lines(
    inner_width: usize,
    tick_count: u64,
    playback: &PlaybackState,
    recording_state: RecordingState,
) -> Vec<Line<'static>> {
    let shell_style = theme::dim();
    let reel_style = Style::default()
        .fg(theme::highlight())
        .bg(theme::bg())
        .add_modifier(Modifier::BOLD);

    let mut lines = Vec::with_capacity(CASSETTE_HEIGHT as usize);
    lines.push(cassette_border_line("╭", "╮", inner_width, shell_style));
    lines.push(cassette_label_line(
        inner_width,
        tick_count,
        recording_state,
        shell_style,
    ));
    lines.push(shell_text_line(
        "        ────────────────────────────        ",
        inner_width,
        shell_style,
        shell_style,
    ));
    lines.push(shell_text_line(
        "  ╭────────╮        ════        ╭────────╮  ",
        inner_width,
        shell_style,
        shell_style,
    ));

    let (left_reel, right_reel) = reel_cells_for_state(tick_count, playback);
    lines.push(shell_line(
        inner_width,
        shell_style,
        vec![
            segment("  ", shell_style),
            segment(left_reel, reel_style),
            segment("  ────────────────  ", shell_style),
            segment(right_reel, reel_style),
            segment("  ", shell_style),
        ],
    ));

    lines.push(shell_text_line(
        "  ╰────────╯                    ╰────────╯  ",
        inner_width,
        shell_style,
        shell_style,
    ));
    lines.push(shell_text_line(
        "                                            ",
        inner_width,
        shell_style,
        shell_style,
    ));
    lines.push(shell_text_line(
        "       ╲____________________________╱       ",
        inner_width,
        shell_style,
        shell_style,
    ));
    lines.push(cassette_border_line("╰", "╯", inner_width, shell_style));

    lines
}

fn cassette_label_line(
    inner_width: usize,
    tick_count: u64,
    recording_state: RecordingState,
    shell_style: Style,
) -> Line<'static> {
    let brand = "P U L S E  D E C K";
    let label_style = Style::default()
        .fg(theme::accent_secondary())
        .bg(theme::bg())
        .add_modifier(Modifier::BOLD);

    let (status, status_style) = cassette_recording_status(tick_count, recording_state);
    let fixed_padding = 4;
    let spacer_width = inner_width
        .saturating_sub(visible_len(brand))
        .saturating_sub(visible_len(status))
        .saturating_sub(fixed_padding);

    shell_line(
        inner_width,
        shell_style,
        vec![
            segment("  ", shell_style),
            segment(brand, label_style),
            segment(" ".repeat(spacer_width), shell_style),
            segment(status, status_style),
            segment("  ", shell_style),
        ],
    )
}

fn cassette_recording_status(
    tick_count: u64,
    recording_state: RecordingState,
) -> (&'static str, Style) {
    match recording_state {
        RecordingState::Active => {
            let status = if tick_count.is_multiple_of(2) {
                "● REC ACTIVE"
            } else {
                "  REC ACTIVE"
            };
            (status, theme::error().add_modifier(Modifier::BOLD))
        }
        RecordingState::Pending => {
            let status = if tick_count.is_multiple_of(2) {
                "● REC PENDING"
            } else {
                "  REC PENDING"
            };
            (
                status,
                Style::default()
                    .fg(theme::warm())
                    .bg(theme::bg())
                    .add_modifier(Modifier::BOLD),
            )
        }
        RecordingState::Off => (
            "A-SIDE",
            Style::default()
                .fg(theme::highlight())
                .bg(theme::bg())
                .add_modifier(Modifier::BOLD),
        ),
    }
}

fn reel_cells_for_state(tick_count: u64, playback: &PlaybackState) -> (String, String) {
    match playback {
        PlaybackState::Playing | PlaybackState::FadingOut { .. } => {
            let transfer_step = ((tick_count / 6) % 8) as usize;
            let transfer = if transfer_step < 4 {
                transfer_step
            } else {
                7 - transfer_step
            };

            let left_fill = CASSETTE_TAPE_WIDTH.saturating_sub(transfer).max(1);
            let right_fill = (1 + transfer).min(CASSETTE_TAPE_WIDTH);

            (
                reel_cell("○", fixed_tape_mass(left_fill, CASSETTE_TAPE_WIDTH), "○"),
                reel_cell("○", fixed_tape_mass(right_fill, CASSETTE_TAPE_WIDTH), "○"),
            )
        }
        PlaybackState::Connecting => {
            let hub = if (tick_count / 4).is_multiple_of(2) {
                "◌"
            } else {
                "○"
            };
            let tape = fixed_tape_mass(1, CASSETTE_TAPE_WIDTH);
            (reel_cell(hub, tape.clone(), hub), reel_cell(hub, tape, hub))
        }
        PlaybackState::Paused => {
            let tape = fixed_tape_mass(2, CASSETTE_TAPE_WIDTH);
            (reel_cell("○", tape.clone(), "○"), reel_cell("○", tape, "○"))
        }
        PlaybackState::Error(_) => {
            let tape = fixed_tape_mass(0, CASSETTE_TAPE_WIDTH);
            (reel_cell("×", tape.clone(), "×"), reel_cell("×", tape, "×"))
        }
        PlaybackState::Stopped => {
            let tape = fixed_tape_mass(0, CASSETTE_TAPE_WIDTH);
            (reel_cell("○", tape.clone(), "○"), reel_cell("○", tape, "○"))
        }
    }
}

fn fixed_tape_mass(fill: usize, width: usize) -> String {
    let fill = fill.min(width);
    format!("{}{}", "█".repeat(fill), "░".repeat(width - fill))
}

fn reel_cell(left_hub: &str, tape: String, right_hub: &str) -> String {
    format!("│ {left_hub}{tape}{right_hub} │")
}

fn cassette_border_line(
    left_corner: &str,
    right_corner: &str,
    inner_width: usize,
    style: Style,
) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("{left_corner}{}{right_corner}", "─".repeat(inner_width)),
        style,
    )])
}

fn shell_text_line(
    text: impl Into<String>,
    inner_width: usize,
    shell_style: Style,
    content_style: Style,
) -> Line<'static> {
    shell_line(
        inner_width,
        shell_style,
        vec![segment(text.into(), content_style)],
    )
}

fn shell_line(
    inner_width: usize,
    shell_style: Style,
    parts: Vec<CassetteSegment>,
) -> Line<'static> {
    let mut spans = Vec::with_capacity(parts.len() + 3);
    let mut remaining = inner_width;

    spans.push(Span::styled("│", shell_style));

    for part in parts {
        if remaining == 0 {
            break;
        }

        let part_width = visible_len(&part.text);
        let text = if part_width > remaining {
            truncate_to_chars(&part.text, remaining)
        } else {
            part.text
        };
        let text_width = visible_len(&text);

        remaining = remaining.saturating_sub(text_width);
        spans.push(Span::styled(text, part.style));
    }

    if remaining > 0 {
        spans.push(Span::styled(" ".repeat(remaining), shell_style));
    }

    spans.push(Span::styled("│", shell_style));
    Line::from(spans)
}

fn segment(text: impl Into<String>, style: Style) -> CassetteSegment {
    CassetteSegment {
        text: text.into(),
        style,
    }
}

fn visible_len(text: &str) -> usize {
    text.chars().count()
}

fn truncate_to_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn render_meta_details(frame: &mut Frame, area: Rect, app: &App, full_deck: bool) {
    let (status_text, status_style) = match app.playback {
        PlaybackState::Playing => ("PLAYING", theme::playing()),
        PlaybackState::FadingOut { .. } => (
            "FADING...",
            Style::default()
                .fg(theme::warm())
                .add_modifier(Modifier::BOLD),
        ),
        PlaybackState::Connecting => (
            "TUNING...",
            Style::default()
                .fg(theme::warm())
                .add_modifier(Modifier::BOLD),
        ),
        PlaybackState::Paused => ("PAUSED", theme::neon()),
        PlaybackState::Error(_) => ("OFFLINE / ERROR", theme::error()),
        PlaybackState::Stopped => ("STOPPED", theme::dim()),
    };

    let station = app.now_playing();
    let genre = station.map(|s| s.genre.as_str()).unwrap_or("N/A");
    let country = station.map(|s| s.country.as_str()).unwrap_or("N/A");

    let filled = (app.buffer_percent / 10) as usize;
    let empty = 10 - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(" ▶ ", status_style),
        Span::styled(status_text, status_style),
        Span::styled("   GENRE ", theme::dim()),
        Span::styled(genre, theme::cyan()),
        Span::styled("   ORIGIN ", theme::dim()),
        Span::styled(country, theme::cyan()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" BUFFER ", theme::dim()),
        Span::styled(
            format!("[{}] ", bar),
            Style::default()
                .fg(theme::highlight())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{}% ", app.buffer_percent), theme::cyan()),
        Span::styled(format!("({}s)", app.buffer_seconds), theme::dim()),
    ]));

    if app.recording_state == RecordingState::Active {
        if let Some(ref filepath) = app.active_record_filepath {
            let filename = std::path::Path::new(filepath)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(filepath);
            lines.push(Line::from(vec![
                Span::styled(
                    " ● REC ACTIVE ",
                    theme::error().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("capture -> {}", filename), theme::dim()),
            ]));
        }
    }

    let title = if full_deck {
        " SIGNAL / TAPE STATUS "
    } else {
        " SIGNAL "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(title, theme::title()));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_oscilloscope(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 3 || area.height < 3 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(Span::styled(visualizer_title(app), theme::title()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    render_visualizer_signal(frame, inner, app);
}

fn visualizer_title(app: &App) -> &'static str {
    match app.visualizer_mode {
        0 => " RTA SPECTRUM ",
        1 => " REAL OSC ",
        _ => " SIM OSC ",
    }
}

fn should_render_spectrum_analyzer(playback: &PlaybackState, visualizer_mode: usize) -> bool {
    visualizer_mode == 0
        && matches!(
            playback,
            PlaybackState::Playing | PlaybackState::Connecting | PlaybackState::FadingOut { .. }
        )
}

fn visualizer_amplitude_gain(playback: &PlaybackState, volume: u8) -> f32 {
    match playback {
        PlaybackState::FadingOut { current_volume } => current_volume.clamp(0.0, 1.0),
        _ => volume as f32 / 100.0,
    }
}

fn render_visualizer_signal(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let height = area.height as usize;

    if height == 0 || width == 0 {
        return;
    }

    // Mode 0: Spectrum Analyzer (renders vertical block bars with neon tri-color gradient)
    if should_render_spectrum_analyzer(&app.playback, app.visualizer_mode) {
        render_spectrum_analyzer(frame, area, app);
        return;
    }

    let mut canvas = BrailleCanvas::new(width, height);
    match app.playback {
        PlaybackState::Playing | PlaybackState::FadingOut { .. } => match app.visualizer_mode {
            1 => {
                let pixel_width = width * 2;
                let pixel_height = height * 4;
                let center_y = pixel_height as f32 * 0.5;
                let amplitude = visualizer_amplitude_gain(&app.playback, app.volume)
                    * (pixel_height as f32 * 0.45);

                let mut samples = Vec::with_capacity(pixel_width);
                if let Ok(buf) = app.sample_buffer.lock() {
                    let n = buf.len();
                    if n >= pixel_width {
                        let start_idx = n - pixel_width;
                        samples.extend(buf.iter().skip(start_idx).take(pixel_width).copied());
                    } else {
                        samples.extend(vec![0.0; pixel_width - n]);
                        samples.extend(buf.iter().copied());
                    }
                } else {
                    samples.extend(vec![0.0; pixel_width]);
                }

                for (x, sample_val) in samples.iter().enumerate().take(pixel_width) {
                    let y_float = center_y - (sample_val * amplitude);
                    let y = y_float.clamp(0.0, (pixel_height - 1) as f32) as usize;
                    canvas.set_pixel(x, y);
                }
            }
            _ => {
                let pixel_width = width * 2;
                let pixel_height = height * 4;
                let center_y = pixel_height as f32 * 0.5;
                let amplitude = visualizer_amplitude_gain(&app.playback, app.volume)
                    * (pixel_height as f32 * 0.4);

                for x in 0..pixel_width {
                    let t = app.tick_count as f32 * 0.15;
                    let bass = (x as f32 * 0.05 + t).sin() * 0.6;
                    let mid = (x as f32 * 0.15 - t * 0.8).cos() * 0.3;
                    let high = (x as f32 * 0.45 + t * 2.0).sin() * 0.1;

                    let wave_sum = bass + mid + high;
                    let y_float = center_y + wave_sum * amplitude;
                    let y = y_float.clamp(0.0, (pixel_height - 1) as f32) as usize;
                    canvas.set_pixel(x, y);
                }
            }
        },
        PlaybackState::Connecting => {
            let pixel_width = width * 2;
            let pixel_height = height * 4;
            let center_y = pixel_height as f32 * 0.5;
            let amplitude = pixel_height as f32 * 0.2;

            for x in 0..pixel_width {
                let t = app.tick_count as f32 * 0.4;
                let carrier = (x as f32 * 0.3 + t).sin();
                let envelope = (x as f32 * 0.04 - t * 0.25).cos().abs();

                let y_float = center_y + carrier * envelope * amplitude;
                let y = y_float.clamp(0.0, (pixel_height - 1) as f32) as usize;
                canvas.set_pixel(x, y);
            }
        }
        PlaybackState::Paused => {
            let pixel_width = width * 2;
            let pixel_height = height * 4;
            let center_y = pixel_height as f32 * 0.5;
            let amplitude = pixel_height as f32 * 0.07;

            for x in 0..pixel_width {
                let t = app.tick_count as f32 * 0.05;
                let ripple = (x as f32 * 0.08 + t).cos();
                let y_float = center_y + ripple * amplitude;
                let y = y_float.clamp(0.0, (pixel_height - 1) as f32) as usize;
                canvas.set_pixel(x, y);
            }
        }
        _ => {}
    }

    let active_style = Style::default()
        .fg(theme::accent_secondary())
        .add_modifier(Modifier::BOLD);
    let lines = canvas.to_lines(active_style, theme::dim());

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn render_spectrum_analyzer(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let height = area.height as usize;
    let peaks = &app.visualizer_peaks;

    if peaks.is_empty() {
        frame.render_widget(Paragraph::new(Vec::<Line<'static>>::new()), area);
        return;
    }

    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        let y_factor = (height - 1 - row) as f32 / (height.max(2) - 1) as f32;
        let color = if y_factor < 0.35 {
            theme::highlight()
        } else if y_factor < 0.7 {
            theme::accent_secondary()
        } else {
            theme::warm()
        };
        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);

        for col in 0..width {
            let (band, is_spacer) = spectrum_column_slot(col, width, peaks.len());
            let val = if is_spacer { 0.0 } else { peaks[band] };
            let h = val * height.saturating_sub(1).max(1) as f32;
            let height_in_row = h - (height - 1 - row) as f32;

            let char_str = if height_in_row <= 0.0 {
                " "
            } else if height_in_row >= 1.0 {
                "█"
            } else {
                let level = (height_in_row * 8.0).round() as usize;
                let blocks = [" ", " ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
                blocks[level.min(8)]
            };

            spans.push(Span::styled(char_str, style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn spectrum_column_slot(col: usize, width: usize, bands: usize) -> (usize, bool) {
    if bands == 0 || width == 0 {
        return (0, false);
    }

    let band = (col * bands / width).min(bands - 1);
    let start = band * width / bands;
    let end = ((band + 1) * width / bands).min(width);
    let band_width = end.saturating_sub(start);
    let is_spacer = band_width >= 3 && col + 1 == end;

    (band, is_spacer)
}

/// A micro-pixel canvas helper that aggregates sub-pixels into Unicode Braille characters (U+2800 - U+28FF)
struct BrailleCanvas {
    width: usize,
    height: usize,
    grid: Vec<u8>,
}

impl BrailleCanvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: vec![0u8; width * height],
        }
    }

    fn set_pixel(&mut self, x: usize, y: usize) {
        let char_x = x / 2;
        let char_y = y / 4;

        if char_x >= self.width || char_y >= self.height {
            return;
        }

        let sub_x = x % 2;
        let sub_y = y % 4;

        // Unicode Braille dot matrix offsets:
        // Left Column: Dot 1 (1), Dot 2 (2), Dot 3 (4), Dot 7 (64)
        // Right Column: Dot 4 (8), Dot 5 (16), Dot 6 (32), Dot 8 (128)
        let bit = match (sub_x, sub_y) {
            (0, 0) => 1,
            (0, 1) => 2,
            (0, 2) => 4,
            (0, 3) => 64,
            (1, 0) => 8,
            (1, 1) => 16,
            (1, 2) => 32,
            (1, 3) => 128,
            _ => 0,
        };

        let idx = char_y * self.width + char_x;
        self.grid[idx] |= bit;
    }

    fn to_lines(&self, active_style: Style, dim_style: Style) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(self.height);
        let center_y = self.height / 2;

        for y in 0..self.height {
            let mut spans = Vec::with_capacity(self.width);
            for x in 0..self.width {
                let idx = y * self.width + x;
                let cell = self.grid[idx];

                if cell == 0 {
                    if y == center_y {
                        spans.push(Span::styled("⠤", dim_style)); // Dotted neon center grid line
                    } else {
                        spans.push(Span::raw(" "));
                    }
                } else {
                    let c = std::char::from_u32(0x2800 + cell as u32).unwrap_or(' ');
                    spans.push(Span::styled(c.to_string(), active_style));
                }
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

fn render_history(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();

    lines.push(Line::from(vec![Span::styled(
        "   📼 Captured Session Mixtape ",
        Style::default()
            .fg(theme::accent_secondary())
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![Span::styled(
        "   ════════════════════════════",
        theme::dim(),
    )]));
    lines.push(Line::from(""));

    if app.song_history.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "   [ No tracks captured yet ]",
            theme::dim(),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "   Music playback will record",
            theme::dim(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "   inline ICY metadata here...",
            theme::dim(),
        )]));
    } else {
        // Render song history in reverse (newest on top)
        let visible_rows = (area.height as usize).saturating_sub(4);

        for (idx, song) in app.song_history.iter().enumerate().rev().take(visible_rows) {
            let track_num = idx + 1;
            let track_tag = format!("   Track {:02}: ", track_num);

            lines.push(Line::from(vec![
                Span::styled(
                    track_tag,
                    Style::default()
                        .fg(theme::highlight())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(song.as_str(), theme::text()),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_width(line: &Line<'_>) -> usize {
        line.spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum()
    }

    #[test]
    fn cassette_rows_have_equal_width() {
        let expected_width = CASSETTE_INNER_WIDTH + 2;
        let lines = build_cassette_lines(
            CASSETTE_INNER_WIDTH,
            0,
            &PlaybackState::Playing,
            RecordingState::Off,
        );

        assert_eq!(lines.len(), CASSETTE_HEIGHT as usize);
        for line in lines {
            assert_eq!(line_width(&line), expected_width);
        }
    }

    #[test]
    fn reel_animation_keeps_constant_cell_width() {
        for tick_count in 0..96 {
            let (left_reel, right_reel) = reel_cells_for_state(tick_count, &PlaybackState::Playing);

            assert_eq!(left_reel.chars().count(), CASSETTE_REEL_CELL_WIDTH);
            assert_eq!(right_reel.chars().count(), CASSETTE_REEL_CELL_WIDTH);
        }
    }

    #[test]
    fn tape_mass_keeps_constant_width() {
        for fill in 0..=CASSETTE_TAPE_WIDTH + 2 {
            assert_eq!(
                fixed_tape_mass(fill, CASSETTE_TAPE_WIDTH).chars().count(),
                CASSETTE_TAPE_WIDTH
            );
        }
    }

    #[test]
    fn spectrum_renderer_stays_active_while_connecting() {
        assert!(should_render_spectrum_analyzer(&PlaybackState::Playing, 0));
        assert!(should_render_spectrum_analyzer(
            &PlaybackState::Connecting,
            0
        ));
        assert!(!should_render_spectrum_analyzer(&PlaybackState::Paused, 0));
        assert!(!should_render_spectrum_analyzer(
            &PlaybackState::Connecting,
            1
        ));
    }

    #[test]
    fn spectrum_renderer_stays_active_while_fading_out() {
        assert!(should_render_spectrum_analyzer(
            &PlaybackState::FadingOut {
                current_volume: 0.5,
            },
            0
        ));
    }

    #[test]
    fn fading_out_visualizer_gain_uses_audio_ramp_volume() {
        assert!((visualizer_amplitude_gain(&PlaybackState::Playing, 80) - 0.8).abs() < 0.001);
        assert!(
            (visualizer_amplitude_gain(
                &PlaybackState::FadingOut {
                    current_volume: 0.35,
                },
                80
            ) - 0.35)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn spectrum_column_slot_adds_spacer_when_band_is_wide() {
        let (band, spacer) = spectrum_column_slot(3, 160, 40);

        assert_eq!(band, 0);
        assert!(spacer);
    }

    #[test]
    fn spectrum_column_slot_maps_last_column_to_last_band() {
        let (band, _) = spectrum_column_slot(159, 160, 40);

        assert_eq!(band, 39);
    }
}
