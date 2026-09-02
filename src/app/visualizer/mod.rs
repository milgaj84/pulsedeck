use super::*;

mod fft;
pub mod gradient;
mod spectrum;

use fft::*;
use spectrum::*;

impl App {
    /// Run Fast Fourier Transform (FFT) on the audio samples and update the spectrum peaks with gravity decay.
    pub fn update_visualizer(&mut self) {
        if self.playback.view.state == PlaybackState::Connecting
            && self.ui.visualizer_mode == VisualizerMode::Spectrum
        {
            update_connecting_spectrum_peaks(&mut self.ui.visualizer_peaks, self.ui.tick_count);
            return;
        }

        if !playback_drives_sample_visualizer(&self.playback.view.state) {
            // Gradually decay peaks when stopped/paused.
            for peak in &mut self.ui.visualizer_peaks {
                *peak = (*peak * 0.82).max(0.0);
            }
            return;
        }

        // Extract raw samples from the circular buffer.
        let mut samples = Vec::new();
        if let Ok(buf) = self.playback.sample_buffer.lock() {
            let n = buf.len();
            let window_size = 512;
            if n >= window_size {
                let start_idx = n - window_size;
                samples.extend(buf.iter().skip(start_idx).take(window_size).copied());
            } else {
                samples.extend(buf.iter().copied());
                while samples.len() < window_size {
                    samples.push(0.0);
                }
            }
        }

        if samples.is_empty() {
            return;
        }

        let n = samples.len();
        // 1. Apply Hanning window to minimize spectral leakage.
        let mut windowed = vec![0.0; n];
        for i in 0..n {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos());
            windowed[i] = samples[i] * w;
        }

        // 2. Perform Radix-2 Cooley-Tukey FFT.
        let fft_input: Vec<Complex> = windowed.into_iter().map(Complex::from_real).collect();
        let mut fft_output = vec![Complex::zero(); n];
        fft_rec(&fft_input, &mut fft_output);

        // 3. Map bins logarithmically to equal-width frequency bands.
        let bins_count = n / 2;

        if self.ui.visualizer_peaks.len() != SPECTRUM_ANALYSIS_BANDS {
            self.ui.visualizer_peaks = vec![0.0; SPECTRUM_ANALYSIS_BANDS];
        }

        let mut targets = Vec::with_capacity(SPECTRUM_ANALYSIS_BANDS);
        for band in 0..SPECTRUM_ANALYSIS_BANDS {
            let avg =
                average_log_band_energy(&fft_output, band, SPECTRUM_ANALYSIS_BANDS, bins_count, n);
            let target = spectrum_target(avg, band, SPECTRUM_ANALYSIS_BANDS);
            targets.push(target);
        }

        let targets = smooth_spectrum_targets(&targets);
        let treble_variance =
            treble_delta_variance(&targets, &self.ui.visualizer_peaks, SPECTRUM_ANALYSIS_BANDS);

        for (band, target) in targets.iter().copied().enumerate() {
            let target =
                preserve_treble_variance(target, band, SPECTRUM_ANALYSIS_BANDS, treble_variance);
            let current = self.ui.visualizer_peaks[band];
            if target > current {
                self.ui.visualizer_peaks[band] = target; // Fast rise.
            } else {
                let release = spectrum_release_curve(band, SPECTRUM_ANALYSIS_BANDS);
                self.ui.visualizer_peaks[band] = (current - release).max(target).max(0.0);
            }
        }
    }
}

fn playback_drives_sample_visualizer(playback: &PlaybackState) -> bool {
    matches!(
        playback,
        PlaybackState::Playing | PlaybackState::FadingOut { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fading_out_drives_sample_visualizer() {
        assert!(playback_drives_sample_visualizer(&PlaybackState::Playing));
        assert!(playback_drives_sample_visualizer(
            &PlaybackState::FadingOut {
                current_volume: 0.4,
            }
        ));
        assert!(!playback_drives_sample_visualizer(&PlaybackState::Stopped));
        assert!(!playback_drives_sample_visualizer(&PlaybackState::Paused));
    }
}
