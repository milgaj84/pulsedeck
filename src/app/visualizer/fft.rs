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
