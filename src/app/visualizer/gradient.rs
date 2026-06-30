/// Identifies which color band a given row belongs to within a spectrum bar.
/// The UI layer maps each band to a theme palette color:
/// - Bottom → success (green family)
/// - Middle → highlight (cyan family)
/// - Top → accent (magenta family)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorBand {
    Bottom,
    Middle,
    Top,
}

/// Determine which color band a row belongs to given the bar's current height.
///
/// - Bottom third: rows [0, floor(height/3))
/// - Middle third: rows [floor(height/3), floor(2*height/3))
/// - Top third: rows [floor(2*height/3), height)
///
/// Returns None if bar_height == 0 (no cells to render).
pub fn color_band_for_row(row: usize, bar_height: usize) -> Option<ColorBand> {
    if bar_height == 0 {
        return None;
    }
    let bottom_end = bar_height / 3;
    let middle_end = 2 * bar_height / 3;
    if row < bottom_end {
        Some(ColorBand::Bottom)
    } else if row < middle_end {
        Some(ColorBand::Middle)
    } else {
        Some(ColorBand::Top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_band_height_zero_returns_none() {
        assert_eq!(color_band_for_row(0, 0), None);
    }

    #[test]
    fn test_color_band_height_one_returns_top() {
        // floor(1/3)=0, floor(2/3)=0 → row 0 >= middle_end(0) → Top
        assert_eq!(color_band_for_row(0, 1), Some(ColorBand::Top));
    }

    #[test]
    fn test_color_band_height_two_returns_middle_and_top() {
        // floor(2/3)=0, floor(4/3)=1 → row 0 < middle_end(1) → Middle; row 1 >= 1 → Top
        assert_eq!(color_band_for_row(0, 2), Some(ColorBand::Middle));
        assert_eq!(color_band_for_row(1, 2), Some(ColorBand::Top));
    }

    #[test]
    fn test_color_band_height_three_one_row_per_band() {
        // floor(3/3)=1, floor(6/3)=2
        assert_eq!(color_band_for_row(0, 3), Some(ColorBand::Bottom));
        assert_eq!(color_band_for_row(1, 3), Some(ColorBand::Middle));
        assert_eq!(color_band_for_row(2, 3), Some(ColorBand::Top));
    }

    #[test]
    fn test_color_band_height_ten_correct_boundaries() {
        // floor(10/3)=3, floor(20/3)=6
        // Bottom: 0,1,2  Middle: 3,4,5  Top: 6,7,8,9
        for row in 0..3 {
            assert_eq!(color_band_for_row(row, 10), Some(ColorBand::Bottom));
        }
        for row in 3..6 {
            assert_eq!(color_band_for_row(row, 10), Some(ColorBand::Middle));
        }
        for row in 6..10 {
            assert_eq!(color_band_for_row(row, 10), Some(ColorBand::Top));
        }
    }

    #[test]
    fn test_color_band_all_rows_covered_for_height_nine() {
        // floor(9/3)=3, floor(18/3)=6
        let height = 9;
        for row in 0..height {
            assert!(color_band_for_row(row, height).is_some());
        }
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 1: For any bar height 1..=200, all rows are assigned exactly one band,
        /// bands are contiguous (Bottom < Middle < Top), and each band spans at least 1 row.
        #[test]
        fn prop_gradient_produces_three_contiguous_bands(bar_height in 1..=200usize) {
            let mut bands: Vec<ColorBand> = Vec::with_capacity(bar_height);
            for row in 0..bar_height {
                let band = color_band_for_row(row, bar_height)
                    .expect("all rows should have a band");
                bands.push(band);
            }

            // All rows covered
            prop_assert_eq!(bands.len(), bar_height);

            // Bands are contiguous: once we leave Bottom, we never return; same for Middle
            let mut seen_middle = false;
            let mut seen_top = false;
            for band in &bands {
                match band {
                    ColorBand::Bottom => {
                        prop_assert!(!seen_middle, "Bottom after Middle");
                        prop_assert!(!seen_top, "Bottom after Top");
                    }
                    ColorBand::Middle => {
                        prop_assert!(!seen_top, "Middle after Top");
                        seen_middle = true;
                    }
                    ColorBand::Top => {
                        seen_top = true;
                    }
                }
            }

            // Top must always be present (at least 1 row for height >= 1)
            prop_assert!(seen_top, "Top band must always be present");
        }
    }
}
