use ratatui::style::{Color, Modifier, Style};
use std::sync::RwLock;

// ── Semantic Theme Palette ───────────────────────────────────────────

/// A complete color palette mapped to semantic UI roles.
/// Every theme implements the exact same set of roles so UI code
/// never needs to reference raw color names.
#[derive(Debug, Clone)]
pub struct ThemePalette {
    // Backgrounds
    pub bg: Color,
    pub bg_highlight: Color,
    pub surface: Color,

    // Text
    pub text_primary: Color,
    pub text_dim: Color,

    // Accents (semantic UI roles)
    pub accent: Color,
    pub accent_secondary: Color,
    pub highlight: Color,
    pub warm: Color,

    // Status
    pub success: Color,
    pub error: Color,

    // Volume bar
    pub vol_filled: Color,
    pub vol_empty: Color,
}

// ── Theme Registry ───────────────────────────────────────────────────

/// All available theme names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeName {
    Retrowave,
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    CatppuccinLatte,
}

impl ThemeName {
    /// All themes in display order.
    pub const ALL: &[ThemeName] = &[
        ThemeName::Retrowave,
        ThemeName::CatppuccinMocha,
        ThemeName::CatppuccinMacchiato,
        ThemeName::CatppuccinFrappe,
        ThemeName::CatppuccinLatte,
    ];

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            ThemeName::Retrowave => "Retrowave",
            ThemeName::CatppuccinMocha => "Catppuccin Mocha",
            ThemeName::CatppuccinMacchiato => "Catppuccin Macchiato",
            ThemeName::CatppuccinFrappe => "Catppuccin Frappé",
            ThemeName::CatppuccinLatte => "Catppuccin Latte",
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
        }
    }
}

// ── Palette Definitions ──────────────────────────────────────────────

fn palette_retrowave() -> ThemePalette {
    ThemePalette {
        bg: Color::Rgb(0, 0, 0),
        bg_highlight: Color::Rgb(20, 15, 40),
        surface: Color::Rgb(10, 8, 18),

        text_primary: Color::Rgb(224, 212, 255),
        text_dim: Color::Rgb(120, 100, 160),

        accent: Color::Rgb(255, 46, 151),            // Neon magenta
        accent_secondary: Color::Rgb(255, 106, 193), // Hot pink
        highlight: Color::Rgb(0, 240, 255),          // Neon cyan
        warm: Color::Rgb(255, 140, 66),              // Sunset orange

        success: Color::Rgb(57, 255, 20), // Neon green
        error: Color::Rgb(255, 60, 60),

        vol_filled: Color::Rgb(0, 240, 255),
        vol_empty: Color::Rgb(40, 30, 60),
    }
}

fn palette_mocha() -> ThemePalette {
    ThemePalette {
        bg: Color::Rgb(30, 30, 46),           // Base
        bg_highlight: Color::Rgb(69, 71, 90), // Surface 1
        surface: Color::Rgb(49, 50, 68),      // Surface 0

        text_primary: Color::Rgb(205, 214, 244), // Text
        text_dim: Color::Rgb(166, 173, 200),     // Subtext 0

        accent: Color::Rgb(203, 166, 247),           // Mauve
        accent_secondary: Color::Rgb(245, 194, 231), // Pink
        highlight: Color::Rgb(116, 199, 236),        // Sapphire
        warm: Color::Rgb(250, 179, 135),             // Peach

        success: Color::Rgb(166, 227, 161), // Green
        error: Color::Rgb(243, 139, 168),   // Red

        vol_filled: Color::Rgb(137, 180, 250), // Blue
        vol_empty: Color::Rgb(88, 91, 112),    // Surface 2
    }
}

fn palette_macchiato() -> ThemePalette {
    ThemePalette {
        bg: Color::Rgb(36, 39, 58),            // Base
        bg_highlight: Color::Rgb(73, 77, 100), // Surface 1
        surface: Color::Rgb(54, 58, 79),       // Surface 0

        text_primary: Color::Rgb(202, 211, 245), // Text
        text_dim: Color::Rgb(165, 173, 203),     // Subtext 0

        accent: Color::Rgb(198, 160, 246),           // Mauve
        accent_secondary: Color::Rgb(245, 189, 230), // Pink
        highlight: Color::Rgb(125, 196, 228),        // Sapphire
        warm: Color::Rgb(245, 169, 127),             // Peach

        success: Color::Rgb(166, 218, 149), // Green
        error: Color::Rgb(237, 135, 150),   // Red

        vol_filled: Color::Rgb(138, 173, 244), // Blue
        vol_empty: Color::Rgb(91, 96, 120),    // Surface 2
    }
}

fn palette_frappe() -> ThemePalette {
    ThemePalette {
        bg: Color::Rgb(48, 52, 70),            // Base
        bg_highlight: Color::Rgb(81, 87, 109), // Surface 1
        surface: Color::Rgb(65, 69, 89),       // Surface 0

        text_primary: Color::Rgb(198, 208, 245), // Text
        text_dim: Color::Rgb(165, 173, 206),     // Subtext 0

        accent: Color::Rgb(202, 158, 230),           // Mauve
        accent_secondary: Color::Rgb(244, 184, 228), // Pink
        highlight: Color::Rgb(133, 193, 220),        // Sapphire
        warm: Color::Rgb(239, 159, 118),             // Peach

        success: Color::Rgb(166, 209, 137), // Green
        error: Color::Rgb(231, 130, 132),   // Red

        vol_filled: Color::Rgb(140, 170, 238), // Blue
        vol_empty: Color::Rgb(98, 104, 128),   // Surface 2
    }
}

