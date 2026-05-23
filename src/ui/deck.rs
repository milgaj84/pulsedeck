use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use crate::app::{App, PlaybackState};
use super::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let title_text = match app.active_deck_page {
        0 => " 📼 Tape Deck ",
        _ => " 🪨 Sediment History ",
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
            // Split the inner area vertically: Top (Cassette Art), Middle (Details), Bottom (Scope)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6), // Cassette Art
                    Constraint::Length(5), // Meta Details
                    Constraint::Min(0),    // Oscilloscope Simulated Visualizer
                ])
                .split(inner_area);

            render_cassette(frame, chunks[0], app);
            render_meta_details(frame, chunks[1], app);
            render_oscilloscope(frame, chunks[2], app);
        }
        _ => {
            render_history(frame, inner_area, app);
        }
    }
}

#[allow(clippy::manual_is_multiple_of)]
fn render_cassette(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();

    // 1. Spinning wheel character
    let spin_char = match app.playback {
        PlaybackState::Playing => {
            let frame_idx = (app.tick_count / 2) % 4;
            match frame_idx {
                0 => "/",
                1 => "-",
                2 => "\\",
                _ => "|",
            }
        }
        PlaybackState::Connecting => {
            if (app.tick_count / 4) % 2 == 0 {
                "o"
            } else {
                " "
            }
        }
        PlaybackState::Error(_) => "x",
        _ => "o",
    };

    // 2. Dynamic Tape supply/take-up transfer brackets (always exactly 9 chars each)
    let (l_bra, r_bra) = match app.playback {
        PlaybackState::Playing => {
            let step = (app.tick_count / 4) % 11;
            let mut left_size = 9 - (step as usize / 2);
            let mut right_size = 3 + (step as usize / 2);
            left_size = left_size.clamp(2, 9);
            right_size = right_size.clamp(2, 9);

            (
                format!(" (( {} )) ", "█".repeat(left_size)),
                format!(" (( {} )) ", "█".repeat(right_size)),
            )
        }
        _ => {
            (
                format!(" (( {} )) ", spin_char),
                format!(" (( {} )) ", spin_char),
            )
        }
    };

    let cassette_color = theme::dim();
    let label_style = Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD);
    let reel_style = Style::default().fg(theme::highlight()).add_modifier(Modifier::BOLD);

    lines.push(Line::from(vec![
        Span::styled("   ┌───────────────────────────────┐", cassette_color),
    ]));

    // Compact smart cassette capture status flashes inside the tape label!
    let label_spans = match app.recording_state {
        crate::app::RecordingState::Active => {
            let flash = (app.tick_count % 2) == 0;
            vec![
                Span::styled(if flash { " ● " } else { "   " }, Style::default().fg(theme::error().fg.unwrap_or_default()).add_modifier(Modifier::BOLD)),
                Span::styled("REC [ACTIVE]  ", Style::default().fg(theme::error().fg.unwrap_or_default()).add_modifier(Modifier::BOLD)),
            ]
        }
        crate::app::RecordingState::Pending => {
            let flash = (app.tick_count % 2) == 0;
            vec![
                Span::styled(if flash { " ● " } else { "   " }, Style::default().fg(theme::warm()).add_modifier(Modifier::BOLD)),
                Span::styled("PENDING...    ", Style::default().fg(theme::warm()).add_modifier(Modifier::BOLD)),
            ]
        }
        crate::app::RecordingState::Off => {
            vec![Span::styled("   D R I F T   F M   ", label_style)]
        }
    };

    let mut rec_line = vec![Span::styled("   │ ", cassette_color)];
    rec_line.extend(label_spans);
    rec_line.push(Span::styled("   │", cassette_color));
    lines.push(Line::from(rec_line));

    lines.push(Line::from(vec![
        Span::styled("   │  ___________________________  │", cassette_color),
    ]));
    lines.push(Line::from(vec![
        Span::styled("   │ /  ", cassette_color),
        Span::styled(l_bra, reel_style),
        Span::styled("             ", cassette_color),
        Span::styled(r_bra, reel_style),
        Span::styled("  \\ │", cassette_color),
    ]));
    lines.push(Line::from(vec![
        Span::styled("   │ \\___________________________/ │", cassette_color),
    ]));
    lines.push(Line::from(vec![
        Span::styled("   └───────────────────────────────┘", cassette_color),
    ]));

    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn render_meta_details(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();

    // Status string
    let (status_text, status_style) = match app.playback {
        PlaybackState::Playing => ("PLAYING", theme::playing()),
        PlaybackState::Connecting => ("TUNING...", Style::default().fg(theme::warm()).add_modifier(Modifier::BOLD)),
        PlaybackState::Paused => ("PAUSED", theme::neon()),
        PlaybackState::Error(_) => ("OFFLINE / ERROR", theme::error()),
        PlaybackState::Stopped => ("STOPPED", theme::dim()),
    };

    let station = app.now_playing();
    let genre = station.map(|s| s.genre.as_str()).unwrap_or("N/A");
    let country = station.map(|s| s.country.as_str()).unwrap_or("N/A");

    lines.push(Line::from(vec![
        Span::styled("  Status:  ", theme::dim()),
        Span::styled(status_text, status_style),
        Span::styled(" | ", theme::dim()),
        Span::styled(genre, theme::cyan()),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Origin:  ", theme::dim()),
        Span::styled(country, theme::cyan()),
    ]));

    // Render circular buffer resiliency progress bar!
    let filled = (app.buffer_percent / 10) as usize;
    let empty = 10 - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    lines.push(Line::from(vec![
        Span::styled("  Buffer:  ", theme::dim()),
        Span::styled(format!("[{}] ", bar), Style::default().fg(theme::highlight()).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{}% ", app.buffer_percent), theme::cyan()),
        Span::styled(format!("({}s)", app.buffer_seconds), theme::dim()),
    ]));

    // Render active segment capture file name!
    if app.recording_state == crate::app::RecordingState::Active {
        if let Some(ref filepath) = app.active_record_filepath {
            let filename = std::path::Path::new(filepath)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(filepath);
            lines.push(Line::from(vec![
                Span::styled("  Tape:    ", theme::dim()),
                Span::styled(format!("🔴 capture -> {}", filename), Style::default().fg(theme::error().fg.unwrap_or_default()).add_modifier(Modifier::BOLD)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn render_oscilloscope(frame: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let height = area.height as usize;

    if height == 0 || width == 0 {
        return;
    }

    // Mode 0: Spectrum Analyzer (renders vertical block bars with neon tri-color gradient)
    if app.playback == PlaybackState::Playing && app.visualizer_mode == 0 {
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
                let val = if app.visualizer_peaks.is_empty() {
                    0.0
                } else {
                    let t = col as f32 / width as f32;
                    let peak_idx = t * (app.visualizer_peaks.len() - 1) as f32;
                    let idx_floor = peak_idx.floor() as usize;
                    let idx_ceil = (idx_floor + 1).min(app.visualizer_peaks.len() - 1);
                    let frac = peak_idx - idx_floor as f32;
                    app.visualizer_peaks[idx_floor] * (1.0 - frac) + app.visualizer_peaks[idx_ceil] * frac
                };

                let h = val * height as f32;
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
        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
        return;
    }

    let mut canvas = BrailleCanvas::new(width, height);
    match app.playback {
        PlaybackState::Playing => {
            match app.visualizer_mode {
                1 => {
                    // Mode 1: Real-Time Oscilloscope using sample buffer
                    let pixel_width = width * 2;
                    let pixel_height = height * 4;
                    let center_y = pixel_height as f32 * 0.5;
                    let amplitude = (app.volume as f32 / 100.0) * (pixel_height as f32 * 0.45);

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
                    // Mode 2: Simulated Oscilloscope (fallback / original math wave)
                    let pixel_width = width * 2;
                    let pixel_height = height * 4;
                    let center_y = pixel_height as f32 * 0.5;
                    let amplitude = (app.volume as f32 / 100.0) * (pixel_height as f32 * 0.4);

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
            }
        }
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

    let active_style = Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD);
    let lines = canvas.to_lines(active_style, theme::dim());

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
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
    
    lines.push(Line::from(vec![
        Span::styled("   📼 Captured Session Mixtape ", Style::default().fg(theme::accent_secondary()).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("   ════════════════════════════", theme::dim()),
    ]));
    lines.push(Line::from(""));

    if app.song_history.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("   [ No tracks captured yet ]", theme::dim()),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("   Music playback will record", theme::dim()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   inline ICY metadata here...", theme::dim()),
        ]));
    } else {
        // Render song history in reverse (newest on top)
        let visible_rows = (area.height as usize).saturating_sub(4);
        
        for (idx, song) in app.song_history.iter().enumerate().rev().take(visible_rows) {
            let track_num = idx + 1;
            let track_tag = format!("   Track {:02}: ", track_num);
            
            lines.push(Line::from(vec![
                Span::styled(track_tag, Style::default().fg(theme::highlight()).add_modifier(Modifier::BOLD)),
                Span::styled(song.as_str(), theme::text()),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
