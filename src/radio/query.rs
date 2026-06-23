const SEARCH_MIN_CHARS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchField {
    Name,
    Tag,
    Country,
    CountryCode,
    Language,
    Codec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchPrefixHelp {
    pub prefix: &'static str,
    pub aliases: &'static [&'static str],
    pub field: SearchField,
    pub api_param: &'static str,
    pub label_prefix: &'static str,
    pub label: &'static str,
    pub example: &'static str,
}

pub const SEARCH_PREFIX_HELP: &[SearchPrefixHelp] = &[
    SearchPrefixHelp {
        prefix: "name",
        aliases: &["station"],
        field: SearchField::Name,
        api_param: "name",
        label_prefix: "name",
        label: "station name",
        example: "name:lofi",
    },
    SearchPrefixHelp {
        prefix: "tag",
        aliases: &["genre"],
        field: SearchField::Tag,
        api_param: "tag",
        label_prefix: "tag",
        label: "genre or tag",
        example: "tag:ambient",
    },
    SearchPrefixHelp {
        prefix: "country",
        aliases: &["cc"],
        field: SearchField::Country,
        api_param: "country",
        label_prefix: "country",
        label: "country name or two-letter code",
        example: "country:BA",
    },
    SearchPrefixHelp {
        prefix: "lang",
        aliases: &["language"],
        field: SearchField::Language,
        api_param: "language",
        label_prefix: "lang",
        label: "station language",
        example: "lang:english",
    },
    SearchPrefixHelp {
        prefix: "codec",
        aliases: &["format"],
        field: SearchField::Codec,
        api_param: "codec",
        label_prefix: "codec",
        label: "stream codec",
        example: "codec:mp3",
    },
];

pub fn prefix_examples_inline() -> String {
    let examples = SEARCH_PREFIX_HELP
        .iter()
        .map(|help| help.example)
        .collect::<Vec<_>>()
        .join(", ");
    format!("try {examples}")
}

pub fn known_search_prefix(prefix: &str) -> bool {
    search_prefix(prefix).is_some()
}

fn search_prefix(prefix: &str) -> Option<&'static SearchPrefixHelp> {
    let prefix = prefix.trim();
    SEARCH_PREFIX_HELP.iter().find(|help| {
        help.prefix.eq_ignore_ascii_case(prefix)
            || help
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(prefix))
    })
}

pub fn query_prefix(raw: &str) -> Option<String> {
    raw.trim()
        .split_once(':')
        .map(|(prefix, _)| prefix.trim().to_string())
        .filter(|prefix| !prefix.is_empty())
}

pub fn has_unknown_prefix(raw: &str) -> bool {
    query_prefix(raw).is_some_and(|prefix| !known_search_prefix(&prefix))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationSearchQuery {
    raw: String,
    value: String,
    field: SearchField,
}

impl StationSearchQuery {
    pub fn parse(raw: &str) -> Self {
        let raw_trimmed = raw.trim().to_string();
        let Some((prefix, value)) = raw_trimmed.split_once(':') else {
            return Self::plain(raw_trimmed);
        };

        let value = value.trim().to_string();
        let Some(help) = search_prefix(prefix) else {
            return Self::plain(raw_trimmed);
        };

        let (field, value) = field_and_value_for_prefix(help, &value);
        Self::with_field(raw_trimmed, field, value)
    }

    fn plain(value: String) -> Self {
        Self {
            raw: value.clone(),
            value,
            field: SearchField::Name,
        }
    }

    fn with_field(raw: String, field: SearchField, value: String) -> Self {
        Self { raw, value, field }
    }

    pub fn is_short(&self) -> bool {
        self.value.trim().chars().count() < SEARCH_MIN_CHARS
    }

    pub fn api_params(&self) -> Vec<(&'static str, String)> {
        let mut params = vec![
            ("hidebroken", "true".to_string()),
            ("order", "clickcount".to_string()),
            ("reverse", "true".to_string()),
            ("limit", "40".to_string()),
        ];

        params.push((api_param_for_field(self.field), self.value.clone()));
        params
    }

    pub fn display_label(&self) -> String {
        format!("{}:{}", label_prefix_for_field(self.field), self.value)
    }

    pub fn field(&self) -> SearchField {
        self.field
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

fn field_and_value_for_prefix(help: &SearchPrefixHelp, value: &str) -> (SearchField, String) {
    if help.field == SearchField::Country && is_country_code(value) {
        (SearchField::CountryCode, value.to_ascii_uppercase())
    } else {
        (help.field, value.to_string())
    }
}

fn api_param_for_field(field: SearchField) -> &'static str {
    match field {
        SearchField::CountryCode => "countrycode",
        _ => prefix_help_for_field(field).api_param,
    }
}

fn label_prefix_for_field(field: SearchField) -> &'static str {
    match field {
        SearchField::CountryCode => "country",
        _ => prefix_help_for_field(field).label_prefix,
    }
}

fn prefix_help_for_field(field: SearchField) -> &'static SearchPrefixHelp {
    SEARCH_PREFIX_HELP
        .iter()
        .find(|help| help.field == field)
        .expect("every search field except CountryCode has prefix metadata")
}

