use super::*;

const SPECTRUM_BANDS: usize = 40;
const SPECTRUM_NOISE_FLOOR: f32 = 0.0008;

impl App {
    /// Run Fast Fourier Transform (FFT) on the audio samples and update the spectrum peaks with gravity decay.
    pub fn update_visualizer(&mut self) {
        if self.playback != PlaybackState::Playing {
            // Gradually decay peaks when stopped/paused.
            for peak in &mut self.visualizer_peaks {
                *peak = (*peak * 0.82).max(0.0);
            }
            return;
        }

        // Extract raw samples from the circular buffer.
        let mut samples = Vec::new();
        if let Ok(buf) = self.sample_buffer.lock() {
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

        if self.visualizer_peaks.len() != SPECTRUM_BANDS {
            self.visualizer_peaks = vec![0.0; SPECTRUM_BANDS];
        }

        let mut targets = Vec::with_capacity(SPECTRUM_BANDS);
        for band in 0..SPECTRUM_BANDS {
            let avg = average_log_band_energy(&fft_output, band, SPECTRUM_BANDS, bins_count, n);
            let target = spectrum_target(avg, band, SPECTRUM_BANDS);
            targets.push(target);
        }

        let targets = smooth_spectrum_targets(&targets);

        for (band, target) in targets.iter().copied().enumerate() {
            let current = self.visualizer_peaks[band];
            if target > current {
                self.visualizer_peaks[band] = target; // Fast rise.
            } else {
                let release = spectrum_release_curve(band, SPECTRUM_BANDS);
                self.visualizer_peaks[band] = (current - release).max(target).max(0.0);
            }
        }
    }
}

fn average_log_band_energy(
    fft_output: &[Complex],
    band: usize,
    total_bands: usize,
    bins_count: usize,
    sample_count: usize,
) -> f32 {
    let t = band as f32 / total_bands as f32;
    let min_bin = 1.0_f32;
    let max_bin = bins_count as f32;
    let bin_start_f = min_bin * (max_bin / min_bin).powf(t);
    let bin_end_f = min_bin * (max_bin / min_bin).powf((band + 1) as f32 / total_bands as f32);

    let start = (bin_start_f.floor() as usize).clamp(0, bins_count - 1);
    let end = (bin_end_f.ceil() as usize).clamp(start + 1, bins_count);

    let mut sum = 0.0;
    let mut count = 0;
    for bin in fft_output.iter().take(end).skip(start) {
        sum += bin.norm() / sample_count as f32;
        count += 1;
    }

    if count > 0 {
        sum / count as f32
    } else {
        0.0
    }
}

fn spectrum_target(avg: f32, band: usize, total_bands: usize) -> f32 {
    let energy = (avg - SPECTRUM_NOISE_FLOOR).max(0.0);
    let compressed = compress_spectrum_energy(energy);
    let scaled = compressed * spectrum_gain_curve(band, total_bands) * 2.25;

    scaled.clamp(0.0, 1.0)
}

fn compress_spectrum_energy(avg: f32) -> f32 {
    avg.powf(0.42)
}

fn spectrum_gain_curve(band: usize, total_bands: usize) -> f32 {
    let denominator = total_bands.saturating_sub(1).max(1) as f32;
    let t = band as f32 / denominator;

    // Gentle broad lift through mids/highs without the old final-bin rocket boost.
    let broad_lift = 1.0 + 1.45 * t.powf(0.70);

    // Tame the last treble shelf so weak high-frequency bins do not dominate visually.
    let high_tame = if t > 0.82 {
        1.0 - ((t - 0.82) / 0.18).clamp(0.0, 1.0) * 0.28
    } else {
        1.0
    };

    broad_lift * high_tame
}

fn smooth_spectrum_targets(targets: &[f32]) -> Vec<f32> {
    let mut smoothed = Vec::with_capacity(targets.len());

    for i in 0..targets.len() {
        let previous = if i > 0 { targets[i - 1] } else { targets[i] };
        let current = targets[i];
        let next = targets.get(i + 1).copied().unwrap_or(current);

        smoothed.push(previous * 0.20 + current * 0.60 + next * 0.20);
    }

    smoothed
}

fn spectrum_release_curve(band: usize, total_bands: usize) -> f32 {
    let denominator = total_bands.saturating_sub(1).max(1) as f32;
    let t = band as f32 / denominator;

    if t > 0.82 { 0.11 } else { 0.075 }
}

#[derive(Debug, Clone, Copy)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    fn from_real(re: f32) -> Self {
        Self { re, im: 0.0 }
    }

    fn add(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            re: self.re - other.re,
            im: self.im - other.im,
        }
    }

    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn norm(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

fn fft_rec(input: &[Complex], output: &mut [Complex]) {
    let n = input.len();
    if n <= 1 {
        if n == 1 {
            output[0] = input[0];
        }
        return;
    }

    let mut even = vec![Complex::zero(); n / 2];
    let mut odd = vec![Complex::zero(); n / 2];
    for i in 0..n / 2 {
        even[i] = input[2 * i];
        odd[i] = input[2 * i + 1];
    }

    let mut even_fft = vec![Complex::zero(); n / 2];
    let mut odd_fft = vec![Complex::zero(); n / 2];
    fft_rec(&even, &mut even_fft);
    fft_rec(&odd, &mut odd_fft);

    for k in 0..n / 2 {
        let angle = -2.0 * std::f32::consts::PI * (k as f32) / (n as f32);
        let twiddle = Complex::new(angle.cos(), angle.sin());
        let t = twiddle.mul(odd_fft[k]);
        output[k] = even_fft[k].add(t);
        output[k + n / 2] = even_fft[k].sub(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_gain_curve_does_not_rocket_final_band() {
        let mid_gain = spectrum_gain_curve(20, SPECTRUM_BANDS);
        let final_gain = spectrum_gain_curve(SPECTRUM_BANDS - 1, SPECTRUM_BANDS);

        assert!(final_gain > mid_gain);
        assert!(final_gain < 2.0);
    }

    #[test]
    fn spectrum_target_gates_tiny_noise_floor() {
        assert_eq!(spectrum_target(SPECTRUM_NOISE_FLOOR * 0.5, 30, SPECTRUM_BANDS), 0.0);
    }

    #[test]
    fn smoothing_preserves_constant_flat_targets() {
        let targets = vec![0.42; SPECTRUM_BANDS];
        let smoothed = smooth_spectrum_targets(&targets);

        assert_eq!(smoothed, targets);
    }

    #[test]
    fn high_treble_releases_faster_than_midrange() {
        let mid_release = spectrum_release_curve(20, SPECTRUM_BANDS);
        let high_release = spectrum_release_curve(SPECTRUM_BANDS - 1, SPECTRUM_BANDS);

        assert!(high_release > mid_release);
    }
}
