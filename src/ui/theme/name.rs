use super::palette::{
    palette_frappe, palette_latte, palette_macchiato, palette_mocha, palette_retrowave,
    palette_terminal, ThemePalette,
};

// ── Theme Registry ───────────────────────────────────────────────────

/// All available theme names.
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

    /// Get the palette for this theme.
    pub fn palette(self) -> ThemePalette {
        match self {
            ThemeName::Retrowave => palette_retrowave(),
            ThemeName::CatppuccinMocha => palette_mocha(),
            ThemeName::CatppuccinMacchiato => palette_macchiato(),
            ThemeName::CatppuccinFrappe => palette_frappe(),
            ThemeName::CatppuccinLatte => palette_latte(),
            ThemeName::Terminal => palette_terminal(),
        }
    }
}