fn is_country_code(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() == 2 && trimmed.chars().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param_value(query: &StationSearchQuery, key: &str) -> Option<String> {
        query
            .api_params()
            .into_iter()
            .find(|(param, _)| *param == key)
            .map(|(_, value)| value)
    }

    #[test]
    fn plain_search_maps_to_name() {
        let query = StationSearchQuery::parse(" lofi ");
        assert_eq!(query.field(), SearchField::Name);
        assert_eq!(query.value(), "lofi");
        assert_eq!(param_value(&query, "name").as_deref(), Some("lofi"));
    }

    #[test]
    fn tag_prefix_maps_to_tag() {
        let query = StationSearchQuery::parse("tag:ambient drone");
        assert_eq!(query.field(), SearchField::Tag);
        assert_eq!(param_value(&query, "tag").as_deref(), Some("ambient drone"));
    }

    #[test]
    fn genre_prefix_maps_to_tag() {
        let query = StationSearchQuery::parse("genre:jazz");
        assert_eq!(query.field(), SearchField::Tag);
        assert_eq!(param_value(&query, "tag").as_deref(), Some("jazz"));
    }

    #[test]
    fn country_two_letters_maps_to_country_code() {
        let query = StationSearchQuery::parse("country:ba");
        assert_eq!(query.field(), SearchField::CountryCode);
        assert_eq!(query.value(), "BA");
        assert_eq!(param_value(&query, "countrycode").as_deref(), Some("BA"));
        assert_eq!(query.display_label(), "country:BA");
    }

    #[test]
    fn country_long_value_maps_to_country() {
        let query = StationSearchQuery::parse("country:Bosnia");
        assert_eq!(query.field(), SearchField::Country);
        assert_eq!(param_value(&query, "country").as_deref(), Some("Bosnia"));
    }

    #[test]
    fn language_aliases_map_to_language() {
        assert_eq!(
            StationSearchQuery::parse("lang:english").field(),
            SearchField::Language
        );
        assert_eq!(
            StationSearchQuery::parse("language:serbian").field(),
            SearchField::Language
        );
    }

    #[test]
    fn codec_prefix_maps_to_codec() {
        let query = StationSearchQuery::parse("codec:mp3");
        assert_eq!(query.field(), SearchField::Codec);
        assert_eq!(param_value(&query, "codec").as_deref(), Some("mp3"));
    }

    #[test]
    fn known_search_prefix_uses_metadata_and_aliases() {
        for help in SEARCH_PREFIX_HELP {
            assert!(known_search_prefix(help.prefix));
            for alias in help.aliases {
                assert!(known_search_prefix(alias));
            }
        }
        assert!(!known_search_prefix("mood"));
    }

    #[test]
    fn prefix_examples_inline_is_generated_from_metadata() {
        let examples = prefix_examples_inline();

        for help in SEARCH_PREFIX_HELP {
            assert!(examples.contains(help.example));
        }
        assert!(examples.starts_with("try "));
    }

    #[test]
    fn unknown_prefix_remains_plain_search() {
        let query = StationSearchQuery::parse("mood:rain");
        assert_eq!(query.field(), SearchField::Name);
        assert_eq!(query.value(), "mood:rain");
    }

    #[test]
    fn empty_prefix_value_is_short() {
        assert!(StationSearchQuery::parse("tag:").is_short());
        assert!(StationSearchQuery::parse("tag:a").is_short());
        assert!(!StationSearchQuery::parse("tag:am").is_short());
    }

    #[test]
    fn api_params_include_safe_defaults() {
        let query = StationSearchQuery::parse("lofi");
        let params = query.api_params();
        assert!(params.contains(&("hidebroken", "true".to_string())));
        assert!(params.contains(&("order", "clickcount".to_string())));
        assert!(params.contains(&("reverse", "true".to_string())));
        assert!(params.contains(&("limit", "40".to_string())));
    }
}
