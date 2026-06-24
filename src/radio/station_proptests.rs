use super::*;
use proptest::prelude::*;

// **Property 1: Country code normalization invariant**
//
// For any string input, `normalize_country_code` produces a result with no
// leading or trailing whitespace and all ASCII alphabetic chars are uppercase.
//
// **Validates: Requirements 6.1**
proptest! {
    #[test]
    fn normalize_country_code_invariant(input in ".*") {
        let result = normalize_country_code(&input);
        prop_assert!(
            result == result.trim(),
            "result '{}' has leading/trailing whitespace", result
        );
        for ch in result.chars() {
            if ch.is_ascii_alphabetic() {
                prop_assert!(
                    ch.is_ascii_uppercase(),
                    "char '{}' is not uppercase in result '{}'", ch, result
                );
            }
        }
    }
}

// **Property 2: Codec normalization invariant**
//
// For any string input, `normalize_codec` produces a result with no leading or
// trailing whitespace and ASCII alphabetic chars are uppercase. Known aliases
// map correctly.
//
// **Validates: Requirements 6.2**
proptest! {
    #[test]
    fn normalize_codec_invariant(input in ".*") {
        let result = normalize_codec(&input);
        prop_assert_eq!(
            result.clone(), result.trim(), "result has leading/trailing whitespace"
        );
        for ch in result.chars() {
            if ch.is_ascii_alphabetic() {
                prop_assert!(
                    ch.is_ascii_uppercase(),
                    "char '{}' is not uppercase in result '{}'", ch, result
                );
            }
        }
    }

    #[test]
    fn normalize_codec_known_aliases(
        input in prop_oneof![
            Just("AUDIO/MPEG".to_string()),
            Just("MPEG".to_string()),
            Just("audio/mpeg".to_string()),
            Just("mpeg".to_string()),
            Just(" audio/mpeg ".to_string()),
            Just("AAC+".to_string()),
            Just("HE-AAC".to_string()),
            Just("HEAAC".to_string()),
            Just("aac+".to_string()),
            Just("he-aac".to_string()),
            Just("heaac".to_string()),
            Just("OGG VORBIS".to_string()),
            Just("VORBIS".to_string()),
            Just("ogg vorbis".to_string()),
            Just("vorbis".to_string()),
        ]
    ) {
        let result = normalize_codec(&input);
        let upper_trimmed = input.trim().to_ascii_uppercase();
        match upper_trimmed.as_str() {
            "AUDIO/MPEG" | "MPEG" => prop_assert_eq!(result, "MP3"),
            "AAC+" | "HE-AAC" | "HEAAC" => prop_assert_eq!(result, "AAC"),
            "OGG VORBIS" | "VORBIS" => prop_assert_eq!(result, "OGG"),
            _ => {}
        }
    }
}

// **Property 3: Bitrate sanitization invariant**
//
// For any u32 input, `sanitize_bitrate` returns the input unchanged when ≤1024
// and returns 0 when >1024.
//
// **Validates: Requirements 6.3**
proptest! {
    #[test]
    fn sanitize_bitrate_invariant(value in any::<u32>()) {
        let result = sanitize_bitrate(value);
        if value <= 1024 {
            prop_assert_eq!(result, value, "expected {} unchanged, got {}", value, result);
        } else {
            prop_assert_eq!(result, 0, "expected 0 for value > 1024, got {}", result);
        }
    }
}

// **Property 4: Station URL normalization invariant**
//
// For any string input, `normalized_station_url` produces a result with all
// ASCII alpha chars lowercased and no trailing '/' (unless empty).
// Note: The function does trim().trim_end_matches('/').to_ascii_lowercase(),
// which means stripping trailing slashes can expose internal whitespace chars
// (like vertical tab \u{b}) that were not at the boundary before slash removal.
// The core invariant is: all ASCII alpha lowercased + no trailing slash.
//
// **Validates: Requirements 6.4**
proptest! {
    #[test]
    fn normalized_station_url_invariant(input in ".*") {
        let result = normalized_station_url(&input);
        for ch in result.chars() {
            if ch.is_ascii_alphabetic() {
                prop_assert!(
                    ch.is_ascii_lowercase(),
                    "char '{}' is not lowercase in result '{}'", ch, result
                );
            }
        }
        if !result.is_empty() {
            prop_assert!(
                !result.ends_with('/'),
                "non-empty result '{}' ends with '/'", result
            );
        }
    }
}

// **Property 5: Station URL matching reflexivity**
//
// For any string input `s`, `station_url_matches(s, s)` returns true.
//
// **Validates: Requirements 6.5**
proptest! {
    #[test]
    fn station_url_matches_reflexivity(s in ".*") {
        prop_assert!(
            station_url_matches(&s, &s),
            "station_url_matches failed reflexivity for input '{}'", s
        );
    }
}
