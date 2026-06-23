/// Visualizer rendering modes with exhaustive matching.
///
/// Adding a new variant forces all match arms to be updated, preventing
/// silent fallthrough with wildcard patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerMode {
    Spectrum,
    RealOscilloscope,
    SimOscilloscope,
}

impl VisualizerMode {
    /// Total number of modes (used for serialization bounds checking).
    #[allow(dead_code)]
    pub const COUNT: usize = 3;

    /// Cycle to the next mode (wraps around).
    pub fn next(self) -> Self {
        match self {
            Self::Spectrum => Self::RealOscilloscope,
            Self::RealOscilloscope => Self::SimOscilloscope,
            Self::SimOscilloscope => Self::Spectrum,
        }
    }

    /// Convert from a persisted `usize` index (clamped to valid range).
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Spectrum,
            1 => Self::RealOscilloscope,
            _ => Self::SimOscilloscope,
        }
    }

    /// Convert to a `usize` index for persistence.
    pub fn to_index(self) -> usize {
        match self {
            Self::Spectrum => 0,
            Self::RealOscilloscope => 1,
            Self::SimOscilloscope => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_cycles_through_all_modes() {
        let mode = VisualizerMode::Spectrum;
        let mode = mode.next();
        assert_eq!(mode, VisualizerMode::RealOscilloscope);
        let mode = mode.next();
        assert_eq!(mode, VisualizerMode::SimOscilloscope);
        let mode = mode.next();
        assert_eq!(mode, VisualizerMode::Spectrum);
    }

    #[test]
    fn from_index_clamps_out_of_range() {
        assert_eq!(VisualizerMode::from_index(0), VisualizerMode::Spectrum);
        assert_eq!(VisualizerMode::from_index(1), VisualizerMode::RealOscilloscope);
        assert_eq!(VisualizerMode::from_index(2), VisualizerMode::SimOscilloscope);
        assert_eq!(VisualizerMode::from_index(99), VisualizerMode::SimOscilloscope);
    }

    #[test]
    fn to_index_roundtrips() {
        for mode in [VisualizerMode::Spectrum, VisualizerMode::RealOscilloscope, VisualizerMode::SimOscilloscope] {
            assert_eq!(VisualizerMode::from_index(mode.to_index()), mode);
        }
    }
}
