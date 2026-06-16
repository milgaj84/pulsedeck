//! Shared unicode-aware text helpers for the UI layer.

/// Number of visible characters (chars, not bytes).
pub fn visible_len(text: &str) -> usize {
    text.chars().count()
}

/// Hard truncate to at most `max_chars` characters (no ellipsis).
pub fn truncate_to_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Truncate to `max_chars`, appending an ellipsis when the value was shortened.
pub fn truncate_with_ellipsis(value: &str, max_chars: usize) -> String {
    let value_len = visible_len(value);
    if value_len <= max_chars {
        return value.to_string();
    }

    if max_chars <= 1 {
        return "…".to_string();
    }

    let mut truncated = value.chars().take(max_chars - 1).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_len_counts_chars() {
        assert_eq!(visible_len("abc"), 3);
        assert_eq!(visible_len("café"), 4);
    }

    #[test]
    fn truncate_to_chars_caps_length() {
        assert_eq!(truncate_to_chars("abcdef", 3), "abc");
        assert_eq!(truncate_to_chars("ab", 5), "ab");
    }

    #[test]
    fn ellipsis_only_when_shortened() {
        assert_eq!(truncate_with_ellipsis("abc", 5), "abc");
        assert_eq!(truncate_with_ellipsis("abcdef", 4), "abc…");
        assert_eq!(truncate_with_ellipsis("abcdef", 1), "…");
    }
}
