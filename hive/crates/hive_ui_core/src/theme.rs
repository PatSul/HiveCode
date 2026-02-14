use gpui::{Hsla, Pixels, SharedString, hsla, px};

/// Complete design system with all color tokens, typography, spacing, and radii.
/// Fluent Design (Windows 11) + warm rustic dark palette.
pub struct HiveTheme {
    // Base (warm rustic dark)
    pub bg_primary: Hsla,
    pub bg_secondary: Hsla,
    pub bg_tertiary: Hsla,
    pub bg_surface: Hsla,

    // Accent (modern contrast colors)
    pub accent_aqua: Hsla,
    pub accent_powder: Hsla,
    pub accent_cyan: Hsla,
    pub accent_green: Hsla,
    pub accent_red: Hsla,
    pub accent_yellow: Hsla,
    pub accent_pink: Hsla,

    // Text
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_muted: Hsla,
    pub text_on_accent: Hsla,

    // Borders
    pub border: Hsla,
    pub border_focus: Hsla,

    // Typography
    pub font_ui: SharedString,
    pub font_mono: SharedString,
    pub font_size_xs: Pixels,
    pub font_size_sm: Pixels,
    pub font_size_base: Pixels,
    pub font_size_lg: Pixels,
    pub font_size_xl: Pixels,
    pub font_size_2xl: Pixels,

    // Spacing (4px grid)
    pub space_1: Pixels,
    pub space_2: Pixels,
    pub space_3: Pixels,
    pub space_4: Pixels,
    pub space_6: Pixels,
    pub space_8: Pixels,

    // Radii
    pub radius_sm: Pixels,
    pub radius_md: Pixels,
    pub radius_lg: Pixels,
    pub radius_xl: Pixels,
    pub radius_full: Pixels,
}

impl HiveTheme {
    pub fn dark() -> Self {
        Self {
            // Base palette
            bg_primary: hex_to_hsla(0x12, 0x16, 0x2A),
            bg_secondary: hex_to_hsla(0x15, 0x1B, 0x34),
            bg_tertiary: hex_to_hsla(0x1F, 0x2A, 0x4A),
            bg_surface: hex_to_hsla(0x18, 0x1F, 0x3A),

            // Accents
            accent_aqua: hex_to_hsla(0x00, 0xFF, 0xF0),
            accent_powder: hex_to_hsla(0xB8, 0xE3, 0xE9),
            accent_cyan: hex_to_hsla(0x00, 0xD4, 0xFF),
            accent_green: hex_to_hsla(0xA6, 0xE3, 0xA1),
            accent_red: hex_to_hsla(0xF3, 0x8B, 0xA8),
            accent_yellow: hex_to_hsla(0xF9, 0xE2, 0xAF),
            accent_pink: hex_to_hsla(0xF5, 0xC2, 0xE7),

            // Text
            text_primary: hex_to_hsla(0xE7, 0xEC, 0xFF),
            text_secondary: hex_to_hsla(0xBE, 0xC7, 0xE7),
            text_muted: hex_to_hsla(0x8A, 0x96, 0xBC),
            text_on_accent: hex_to_hsla(0x0A, 0x0A, 0x14),

            // Borders
            border: hex_to_hsla(0x2D, 0x3A, 0x5E),
            border_focus: hsla(174.0 / 360.0, 1.0, 0.5, 0.5),

            // Typography
            font_ui: SharedString::from("Inter"),
            font_mono: SharedString::from("JetBrains Mono"),
            font_size_xs: px(11.0),
            font_size_sm: px(12.0),
            font_size_base: px(14.0),
            font_size_lg: px(16.0),
            font_size_xl: px(20.0),
            font_size_2xl: px(28.0),

            // Spacing (4px grid)
            space_1: px(4.0),
            space_2: px(8.0),
            space_3: px(12.0),
            space_4: px(16.0),
            space_6: px(24.0),
            space_8: px(32.0),

            // Radii
            radius_sm: px(4.0),
            radius_md: px(8.0),
            radius_lg: px(12.0),
            radius_xl: px(16.0),
            radius_full: px(9999.0),
        }
    }
}

/// Convert RGB bytes to GPUI Hsla color.
fn hex_to_hsla(r: u8, g: u8, b: u8) -> Hsla {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let l = (max + min) / 2.0;

    if delta == 0.0 {
        return hsla(0.0, 0.0, l, 1.0);
    }

    let s = if l < 0.5 {
        delta / (max + min)
    } else {
        delta / (2.0 - max - min)
    };

    let h = if max == rf {
        ((gf - bf) / delta + if gf < bf { 6.0 } else { 0.0 }) / 6.0
    } else if max == gf {
        ((bf - rf) / delta + 2.0) / 6.0
    } else {
        ((rf - gf) / delta + 4.0) / 6.0
    };

    hsla(h, s, l, 1.0)
}
