use crate::app::visualizer::gradient::{color_band_for_row, ColorBand};
use crate::ui::model::UiModel;
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub(super) fn render_spectrum_analyzer(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let width = area.width as usize;
    let height = area.height as usize;
    let peaks = &app.visualizer_peaks;

    if peaks.is_empty() {
        frame.render_widget(Paragraph::new(Vec::<Line<'static>>::new()), area);
        return;
    }

    let palette = theme::active_palette();
    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(width);

        for col in 0..width {
            let (band, is_spacer) = spectrum_column_slot(col, width, peaks.len());
            let val = if is_spacer { 0.0 } else { peaks[band] };
            let bar_height_f = val * height.saturating_sub(1).max(1) as f32;
            let bar_height = bar_height_f.round() as usize;
            let height_in_row = bar_height_f - (height - 1 - row) as f32;

            let char_str = if height_in_row <= 0.0 {
                " "
            } else if height_in_row >= 1.0 {
                "█"
            } else {
                let level = (height_in_row * 8.0).round() as usize;
                let blocks = [" ", " ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
                blocks[level.min(8)]
            };

            // Per-bar gradient: row position within this bar's height
            let row_in_bar = if height_in_row > 0.0 {
                bar_height
                    .saturating_sub(1)
                    .saturating_sub(height - 1 - row)
            } else {
                0
            };

            let color = if height_in_row <= 0.0 {
                palette.bg
            } else {
                match color_band_for_row(row_in_bar, bar_height) {
                    Some(ColorBand::Bottom) => palette.success,
                    Some(ColorBand::Middle) => palette.highlight,
                    Some(ColorBand::Top) => palette.accent,
                    None => palette.accent,
                }
            };

            let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
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

#[cfg(test)]
mod tests {
    use super::*;

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
