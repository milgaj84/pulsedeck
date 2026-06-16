pub(super) const SPECTRUM_ANALYSIS_BANDS: usize = 40;
const SPECTRUM_NOISE_FLOOR: f32 = 0.0008;
const SPECTRUM_OUTPUT_GAIN: f32 = 1.70;
const SPECTRUM_CONNECTING_PEAK_MIN: f32 = 0.03;
const SPECTRUM_CONNECTING_PEAK_MAX: f32 = 0.30;
const SPECTRUM_TREBLE_START: f32 = 0.82;
const SPECTRUM_TREBLE_SOFT_KNEE_SLOPE: f32 = 0.15;
const SPECTRUM_TREBLE_VARIANCE_EXPANSION: f32 = 0.22;
const SPECTRUM_TREBLE_VARIANCE_SENSITIVITY: f32 = 8.0;

pub(super) fn update_connecting_spectrum_peaks(peaks: &mut Vec<f32>, tick_count: u64) {
    if peaks.len() != SPECTRUM_ANALYSIS_BANDS {
        peaks.resize(SPECTRUM_ANALYSIS_BANDS, 0.0);
    }

    for (band, peak) in peaks.iter_mut().enumerate() {
        *peak = connecting_spectrum_peak(band, tick_count, SPECTRUM_ANALYSIS_BANDS);
    }
}

fn connecting_spectrum_peak(band: usize, tick_count: u64, total_bands: usize) -> f32 {
    let denominator = total_bands.saturating_sub(1).max(1) as f32;
    let band_t = band as f32 / denominator;
    let t = tick_count as f32;

    let sweep = ((band_t * 10.0 - t * 0.055).sin() + 1.0) * 0.5;
    let shimmer = ((band_t * 27.0 + t * 0.037).sin() + 1.0) * 0.5;
    let breathing = ((t * 0.045).sin() + 1.0) * 0.5;
    let low_bias = (1.0 - band_t).powf(0.45) * 0.08;

    let peak = low_bias + sweep.powf(2.0) * 0.15 + shimmer * 0.035 + breathing * 0.025;
    peak.clamp(SPECTRUM_CONNECTING_PEAK_MIN, SPECTRUM_CONNECTING_PEAK_MAX)
}

pub(super) fn spectrum_target(avg: f32, band: usize, total_bands: usize) -> f32 {
    let energy = (avg - SPECTRUM_NOISE_FLOOR).max(0.0);
    let compressed = compress_spectrum_energy(energy);
    let scaled = compressed * spectrum_gain_curve(band, total_bands) * SPECTRUM_OUTPUT_GAIN;

    scaled.clamp(0.0, 0.92)
}

fn compress_spectrum_energy(avg: f32) -> f32 {
    avg.powf(0.50)
}

fn spectrum_gain_curve(band: usize, total_bands: usize) -> f32 {
    let t = band_position(band, total_bands);

    // Gentle broad lift through mids/highs without the old final-bin rocket boost.
    spectrum_broad_lift(t) * high_treble_soft_knee(t)
}

fn spectrum_broad_lift(t: f32) -> f32 {
    1.0 + 1.20 * t.powf(0.72)
}

fn high_treble_soft_knee(t: f32) -> f32 {
    if t > SPECTRUM_TREBLE_START {
        1.0 - (t - SPECTRUM_TREBLE_START) * SPECTRUM_TREBLE_SOFT_KNEE_SLOPE
    } else {
        1.0
    }
}

pub(super) fn preserve_treble_variance(
    target: f32,
    band: usize,
    total_bands: usize,
    variance: f32,
) -> f32 {
    (target * treble_variance_expansion_factor(band, total_bands, variance)).clamp(0.0, 0.92)
}

fn treble_variance_expansion_factor(band: usize, total_bands: usize, variance: f32) -> f32 {
    let t = band_position(band, total_bands);
    if t <= SPECTRUM_TREBLE_START {
        return 1.0;
    }

    let treble_t = ((t - SPECTRUM_TREBLE_START) / (1.0 - SPECTRUM_TREBLE_START)).clamp(0.0, 1.0);
    let structured_motion =
        (variance.sqrt() * SPECTRUM_TREBLE_VARIANCE_SENSITIVITY).clamp(0.0, 1.0);

    1.0 + structured_motion * treble_t * SPECTRUM_TREBLE_VARIANCE_EXPANSION
}

pub(super) fn treble_delta_variance(
    targets: &[f32],
    previous_peaks: &[f32],
    total_bands: usize,
) -> f32 {
    let start = treble_start_band(total_bands);
    let end = targets.len().min(previous_peaks.len());

    if start >= end {
        return 0.0;
    }

    let deltas = (start..end)
        .map(|band| (targets[band] - previous_peaks[band]).abs())
        .collect::<Vec<_>>();

    variance(&deltas)
}

fn treble_start_band(total_bands: usize) -> usize {
    if total_bands == 0 {
        return 0;
    }

    let denominator = total_bands.saturating_sub(1).max(1) as f32;
    ((denominator * SPECTRUM_TREBLE_START).floor() as usize + 1).min(total_bands)
}

fn band_position(band: usize, total_bands: usize) -> f32 {
    let denominator = total_bands.saturating_sub(1).max(1) as f32;
    band as f32 / denominator
}

