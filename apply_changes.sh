#!/usr/bin/env bash
set -euo pipefail

mkdir -p src/app src/ui

python3 <<'PY'
from pathlib import Path

def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"Missing expected block while patching {label}")
    return text.replace(old, new, 1)

path = Path("src/app/visualizer.rs")
text = path.read_text(encoding="utf-8")

if "SPECTRUM_CONNECTING_PEAK_MAX" not in text:
    text = replace_once(
        text,
        "const SPECTRUM_OUTPUT_GAIN: f32 = 1.70;\n",
        """const SPECTRUM_OUTPUT_GAIN: f32 = 1.70;
const SPECTRUM_CONNECTING_PEAK_MIN: f32 = 0.03;
const SPECTRUM_CONNECTING_PEAK_MAX: f32 = 0.30;
""",
        "visualizer connecting constants",
    )

if "update_connecting_spectrum_peaks(&mut self.visualizer_peaks" not in text:
    text = replace_once(
        text,
        """    pub fn update_visualizer(&mut self) {
        if self.playback != PlaybackState::Playing {
            // Gradually decay peaks when stopped/paused.
            for peak in &mut self.visualizer_peaks {
                *peak = (*peak * 0.82).max(0.0);
            }
            return;
        }

""",
        """    pub fn update_visualizer(&mut self) {
        if self.playback == PlaybackState::Connecting && self.visualizer_mode == 0 {
            update_connecting_spectrum_peaks(&mut self.visualizer_peaks, self.tick_count);
            return;
        }

        if self.playback != PlaybackState::Playing {
            // Gradually decay peaks when stopped/paused.
            for peak in &mut self.visualizer_peaks {
                *peak = (*peak * 0.82).max(0.0);
            }
            return;
        }

""",
        "visualizer connecting early return",
    )

if "fn update_connecting_spectrum_peaks" not in text:
    text = replace_once(
        text,
        "\nfn average_log_band_energy(\n",
        """
fn update_connecting_spectrum_peaks(peaks: &mut Vec<f32>, tick_count: u64) {
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

fn average_log_band_energy(
""",
        "visualizer connecting helper functions",
    )

if "connecting_spectrum_pattern_resizes_and_stays_subtle" not in text:
    text = replace_once(
        text,
        """    #[test]
    fn high_treble_releases_faster_than_midrange() {
""",
        """    #[test]
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
""",
        "visualizer connecting tests",
    )

path.write_text(text, encoding="utf-8")

path = Path("src/ui/deck.rs")
text = path.read_text(encoding="utf-8")

if "should_render_spectrum_analyzer(&app.playback, app.visualizer_mode)" not in text:
    text = replace_once(
        text,
        """    if app.playback == PlaybackState::Playing && app.visualizer_mode == 0 {
        render_spectrum_analyzer(frame, area, app);
        return;
    }
""",
        """    if should_render_spectrum_analyzer(&app.playback, app.visualizer_mode) {
        render_spectrum_analyzer(frame, area, app);
        return;
    }
""",
        "deck spectrum render routing",
    )

if "fn should_render_spectrum_analyzer" not in text:
    text = replace_once(
        text,
        """fn visualizer_title(app: &App) -> &'static str {
    match app.visualizer_mode {
        0 => " RTA SPECTRUM ",
        1 => " REAL OSC ",
        _ => " SIM OSC ",
    }
}

""",
        """fn visualizer_title(app: &App) -> &'static str {
    match app.visualizer_mode {
        0 => " RTA SPECTRUM ",
        1 => " REAL OSC ",
        _ => " SIM OSC ",
    }
}

fn should_render_spectrum_analyzer(playback: &PlaybackState, visualizer_mode: usize) -> bool {
    visualizer_mode == 0 && matches!(playback, PlaybackState::Playing | PlaybackState::Connecting)
}

""",
        "deck spectrum routing helper",
    )

if "spectrum_renderer_stays_active_while_connecting" not in text:
    text = replace_once(
        text,
        """    #[test]
    fn spectrum_column_slot_adds_spacer_when_band_is_wide() {
""",
        """    #[test]
    fn spectrum_renderer_stays_active_while_connecting() {
        assert!(should_render_spectrum_analyzer(&PlaybackState::Playing, 0));
        assert!(should_render_spectrum_analyzer(&PlaybackState::Connecting, 0));
        assert!(!should_render_spectrum_analyzer(&PlaybackState::Paused, 0));
        assert!(!should_render_spectrum_analyzer(&PlaybackState::Connecting, 1));
    }

    #[test]
    fn spectrum_column_slot_adds_spacer_when_band_is_wide() {
""",
        "deck spectrum routing tests",
    )

path.write_text(text, encoding="utf-8")

path = Path("README.md")
text = path.read_text(encoding="utf-8")

old = "- 📊 **Deck visualizers** — press `v` to cycle between a calibrated RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope. The RTA is tuned to avoid artificial final-treble spikes while keeping bars readable.\n"
new = "- 📊 **Deck visualizers** — press `v` to cycle between a calibrated RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope. The RTA is tuned to avoid artificial final-treble spikes, keeps bars readable, and shows a subtle tuning pulse while streams connect.\n"
if old in text:
    text = text.replace(old, new, 1)

if "In RTA Spectrum mode, the signal screen shows a subtle tuning pulse" not in text:
    text = replace_once(
        text,
        "- Press `v` to cycle the deck signal display between RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope.\n",
        "- Press `v` to cycle the deck signal display between RTA Spectrum, Real Oscilloscope, and Simulated Oscilloscope.\n- In RTA Spectrum mode, the signal screen shows a subtle tuning pulse during connection handshakes, so slow streams look active instead of blank.\n",
        "README cassette deck usage",
    )

path.write_text(text, encoding="utf-8")

path = Path("CHANGELOG.md")
text = path.read_text(encoding="utf-8")

entry = "*   **Spectrum Tuning Feedback**: Kept the RTA Spectrum alive during `TUNING...` / connecting states with a subtle ambient pulse, matching the oscilloscope's existing interstitial feedback.\n"

if "## [Unreleased]" in text:
    if "Spectrum Tuning Feedback" not in text:
        text = replace_once(
            text,
            "## [Unreleased]\n",
            "## [Unreleased]\n\n### Improved\n" + entry,
            "CHANGELOG Unreleased entry",
        )
else:
    text = replace_once(
        text,
        "---\n\n## [0.1.6]",
        "---\n\n## [Unreleased]\n\n### Improved\n" + entry + "\n---\n\n## [0.1.6]",
        "CHANGELOG Unreleased section",
    )

path.write_text(text, encoding="utf-8")
PY
