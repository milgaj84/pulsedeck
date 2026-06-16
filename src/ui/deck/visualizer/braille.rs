use ratatui::prelude::*;

/// A micro-pixel canvas helper that aggregates sub-pixels into Unicode Braille characters (U+2800 - U+28FF)
pub(super) struct BrailleCanvas {
    width: usize,
    height: usize,
    grid: Vec<u8>,
}

impl BrailleCanvas {
    pub(super) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            grid: vec![0u8; width * height],
        }
    }

    pub(super) fn set_pixel(&mut self, x: usize, y: usize) {
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

    pub(super) fn to_lines(&self, active_style: Style, dim_style: Style) -> Vec<Line<'static>> {
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