fn palette_latte() -> ThemePalette {
    ThemePalette {
        bg: Color::Rgb(239, 241, 245),           // Base
        bg_highlight: Color::Rgb(188, 192, 204), // Surface 1
        surface: Color::Rgb(204, 208, 218),      // Surface 0

        text_primary: Color::Rgb(76, 79, 105), // Text
        text_dim: Color::Rgb(108, 111, 133),   // Subtext 0

        accent: Color::Rgb(136, 57, 239),            // Mauve
        accent_secondary: Color::Rgb(234, 118, 203), // Pink
        highlight: Color::Rgb(32, 159, 181),         // Sapphire
        warm: Color::Rgb(254, 100, 11),              // Peach

        success: Color::Rgb(64, 160, 43), // Green
        error: Color::Rgb(210, 15, 57),   // Red

        vol_filled: Color::Rgb(30, 102, 245), // Blue
        vol_empty: Color::Rgb(172, 176, 190), // Surface 2
    }
}

// ── Global Active Palette ────────────────────────────────────────────

static ACTIVE_PALETTE: RwLock<Option<ThemePalette>> = RwLock::new(None);

/// Initialize or change the active theme.
pub fn set_active(name: ThemeName) {
    let palette = name.palette();
    let mut lock = ACTIVE_PALETTE.write().unwrap();
    *lock = Some(palette);
}

/// Read the current active palette (falls back to Retrowave).
fn active() -> ThemePalette {
    let lock = ACTIVE_PALETTE.read().unwrap();
    lock.clone().unwrap_or_else(palette_retrowave)
}

// ── Semantic Color Accessors ─────────────────────────────────────────
// These replace the old `pub const` color values.
// UI files call these instead of referencing raw constants.

pub fn bg() -> Color {
    active().bg
}

/// Full-screen and overlay clear style routed through the active theme.
pub fn clear() -> Style {
    let p = active();
    Style::default().bg(p.bg)
}

pub fn surface_color() -> Color {
    active().surface
}
pub fn accent() -> Color {
    active().accent
}
pub fn accent_secondary() -> Color {
    active().accent_secondary
}
pub fn highlight() -> Color {
    active().highlight
}
pub fn warm() -> Color {
    active().warm
}

// ── Style Helpers ────────────────────────────────────────────────────
// These have the same signatures as before. All 118 existing call sites
// continue to work with zero changes.

/// Default text style
pub fn text() -> Style {
    let p = active();
    Style::default().fg(p.text_primary).bg(p.bg)
}

/// Dim/secondary text
pub fn dim() -> Style {
    let p = active();
    Style::default().fg(p.text_dim).bg(p.bg)
}

/// Primary accent text (bold)
pub fn neon() -> Style {
    let p = active();
    Style::default()
        .fg(p.accent)
        .bg(p.bg)
        .add_modifier(Modifier::BOLD)
}

/// Highlight text (secondary color)
pub fn cyan() -> Style {
    let p = active();
    Style::default().fg(p.highlight).bg(p.bg)
}

/// Selected/highlighted item in lists
pub fn selected() -> Style {
    let p = active();
    Style::default()
        .fg(p.highlight)
        .bg(p.bg_highlight)
        .add_modifier(Modifier::BOLD)
}

/// The playing station indicator
pub fn playing() -> Style {
    let p = active();
    Style::default()
        .fg(p.success)
        .bg(p.bg)
        .add_modifier(Modifier::BOLD)
}

/// Error style
pub fn error() -> Style {
    let p = active();
    Style::default().fg(p.error).bg(p.bg)
}

/// Border style for blocks
pub fn border() -> Style {
    let p = active();
    Style::default().fg(p.accent).bg(p.bg)
}

/// Title style for blocks
pub fn title() -> Style {
    let p = active();
    Style::default()
        .fg(p.accent)
        .bg(p.bg)
        .add_modifier(Modifier::BOLD)
}

/// Scanline effect — slightly different background for alternating rows
pub fn scanline() -> Style {
    let p = active();
    Style::default().fg(p.text_primary).bg(p.surface)
}

/// Volume bar filled style
pub fn vol_filled() -> Style {
    let p = active();
    Style::default().fg(p.vol_filled).bg(p.bg)
}

/// Volume bar empty style
pub fn vol_empty() -> Style {
    let p = active();
    Style::default().fg(p.vol_empty).bg(p.bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_key_roundtrip() {
        for &theme in ThemeName::ALL {
            let key = theme.key();
            let restored = ThemeName::from_key(key);
            assert_eq!(theme, restored, "Key roundtrip failed for {:?}", theme);
        }
    }

    #[test]
    fn test_theme_from_key_unknown_defaults_retrowave() {
        assert_eq!(
            ThemeName::from_key("NonExistentTheme"),
            ThemeName::Retrowave
        );
        assert_eq!(ThemeName::from_key(""), ThemeName::Retrowave);
    }

    #[test]
    fn test_all_themes_have_labels() {
        for &theme in ThemeName::ALL {
            assert!(!theme.label().is_empty());
        }
    }

    #[test]
    fn clear_style_uses_active_palette_background() {
        set_active(ThemeName::CatppuccinLatte);

        assert_eq!(clear().bg, Some(ThemeName::CatppuccinLatte.palette().bg));
    }
}
