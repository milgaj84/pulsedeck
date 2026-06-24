pub(super) fn truncate_station_name(value: &str, query: Option<&str>, max_chars: usize) -> String {
    match query.map(str::trim).filter(|query| !query.is_empty()) {
        Some(query) => adaptive_search_truncate(value, query, max_chars),
        None => crate::text::truncate_with_ellipsis(value, max_chars),
    }
}

fn adaptive_search_truncate(value: &str, query: &str, max_chars: usize) -> String {
    let value_len = crate::text::visible_len(value);
    if value_len <= max_chars {
        return value.to_string();
    }

    if max_chars <= 1 {
        return "…".to_string();
    }

    let Some(match_start) = find_case_insensitive_char_index(value, query) else {
        return crate::text::truncate_with_ellipsis(value, max_chars);
    };

    if match_start < max_chars.saturating_sub(1) {
        return crate::text::truncate_with_ellipsis(value, max_chars);
    }

    let available = max_chars.saturating_sub(2);
    if available == 0 {
        return "…".to_string();
    }

    let query_len = crate::text::visible_len(query).max(1);
    let context_before = available.saturating_sub(query_len) / 2;
    let start = match_start
        .saturating_sub(context_before)
        .min(value_len.saturating_sub(available));
    let end = start + available;

    if start == 0 {
        return crate::text::truncate_with_ellipsis(value, max_chars);
    }

    if end >= value_len {
        let tail_width = max_chars.saturating_sub(1);
        let tail_start = value_len.saturating_sub(tail_width);
        let tail = value.chars().skip(tail_start).collect::<String>();
        return format!("…{tail}");
    }

    let window = value
        .chars()
        .skip(start)
        .take(available)
        .collect::<String>();
    format!("…{window}…")
}

fn find_case_insensitive_char_index(value: &str, query: &str) -> Option<usize> {
    let value_lower = value.to_lowercase();
    let query_lower = query.to_lowercase();
    let byte_index = value_lower.find(&query_lower)?;
    Some(value_lower[..byte_index].chars().count())
}
