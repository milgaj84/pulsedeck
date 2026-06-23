#[derive(Debug, Clone, Copy)]
pub(super) struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    pub(super) fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    pub(super) fn from_real(re: f32) -> Self {
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
            im: self.im * other.re + self.re * other.im,
        }
    }

    fn norm(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

pub(super) fn fft_rec(input: &[Complex], output: &mut [Complex]) {
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

pub(super) fn average_log_band_energy(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_rec_single_element_returns_unchanged() {
        let input = [Complex::new(3.5, -1.2)];
        let mut output = [Complex::zero()];
        fft_rec(&input, &mut output);
        assert!((output[0].re - 3.5).abs() < 1e-5);
        assert!((output[0].im - (-1.2)).abs() < 1e-5);
    }

    #[test]
    fn fft_rec_four_element_known_output() {
        // Input: [1, 2, 3, 4] (real-valued)
        // Expected DFT:
        //   X[0] = 10 + 0i
        //   X[1] = -2 + 2i
        //   X[2] = -2 + 0i
        //   X[3] = -2 - 2i
        let input = [
            Complex::from_real(1.0),
            Complex::from_real(2.0),
            Complex::from_real(3.0),
            Complex::from_real(4.0),
        ];
        let mut output = vec![Complex::zero(); 4];
        fft_rec(&input, &mut output);

        let expected = [
            Complex::new(10.0, 0.0),
            Complex::new(-2.0, 2.0),
            Complex::new(-2.0, 0.0),
            Complex::new(-2.0, -2.0),
        ];

        for (i, (out, exp)) in output.iter().zip(expected.iter()).enumerate() {
            assert!(
                (out.re - exp.re).abs() < 1e-5,
                "bin {i} real: got {}, expected {}",
                out.re,
                exp.re
            );
            assert!(
                (out.im - exp.im).abs() < 1e-5,
                "bin {i} imag: got {}, expected {}",
                out.im,
                exp.im
            );
        }
    }

    #[test]
    fn average_log_band_energy_all_zeros_returns_zero() {
        let fft_output = vec![Complex::zero(); 512];
        let result = average_log_band_energy(&fft_output, 0, 40, 256, 512);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn average_log_band_energy_nonzero_returns_positive() {
        // Band 0 with total_bands=40, bins_count=256 covers bins starting at index 1.
        // Place a non-zero value at bin index 1.
        let mut fft_output = vec![Complex::zero(); 512];
        fft_output[1] = Complex::new(5.0, 0.0);
        let result = average_log_band_energy(&fft_output, 0, 40, 256, 512);
        assert!(result > 0.0, "expected positive energy, got {result}");
    }

    #[test]
    fn fft_rec_output_length_matches_input() {
        let input: Vec<Complex> = (0..8).map(|i| Complex::from_real(i as f32)).collect();
        let mut output = vec![Complex::zero(); 8];
        fft_rec(&input, &mut output);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn fft_rec_pure_sine_dominant_peak() {
        // Pure sine at bin frequency k=1: x[n] = sin(2π·1·n/8)
        let n = 8;
        let k = 1;
        let input: Vec<Complex> = (0..n)
            .map(|i| {
                let angle = 2.0 * std::f32::consts::PI * k as f32 * i as f32 / n as f32;
                Complex::from_real(angle.sin())
            })
            .collect();
        let mut output = vec![Complex::zero(); n];
        fft_rec(&input, &mut output);

        // Find peak magnitude
        let magnitudes: Vec<f32> = output.iter().map(|c| c.norm()).collect();
        let peak = magnitudes.iter().cloned().fold(0.0_f32, f32::max);

        // Bin k=1 should be the dominant peak (or bin n-k=7 due to symmetry)
        let mag_at_k = magnitudes[k];
        assert!(
            mag_at_k > peak * 0.9,
            "bin {k} magnitude {mag_at_k} should be near the peak {peak}"
        );

        // All other bins (excluding the symmetric bin at n-k) should be below 10% of peak
        for (i, &mag) in magnitudes.iter().enumerate() {
            if i == k || i == n - k {
                continue;
            }
            assert!(
                mag < peak * 0.1,
                "bin {i} magnitude {mag} should be below 10% of peak {peak}"
            );
        }
    }
}