fn variance(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }

    let mean = values.iter().sum::<f32>() / values.len() as f32;
    values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f32>()
        / values.len() as f32
}

pub(super) fn smooth_spectrum_targets(targets: &[f32]) -> Vec<f32> {
    let mut smoothed = Vec::with_capacity(targets.len());

    for i in 0..targets.len() {
        let previous = if i > 0 { targets[i - 1] } else { targets[i] };
        let current = targets[i];
        let next = targets.get(i + 1).copied().unwrap_or(current);

        smoothed.push(previous * 0.20 + current * 0.60 + next * 0.20);
    }

    smoothed
}

pub(super) fn spectrum_release_curve(band: usize, total_bands: usize) -> f32 {
    let denominator = total_bands.saturating_sub(1).max(1) as f32;
    let t = band as f32 / denominator;

    if t > 0.82 {
        0.12
    } else {
        0.075
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spectrum_gain_curve_softens_high_end_without_flattening_treble() {
        let mid_gain = spectrum_gain_curve(20, SPECTRUM_ANALYSIS_BANDS);
        let final_gain = spectrum_gain_curve(SPECTRUM_ANALYSIS_BANDS - 1, SPECTRUM_ANALYSIS_BANDS);
        let full_final_lift = spectrum_broad_lift(1.0);

        assert!(final_gain > mid_gain);
        assert!(final_gain < full_final_lift);
        assert!(final_gain > full_final_lift * 0.95);
    }

    #[test]
    fn treble_variance_expansion_boosts_structured_highs_only() {
        let quiet_treble = preserve_treble_variance(
            0.4,
            SPECTRUM_ANALYSIS_BANDS - 1,
            SPECTRUM_ANALYSIS_BANDS,
            0.0,
        );
        let structured_treble = preserve_treble_variance(
            0.4,
            SPECTRUM_ANALYSIS_BANDS - 1,
            SPECTRUM_ANALYSIS_BANDS,
            0.04,
        );
        let mid_band = preserve_treble_variance(0.4, 20, SPECTRUM_ANALYSIS_BANDS, 0.04);

        assert!(structured_treble > quiet_treble);
        assert_eq!(mid_band, 0.4);
        assert!(structured_treble <= 0.92);
    }

    #[test]
    fn treble_delta_variance_detects_structured_motion() {
        let previous = vec![0.10; SPECTRUM_ANALYSIS_BANDS];
        let flat_targets = vec![0.20; SPECTRUM_ANALYSIS_BANDS];
        let mut structured_targets = flat_targets.clone();

        for (band, target) in structured_targets
            .iter_mut()
            .enumerate()
            .skip(treble_start_band(SPECTRUM_ANALYSIS_BANDS))
        {
            *target = if band.is_multiple_of(2) { 0.45 } else { 0.05 };
        }

        let flat_variance =
            treble_delta_variance(&flat_targets, &previous, SPECTRUM_ANALYSIS_BANDS);
        let structured_variance =
            treble_delta_variance(&structured_targets, &previous, SPECTRUM_ANALYSIS_BANDS);

        assert!(structured_variance > flat_variance);
    }

    #[test]
    fn spectrum_target_gates_tiny_noise_floor() {
        assert_eq!(
            spectrum_target(SPECTRUM_NOISE_FLOOR * 0.5, 30, SPECTRUM_ANALYSIS_BANDS),
            0.0
        );
    }

    #[test]
    fn smoothing_preserves_constant_flat_targets() {
        let targets = vec![0.42; SPECTRUM_ANALYSIS_BANDS];
        let smoothed = smooth_spectrum_targets(&targets);

        assert_eq!(smoothed, targets);
    }

    #[test]
    fn connecting_spectrum_pattern_resizes_and_stays_subtle() {
        let mut peaks = vec![0.99; 3];

        update_connecting_spectrum_peaks(&mut peaks, 42);

        assert_eq!(peaks.len(), SPECTRUM_ANALYSIS_BANDS);
        assert!(peaks.iter().all(|peak| {
            (SPECTRUM_CONNECTING_PEAK_MIN..=SPECTRUM_CONNECTING_PEAK_MAX).contains(peak)
        }));
        assert!(peaks
            .iter()
            .any(|peak| *peak > SPECTRUM_CONNECTING_PEAK_MIN));
    }

    #[test]
    fn connecting_spectrum_pattern_moves_over_time() {
        let changed = (0..SPECTRUM_ANALYSIS_BANDS).any(|band| {
            let early = connecting_spectrum_peak(band, 0, SPECTRUM_ANALYSIS_BANDS);
            let later = connecting_spectrum_peak(band, 24, SPECTRUM_ANALYSIS_BANDS);

            (early - later).abs() > 0.01
        });

        assert!(changed);
    }

    #[test]
    fn high_treble_releases_faster_than_midrange() {
        let mid_release = spectrum_release_curve(20, SPECTRUM_ANALYSIS_BANDS);
        let high_release =
            spectrum_release_curve(SPECTRUM_ANALYSIS_BANDS - 1, SPECTRUM_ANALYSIS_BANDS);

        assert!(high_release > mid_release);
    }
}
