pub use crate::theme_name::ThemeName;

use super::palette::{
    palette_frappe, palette_latte, palette_macchiato, palette_mocha, palette_retrowave,
    palette_terminal, ThemePalette,
};

/// Extension trait that associates `ThemeName` with its UI palette.
/// This keeps the palette dependency inside the UI layer.
impl ThemeName {
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
