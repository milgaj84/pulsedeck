use crate::app::{App, PlaybackState};
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

const DECK_INNER_WIDTH: usize = 44;
#[cfg(test)]
const DECK_REEL_CELL_WIDTH: usize = 10;
const DECK_SIGNAL_WIDTH: usize = 4;
pub(super) const DECK_ART_HEIGHT: u16 = 9;

pub(super) fn render_cassette(frame: &mut Frame, area: Rect, app: &App) {
    let lines = build_deck_lines(DECK_INNER_WIDTH, app.tick_count, &app.player.state);

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

#[derive(Debug, Clone)]
struct DeckSegment {
    text: String,
    style: Style,
}

fn build_deck_lines(
    inner_width: usize,
    tick_count: u64,
    playback: &PlaybackState,
) -> Vec<Line<'static>> {
    let shell_style = theme::dim();
    let reel_style = Style::default()
        .fg(theme::highlight())
        .bg(theme::bg())
        .add_modifier(Modifier::BOLD);

    let mut lines = Vec::with_capacity(DECK_ART_HEIGHT as usize);
    lines.push(deck_border_line("╭", "╮", inner_width, shell_style));
    lines.push(deck_label_line(inner_width, shell_style));
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
    lines.push(deck_border_line("╰", "╯", inner_width, shell_style));

    lines
}

fn deck_label_line(inner_width: usize, shell_style: Style) -> Line<'static> {
    let brand = "P U L S E  D E C K";
    let label_style = Style::default()
        .fg(theme::accent_secondary())
        .bg(theme::bg())
        .add_modifier(Modifier::BOLD);
    let status = "A-SIDE";
    let status_style = Style::default()
        .fg(theme::highlight())
        .bg(theme::bg())
        .add_modifier(Modifier::BOLD);

    let fixed_padding = 4;
    let spacer_width = inner_width
        .saturating_sub(crate::ui::text::visible_len(brand))
        .saturating_sub(crate::ui::text::visible_len(status))
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

fn reel_cells_for_state(tick_count: u64, playback: &PlaybackState) -> (String, String) {
    match playback {
        PlaybackState::Playing | PlaybackState::FadingOut { .. } => {
            let transfer_step = ((tick_count / 6) % 8) as usize;
            let transfer = if transfer_step < 4 {
                transfer_step
            } else {
                7 - transfer_step
            };

            let left_fill = DECK_SIGNAL_WIDTH.saturating_sub(transfer).max(1);
            let right_fill = (1 + transfer).min(DECK_SIGNAL_WIDTH);

            (
                reel_cell("○", fixed_signal_mass(left_fill, DECK_SIGNAL_WIDTH), "○"),
                reel_cell("○", fixed_signal_mass(right_fill, DECK_SIGNAL_WIDTH), "○"),
            )
        }
        PlaybackState::Connecting => {
            let hub = if (tick_count / 4).is_multiple_of(2) {
                "◌"
            } else {
                "○"
            };
            let tape = fixed_signal_mass(1, DECK_SIGNAL_WIDTH);
            (reel_cell(hub, tape.clone(), hub), reel_cell(hub, tape, hub))
        }
        PlaybackState::Paused => {
            let tape = fixed_signal_mass(2, DECK_SIGNAL_WIDTH);
            (reel_cell("○", tape.clone(), "○"), reel_cell("○", tape, "○"))
        }
        PlaybackState::Error(_) => {
            let tape = fixed_signal_mass(0, DECK_SIGNAL_WIDTH);
            (reel_cell("×", tape.clone(), "×"), reel_cell("×", tape, "×"))
        }
        PlaybackState::Stopped => {
            let tape = fixed_signal_mass(0, DECK_SIGNAL_WIDTH);
            (reel_cell("○", tape.clone(), "○"), reel_cell("○", tape, "○"))
        }
    }
}

fn fixed_signal_mass(fill: usize, width: usize) -> String {
    let fill = fill.min(width);
    format!("{}{}", "█".repeat(fill), "░".repeat(width - fill))
}

fn reel_cell(left_hub: &str, tape: String, right_hub: &str) -> String {
    format!("│ {left_hub}{tape}{right_hub} │")
}

fn deck_border_line(
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

fn shell_line(inner_width: usize, shell_style: Style, parts: Vec<DeckSegment>) -> Line<'static> {
    let mut spans = Vec::with_capacity(parts.len() + 3);
    let mut remaining = inner_width;

    spans.push(Span::styled("│", shell_style));

    for part in parts {
        if remaining == 0 {
            break;
        }

        let part_width = crate::ui::text::visible_len(&part.text);
        let text = if part_width > remaining {
            crate::ui::text::truncate_to_chars(&part.text, remaining)
        } else {
            part.text
        };
        let text_width = crate::ui::text::visible_len(&text);

        remaining = remaining.saturating_sub(text_width);
        spans.push(Span::styled(text, part.style));
    }

    if remaining > 0 {
        spans.push(Span::styled(" ".repeat(remaining), shell_style));
    }

    spans.push(Span::styled("│", shell_style));
    Line::from(spans)
}

fn segment(text: impl Into<String>, style: Style) -> DeckSegment {
    DeckSegment {
        text: text.into(),
        style,
    }
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
    fn deck_rows_have_equal_width() {
        let expected_width = DECK_INNER_WIDTH + 2;
        let lines = build_deck_lines(DECK_INNER_WIDTH, 0, &PlaybackState::Playing);

        assert_eq!(lines.len(), DECK_ART_HEIGHT as usize);
        for line in lines {
            assert_eq!(line_width(&line), expected_width);
        }
    }

    #[test]
    fn reel_animation_keeps_constant_cell_width() {
        for tick_count in 0..96 {
            let (left_reel, right_reel) = reel_cells_for_state(tick_count, &PlaybackState::Playing);

            assert_eq!(left_reel.chars().count(), DECK_REEL_CELL_WIDTH);
            assert_eq!(right_reel.chars().count(), DECK_REEL_CELL_WIDTH);
        }
    }

    #[test]
    fn signal_mass_keeps_constant_width() {
        for fill in 0..=DECK_SIGNAL_WIDTH + 2 {
            assert_eq!(
                fixed_signal_mass(fill, DECK_SIGNAL_WIDTH).chars().count(),
                DECK_SIGNAL_WIDTH
            );
        }
    }
}
