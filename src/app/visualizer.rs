use super::*;

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
        let num_bands = 40;
        let bins_count = n / 2;

        if self.visualizer_peaks.len() != num_bands {
            self.visualizer_peaks = vec![0.0; num_bands];
        }

        for x in 0..num_bands {
            let t = x as f32 / num_bands as f32;
            let min_bin = 1.0_f32;
            let max_bin = bins_count as f32;
            let bin_start_f = min_bin * (max_bin / min_bin).powf(t);
            let bin_end_f = min_bin * (max_bin / min_bin).powf((x + 1) as f32 / num_bands as f32);

            let start = (bin_start_f.floor() as usize).clamp(0, bins_count - 1);
            let end = (bin_end_f.ceil() as usize).clamp(start + 1, bins_count);

            let mut sum = 0.0;
            let mut count = 0;
            for bin in fft_output.iter().take(end).skip(start) {
                sum += bin.norm() / n as f32;
                count += 1;
            }
            let avg = if count > 0 { sum / count as f32 } else { 0.0 };

            // Equalize: boost higher frequency ranges dynamically since human hearing perceives them differently.
            let boost = 1.0 + (x as f32 / num_bands as f32) * 4.0;
            let compressed = avg.sqrt();
            let scaled = compressed * boost * 2.5; // Multiplier tuned for normalized & compressed avg.
            let target = scaled.clamp(0.0, 1.0);

            // 4. Gravity peak decay physics.
            let current = self.visualizer_peaks[x];
            if target > current {
                self.visualizer_peaks[x] = target; // Fast rise.
            } else {
                self.visualizer_peaks[x] = (current - 0.08).max(target).max(0.0);
                // Smooth fall.
            }
        }
    }
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
