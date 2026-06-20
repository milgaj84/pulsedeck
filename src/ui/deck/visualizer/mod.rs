use crate::app::PlaybackState;
use crate::ui::model::UiModel;
use crate::ui::theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

mod braille;
mod oscilloscope;
mod spectrum;

pub(super) use oscilloscope::render_oscilloscope;

fn visualizer_title(app: &UiModel<'_>) -> &'static str {
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

fn render_visualizer_signal(frame: &mut Frame, area: Rect, app: &UiModel<'_>) {
    let width = area.width as usize;
    let height = area.height as usize;

    if height == 0 || width == 0 {
        return;
    }

    // Mode 0: Spectrum Analyzer (renders vertical block bars with neon tri-color gradient)
    if should_render_spectrum_analyzer(&app.player.state, app.visualizer_mode) {
        spectrum::render_spectrum_analyzer(frame, area, app);
        return;
    }

    let mut canvas = braille::BrailleCanvas::new(width, height);
    match app.player.state {
        PlaybackState::Playing | PlaybackState::FadingOut { .. } => match app.visualizer_mode {
            1 => {
                let pixel_width = width * 2;
                let pixel_height = height * 4;
                let center_y = pixel_height as f32 * 0.5;
                let amplitude = visualizer_amplitude_gain(&app.player.state, app.volume)
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
                let amplitude = visualizer_amplitude_gain(&app.player.state, app.volume)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
