use crate::app::ActiveOverlay;
use crate::input_mode::InputMode;

/// Maximum characters to display from a search query in the breadcrumb.
const MAX_QUERY_DISPLAY_LEN: usize = 30;

/// Compute the breadcrumb navigation text from current application state.
///
/// Priority order:
/// 1. Active overlay name (takes precedence over all)
/// 2. Search mode + non-empty query (with result count if > 0)
/// 3. Search mode + empty query
/// 4. LibraryFilter mode
/// 5. CommandPalette mode
/// 6. Normal mode + genre selected (not "All")
/// 7. Normal mode fallback → "Library"
pub fn compute_breadcrumb(
    overlay: ActiveOverlay,
    input_mode: &InputMode,
    search_query: &str,
    selected_genre: Option<&str>,
    search_result_count: usize,
) -> String {
    if let Some(name) = overlay_display_name(overlay) {
        return name.to_string();
    }

    match input_mode {
        InputMode::Search => {
            if search_query.is_empty() {
                "Search".to_string()
            } else {
                let truncated = truncate_query(search_query);
                if search_result_count > 0 {
                    format!(
                        "Search > \"{}\" ({} results)",
                        truncated, search_result_count
                    )
                } else {
                    format!("Search > \"{}\"", truncated)
                }
            }
        }
        InputMode::LibraryFilter => "Library > Filter".to_string(),
        InputMode::CommandPalette => "Command Palette".to_string(),
        InputMode::Normal | InputMode::SleepTimer => match selected_genre {
            Some(genre) if genre != "All" => format!("Library > {}", genre),
            _ => "Library".to_string(),
        },
    }
}

fn overlay_display_name(overlay: ActiveOverlay) -> Option<&'static str> {
    match overlay {
        ActiveOverlay::None => None,
        ActiveOverlay::Help => Some("Help"),
        ActiveOverlay::Settings => Some("Settings"),
        ActiveOverlay::StationDetails => Some("Station Details"),
        ActiveOverlay::RecentTracks => Some("Recent Tracks"),
        ActiveOverlay::PlaybackDoctor => Some("Playback Doctor"),
        ActiveOverlay::SleepTimer => Some("Sleep Timer"),
        ActiveOverlay::Keybindings => Some("Keybindings"),
    }
}

fn truncate_query(query: &str) -> String {
    let chars: Vec<char> = query.chars().collect();
    if chars.len() <= MAX_QUERY_DISPLAY_LEN {
        query.to_string()
    } else {
        let truncated: String = chars[..MAX_QUERY_DISPLAY_LEN].iter().collect();
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_takes_precedence() {
        let result = compute_breadcrumb(
            ActiveOverlay::Help,
            &InputMode::Search,
            "query",
            Some("Jazz"),
            5,
        );
        assert_eq!(result, "Help");
    }

    #[test]
    fn test_each_overlay_name() {
        assert_eq!(
            compute_breadcrumb(ActiveOverlay::Settings, &InputMode::Normal, "", None, 0),
            "Settings"
        );
        assert_eq!(
            compute_breadcrumb(
                ActiveOverlay::StationDetails,
                &InputMode::Normal,
                "",
                None,
                0
            ),
            "Station Details"
        );
        assert_eq!(
            compute_breadcrumb(ActiveOverlay::RecentTracks, &InputMode::Normal, "", None, 0),
            "Recent Tracks"
        );
        assert_eq!(
            compute_breadcrumb(
                ActiveOverlay::PlaybackDoctor,
                &InputMode::Normal,
                "",
                None,
                0
            ),
            "Playback Doctor"
        );
        assert_eq!(
            compute_breadcrumb(ActiveOverlay::SleepTimer, &InputMode::Normal, "", None, 0),
            "Sleep Timer"
        );
        assert_eq!(
            compute_breadcrumb(ActiveOverlay::Keybindings, &InputMode::Normal, "", None, 0),
            "Keybindings"
        );
    }

    #[test]
    fn test_search_with_query_no_results() {
        let result =
            compute_breadcrumb(ActiveOverlay::None, &InputMode::Search, "ambient", None, 0);
        assert_eq!(result, "Search > \"ambient\"");
    }

    #[test]
    fn test_search_with_query_and_results() {
        let result =
            compute_breadcrumb(ActiveOverlay::None, &InputMode::Search, "ambient", None, 12);
        assert_eq!(result, "Search > \"ambient\" (12 results)");
    }

    #[test]
    fn test_search_empty_query() {
        let result = compute_breadcrumb(ActiveOverlay::None, &InputMode::Search, "", None, 0);
        assert_eq!(result, "Search");
    }

    #[test]
    fn test_search_query_at_max_length_no_truncation() {
        let query = "a".repeat(30);
        let result = compute_breadcrumb(ActiveOverlay::None, &InputMode::Search, &query, None, 0);
        assert_eq!(result, format!("Search > \"{}\"", query));
    }

    #[test]
    fn test_search_query_exceeds_max_truncated_with_ellipsis() {
        let query = "a".repeat(31);
        let result = compute_breadcrumb(ActiveOverlay::None, &InputMode::Search, &query, None, 0);
        let expected_truncated = format!("{}…", "a".repeat(30));
        assert_eq!(result, format!("Search > \"{}\"", expected_truncated));
    }

    #[test]
    fn test_library_filter_mode() {
        let result =
            compute_breadcrumb(ActiveOverlay::None, &InputMode::LibraryFilter, "", None, 0);
        assert_eq!(result, "Library > Filter");
    }

    #[test]
    fn test_command_palette_mode() {
        let result =
            compute_breadcrumb(ActiveOverlay::None, &InputMode::CommandPalette, "", None, 0);
        assert_eq!(result, "Command Palette");
    }

    #[test]
    fn test_normal_with_genre() {
        let result =
            compute_breadcrumb(ActiveOverlay::None, &InputMode::Normal, "", Some("Jazz"), 0);
        assert_eq!(result, "Library > Jazz");
    }

    #[test]
    fn test_normal_with_all_genre_shows_library() {
        let result =
            compute_breadcrumb(ActiveOverlay::None, &InputMode::Normal, "", Some("All"), 0);
        assert_eq!(result, "Library");
    }

    #[test]
    fn test_normal_no_genre_shows_library() {
        let result = compute_breadcrumb(ActiveOverlay::None, &InputMode::Normal, "", None, 0);
        assert_eq!(result, "Library");
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Property 7: For any search query string, the displayed query portion never exceeds 30 chars.
        #[test]
        fn prop_breadcrumb_query_truncation(query in ".*") {
            let result = compute_breadcrumb(
                ActiveOverlay::None,
                &InputMode::Search,
                &query,
                None,
                0,
            );
            // Extract the quoted portion: Search > "..."
            if let Some(start) = result.find('"') {
                let after_quote = &result[start + 1..];
                if let Some(end) = after_quote.rfind('"') {
                    let displayed_query = &after_quote[..end];
                    let char_count = displayed_query.chars().count();
                    prop_assert!(char_count <= MAX_QUERY_DISPLAY_LEN + 1,
                        "displayed query too long: {} chars", char_count);
                }
            }
        }
    }
}
