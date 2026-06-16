use ratatui::style::Color;

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

// ── Palette Definitions ──────────────────────────────────────────────

pub(super) fn palette_retrowave() -> ThemePalette {
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

pub(super) fn palette_mocha() -> ThemePalette {
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

pub(super) fn palette_macchiato() -> ThemePalette {
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

pub(super) fn palette_frappe() -> ThemePalette {
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

pub(super) fn palette_latte() -> ThemePalette {
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

pub(super) fn palette_terminal() -> ThemePalette {
    ThemePalette {
        bg: Color::Reset,
        bg_highlight: Color::Black, // ANSI 0 — selection bg
        surface: Color::Reset,      // no scanline tint

        text_primary: Color::Gray, // ANSI 7 — normal fg
        text_dim: Color::DarkGray, // ANSI 8 — dim

        accent: Color::Magenta,                // ANSI 5 — borders/titles
        accent_secondary: Color::LightMagenta, // ANSI 13
        highlight: Color::Cyan,                // ANSI 6 — selection text
        warm: Color::Yellow,                   // ANSI 3 — connecting state

        success: Color::Green, // ANSI 2
        error: Color::Red,     // ANSI 1

        vol_filled: Color::Cyan,    // ANSI 6
        vol_empty: Color::DarkGray, // ANSI 8
    }
}
