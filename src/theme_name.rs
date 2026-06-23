// ── Theme Registry ───────────────────────────────────────────────────

/// All available theme names.
///
/// This enum lives in a cross-cutting location so both the app-state layer
/// and the UI rendering layer can reference it without creating a circular
/// dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Retrowave,
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    CatppuccinLatte,
    Terminal,
}

impl ThemeName {
    /// All themes in display order.
    pub const ALL: &[ThemeName] = &[
        ThemeName::Retrowave,
        ThemeName::CatppuccinMocha,
        ThemeName::CatppuccinMacchiato,
        ThemeName::CatppuccinFrappe,
        ThemeName::CatppuccinLatte,
        ThemeName::Terminal,
    ];

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Retrowave => "Retrowave",
            ThemeName::CatppuccinMocha => "Catppuccin Mocha",
            ThemeName::CatppuccinMacchiato => "Catppuccin Macchiato",
            ThemeName::CatppuccinFrappe => "Catppuccin Frappé",
            ThemeName::CatppuccinLatte => "Catppuccin Latte",
            ThemeName::Terminal => "Terminal",
        }
    }

    /// Resolve from a persisted string key.
    pub fn from_key(key: &str) -> Self {
        match key {
            "Retrowave" => ThemeName::Retrowave,
            "CatppuccinMocha" => ThemeName::CatppuccinMocha,
            "CatppuccinMacchiato" => ThemeName::CatppuccinMacchiato,
            "CatppuccinFrappe" => ThemeName::CatppuccinFrappe,
            "CatppuccinLatte" => ThemeName::CatppuccinLatte,
            "Terminal" => ThemeName::Terminal,
            _ => ThemeName::Retrowave,
        }
    }

    /// Serializable key for persistence.
    pub fn key(self) -> &'static str {
        match self {
            ThemeName::Retrowave => "Retrowave",
            ThemeName::CatppuccinMocha => "CatppuccinMocha",
            ThemeName::CatppuccinMacchiato => "CatppuccinMacchiato",
            ThemeName::CatppuccinFrappe => "CatppuccinFrappe",
            ThemeName::CatppuccinLatte => "CatppuccinLatte",
            ThemeName::Terminal => "Terminal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_key_valid_keys() {
        assert_eq!(ThemeName::from_key("Retrowave"), ThemeName::Retrowave);
        assert_eq!(
            ThemeName::from_key("CatppuccinMocha"),
            ThemeName::CatppuccinMocha
        );
        assert_eq!(
            ThemeName::from_key("CatppuccinMacchiato"),
            ThemeName::CatppuccinMacchiato
        );
        assert_eq!(
            ThemeName::from_key("CatppuccinFrappe"),
            ThemeName::CatppuccinFrappe
        );
        assert_eq!(
            ThemeName::from_key("CatppuccinLatte"),
            ThemeName::CatppuccinLatte
        );
        assert_eq!(ThemeName::from_key("Terminal"), ThemeName::Terminal);
    }

    #[test]
    fn test_from_key_unknown_defaults_to_retrowave() {
        assert_eq!(ThemeName::from_key(""), ThemeName::Retrowave);
        assert_eq!(ThemeName::from_key("unknown"), ThemeName::Retrowave);
        assert_eq!(ThemeName::from_key("catppuccin"), ThemeName::Retrowave);
    }

    #[test]
    fn test_from_key_round_trip() {
        for &variant in ThemeName::ALL {
            assert_eq!(ThemeName::from_key(variant.key()), variant);
        }
    }

    #[test]
    fn test_label_non_empty_with_alphabetic() {
        for &variant in ThemeName::ALL {
            let label = variant.label();
            assert!(
                !label.is_empty(),
                "label for {:?} should not be empty",
                variant
            );
            assert!(
                label.chars().any(|c| c.is_alphabetic()),
                "label for {:?} should contain at least one alphabetic char",
                variant,
            );
        }
    }

    #[test]
    fn test_all_contains_six_elements() {
        assert_eq!(ThemeName::ALL.len(), 6);
    }
}
