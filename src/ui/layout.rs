/// Minimum terminal height to allow vertical inner margins on overlays.
const MIN_HEIGHT_FOR_VERTICAL_MARGIN: u16 = 30;

/// Minimum inner content height before vertical margins are applied.
const MIN_CONTENT_HEIGHT_FOR_MARGIN: u16 = 6;

/// Compute vertical inner margin for overlays.
/// Returns 0 if terminal height < 30 or inner content height <= 6, else 1.
pub fn overlay_vertical_margin(terminal_height: u16, inner_content_height: u16) -> u16 {
    if terminal_height < MIN_HEIGHT_FOR_VERTICAL_MARGIN
        || inner_content_height <= MIN_CONTENT_HEIGHT_FOR_MARGIN
    {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_terminal_returns_zero() {
        assert_eq!(overlay_vertical_margin(24, 20), 0);
        assert_eq!(overlay_vertical_margin(29, 20), 0);
    }

    #[test]
    fn test_small_content_returns_zero() {
        assert_eq!(overlay_vertical_margin(40, 5), 0);
        assert_eq!(overlay_vertical_margin(40, 6), 0);
    }

    #[test]
    fn test_normal_case_returns_one() {
        assert_eq!(overlay_vertical_margin(30, 7), 1);
        assert_eq!(overlay_vertical_margin(40, 20), 1);
    }

    #[test]
    fn test_boundary_exactly_30_height() {
        assert_eq!(overlay_vertical_margin(30, 7), 1);
    }

    #[test]
    fn test_boundary_exactly_6_content() {
        assert_eq!(overlay_vertical_margin(40, 6), 0);
        assert_eq!(overlay_vertical_margin(40, 7), 1);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 2: Overlay vertical margin conditional logic.
        #[test]
        fn prop_overlay_vertical_margin(
            terminal_height in 1..=100u16,
            content_height in 0..=50u16
        ) {
            let result = overlay_vertical_margin(terminal_height, content_height);
            if terminal_height < 30 {
                prop_assert_eq!(result, 0, "should be 0 for terminal_height < 30");
            } else if content_height <= 6 {
                prop_assert_eq!(result, 0, "should be 0 for content_height <= 6");
            } else {
                prop_assert_eq!(result, 1, "should be 1 otherwise");
            }
        }
    }
}
