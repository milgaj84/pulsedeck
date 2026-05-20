use ratatui::style::{Color, Modifier, Style};

// ── Retrowave Color Palette ──────────────────────────────────────────

/// Pure void black background
pub const BG: Color = Color::Rgb(0, 0, 0);

/// Neon magenta — primary accent
pub const NEON_MAGENTA: Color = Color::Rgb(255, 46, 151);

/// Hot pink — secondary accent
pub const HOT_PINK: Color = Color::Rgb(255, 106, 193);

/// Electric cyan — highlights
pub const NEON_CYAN: Color = Color::Rgb(0, 240, 255);

/// Deep purple — borders, subtle elements
pub const DEEP_PURPLE: Color = Color::Rgb(123, 47, 190);

/// Sunset orange — warm accent
pub const SUNSET_ORANGE: Color = Color::Rgb(255, 140, 66);

/// Soft lavender white — primary text
pub const TEXT_PRIMARY: Color = Color::Rgb(224, 212, 255);

/// Dim text — secondary, hints
pub const TEXT_DIM: Color = Color::Rgb(120, 100, 160);

/// Scanline dark — alternating row tint
pub const SCANLINE_DIM: Color = Color::Rgb(10, 8, 18);

/// Green for "LIVE" indicators
pub const NEON_GREEN: Color = Color::Rgb(57, 255, 20);

/// Error/warning red
pub const ERROR_RED: Color = Color::Rgb(255, 60, 60);

// ── Style Helpers ────────────────────────────────────────────────────

/// Default text style
pub fn text() -> Style {
    Style::default().fg(TEXT_PRIMARY).bg(BG)
}

/// Dim/secondary text
pub fn dim() -> Style {
    Style::default().fg(TEXT_DIM).bg(BG)
}

/// Neon magenta accent text
pub fn neon() -> Style {
    Style::default().fg(NEON_MAGENTA).bg(BG).add_modifier(Modifier::BOLD)
}

/// Cyan highlight text
pub fn cyan() -> Style {
    Style::default().fg(NEON_CYAN).bg(BG)
}

/// Selected/highlighted item in lists
pub fn selected() -> Style {
    Style::default()
        .fg(NEON_CYAN)
        .bg(Color::Rgb(20, 15, 40))
        .add_modifier(Modifier::BOLD)
}

/// The playing station indicator
pub fn playing() -> Style {
    Style::default().fg(NEON_GREEN).bg(BG).add_modifier(Modifier::BOLD)
}

/// Error style
pub fn error() -> Style {
    Style::default().fg(ERROR_RED).bg(BG)
}

/// Border style for blocks
pub fn border() -> Style {
    Style::default().fg(DEEP_PURPLE).bg(BG)
}

/// Title style for blocks
pub fn title() -> Style {
    Style::default().fg(NEON_MAGENTA).bg(BG).add_modifier(Modifier::BOLD)
}

/// Scanline effect — slightly darker background for alternating rows
pub fn scanline() -> Style {
    Style::default().fg(TEXT_PRIMARY).bg(SCANLINE_DIM)
}

/// Volume bar filled style
pub fn vol_filled() -> Style {
    Style::default().fg(NEON_CYAN).bg(BG)
}

/// Volume bar empty style  
pub fn vol_empty() -> Style {
    Style::default().fg(Color::Rgb(40, 30, 60)).bg(BG)
}
