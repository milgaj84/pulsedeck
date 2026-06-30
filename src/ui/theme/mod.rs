use ratatui::style::{Color, Modifier, Style};
use std::sync::RwLock;

mod name;
mod palette;

pub use name::ThemeName;
pub use palette::ThemePalette;

use palette::palette_retrowave;

// ── Global Active Palette ────────────────────────────────────────────

static ACTIVE_PALETTE: RwLock<Option<ThemePalette>> = RwLock::new(None);

/// Initialize or change the active theme.
pub fn set_active(name: ThemeName) {
    let palette = name.palette();
    if let Ok(mut lock) = ACTIVE_PALETTE.write() {
        *lock = Some(palette);
    }
}

/// Read the current active palette (falls back to Retrowave).
fn active() -> ThemePalette {
    ACTIVE_PALETTE
        .read()
        .ok()
        .and_then(|lock| (*lock).clone())
        .unwrap_or_else(palette_retrowave)
}

/// Read the current active palette as a public accessor for direct field use.
pub fn active_palette() -> ThemePalette {
    active()
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

/// Non-selected list item on even rows: dimmed text + default bg
pub fn dim_row_even() -> Style {
    let p = active();
    Style::default().fg(p.text_dim).bg(p.bg)
}

/// Non-selected list item on odd rows: dimmed text + surface bg (alternating)
pub fn dim_row_odd() -> Style {
    let p = active();
    Style::default().fg(p.text_dim).bg(p.surface)
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

/// Health dot: healthy (green/success)
pub fn health_healthy() -> Style {
    let p = active();
    Style::default().fg(p.health_healthy)
}

/// Health dot: flaky (yellow/warm)
pub fn health_flaky() -> Style {
    let p = active();
    Style::default().fg(p.health_flaky)
}

/// Health dot: failed (red/error)
pub fn health_failed() -> Style {
    let p = active();
    Style::default().fg(p.health_failed)
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
