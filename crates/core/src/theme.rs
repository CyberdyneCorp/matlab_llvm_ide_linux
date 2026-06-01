//! Color palette, metrics, and code-token colors ported verbatim from the
//! macOS reference's `Theme/Theme.swift`. Kept GTK-free so view models and the
//! Cairo-based renderers (matrix heatmap, plots, flowchart) can share exact
//! colors, and so the values are unit-testable. The GTK chrome additionally
//! mirrors these into a CSS file at `crates/app/resources/theme.css`.

/// An sRGB color with 8-bit channels, constructed from a `0xRRGGBB` literal to
/// match the Swift `Color(hex:)` initializer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn hex(value: u32) -> Self {
        Rgb {
            r: ((value >> 16) & 0xFF) as u8,
            g: ((value >> 8) & 0xFF) as u8,
            b: (value & 0xFF) as u8,
        }
    }

    /// `#rrggbb` string for CSS / Pango markup.
    pub fn to_css(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Channels as 0.0..=1.0 floats for Cairo (`set_source_rgb`).
    pub fn to_unit(self) -> (f64, f64, f64) {
        (self.r as f64 / 255.0, self.g as f64 / 255.0, self.b as f64 / 255.0)
    }

    /// Linear sRGB interpolation: `t = 0` → `self`, `t = 1` → `other`.
    /// Used by the matrix viewer's cold→hot heatmap gradient. `t` is clamped.
    pub fn blend(self, other: Rgb, t: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        Rgb { r: lerp(self.r, other.r), g: lerp(self.g, other.g), b: lerp(self.b, other.b) }
    }
}

/// Window/chrome/panel surfaces and accent colors (`Theme.Palette`).
pub mod palette {
    use super::Rgb;
    pub const WINDOW_BACKGROUND: Rgb = Rgb::hex(0x121A26);
    pub const CHROME: Rgb = Rgb::hex(0x16202E);
    pub const PANEL: Rgb = Rgb::hex(0x1A2434);
    pub const PANEL_ALT: Rgb = Rgb::hex(0x1F2A3C);
    pub const EDITOR_BACKGROUND: Rgb = Rgb::hex(0x131C2A);
    pub const CARD: Rgb = Rgb::hex(0x213046);
    pub const BORDER: Rgb = Rgb::hex(0x2A3A52);
    pub const BORDER_SOFT: Rgb = Rgb::hex(0x223047);

    pub const TEXT_PRIMARY: Rgb = Rgb::hex(0xD3DCEA);
    pub const TEXT_SECONDARY: Rgb = Rgb::hex(0x8898AE);
    pub const TEXT_MUTED: Rgb = Rgb::hex(0x5E6C82);

    pub const ACCENT_ORANGE: Rgb = Rgb::hex(0xE08A45);
    pub const ACCENT_AMBER: Rgb = Rgb::hex(0xC97A3A);
    pub const ACCENT_GREEN: Rgb = Rgb::hex(0x5EBE6E);
    pub const ACCENT_GREEN_DEEP: Rgb = Rgb::hex(0x3F8F4E);
    pub const ACCENT_BLUE: Rgb = Rgb::hex(0x4FA3E3);
    pub const ACCENT_CYAN: Rgb = Rgb::hex(0x64C8D6);
    pub const ACCENT_RED: Rgb = Rgb::hex(0xE05B5B);
    pub const ACCENT_YELLOW: Rgb = Rgb::hex(0xE0C26A);
    pub const ACCENT_MAGENTA: Rgb = Rgb::hex(0xC678DD);

    pub const SELECTION: Rgb = Rgb::hex(0x244064);
    pub const HOVER: Rgb = Rgb::hex(0x223149);
}

/// Syntax-highlight token colors (`Theme.Code`).
pub mod code {
    use super::Rgb;
    pub const BACKGROUND: Rgb = super::palette::EDITOR_BACKGROUND;
    pub const PLAIN: Rgb = Rgb::hex(0xCBD3E1);
    pub const KEYWORD: Rgb = Rgb::hex(0xC678DD);
    pub const CONTROL: Rgb = Rgb::hex(0xE06C75);
    pub const NUMBER: Rgb = Rgb::hex(0x98C379);
    pub const STRING: Rgb = Rgb::hex(0xE0A06A);
    pub const COMMENT: Rgb = Rgb::hex(0x5E6C82);
    pub const FUNCTION: Rgb = Rgb::hex(0x61AFEF);
    pub const IDENTIFIER: Rgb = Rgb::hex(0xCBD3E1);
    pub const OPERATOR: Rgb = Rgb::hex(0xABB2BF);
    pub const LINE_NUMBER: Rgb = Rgb::hex(0x4A5870);
}

/// Layout metrics in logical pixels (`Theme.Metrics`).
pub mod metrics {
    pub const PANEL_HEADER_HEIGHT: i32 = 24;
    pub const TOOLBAR_HEIGHT: i32 = 124;
    pub const STATUS_BAR_HEIGHT: i32 = 22;
    pub const ACTIVITY_BAR_WIDTH: i32 = 56;
    pub const LEFT_SIDEBAR_WIDTH: i32 = 220;
    pub const WORKSPACE_COLUMN_WIDTH: i32 = 380;
    pub const PLOTS_COLUMN_WIDTH: i32 = 360;
    pub const BOTTOM_PANEL_HEIGHT: i32 = 220;
    pub const DETAILS_HEIGHT: i32 = 150;
    pub const CURRENT_FOLDER_HEIGHT: i32 = 56;
    pub const RADIUS: i32 = 4;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_splits_channels() {
        let c = Rgb::hex(0x121A26);
        assert_eq!((c.r, c.g, c.b), (0x12, 0x1A, 0x26));
    }

    #[test]
    fn to_css_roundtrips() {
        assert_eq!(palette::ACCENT_ORANGE.to_css(), "#e08a45");
        assert_eq!(Rgb::hex(0x000000).to_css(), "#000000");
        assert_eq!(Rgb::hex(0xFFFFFF).to_css(), "#ffffff");
    }

    #[test]
    fn to_unit_normalizes() {
        let (r, g, b) = Rgb::hex(0xFF8000).to_unit();
        assert!((r - 1.0).abs() < 1e-9);
        assert!((g - 0.5019607).abs() < 1e-3);
        assert!(b.abs() < 1e-9);
    }

    #[test]
    fn blend_endpoints_and_midpoint() {
        let cold = palette::ACCENT_BLUE;
        let hot = palette::ACCENT_RED;
        assert_eq!(cold.blend(hot, 0.0), cold);
        assert_eq!(cold.blend(hot, 1.0), hot);
        let mid = cold.blend(hot, 0.5);
        // midpoint is strictly between the two reds
        assert!(mid.r > cold.r && mid.r < hot.r);
    }

    #[test]
    fn blend_clamps_out_of_range_t() {
        let a = Rgb::hex(0x000000);
        let b = Rgb::hex(0xFFFFFF);
        assert_eq!(a.blend(b, -1.0), a);
        assert_eq!(a.blend(b, 2.0), b);
    }

    #[test]
    fn code_background_matches_editor() {
        assert_eq!(code::BACKGROUND, palette::EDITOR_BACKGROUND);
    }

    #[test]
    fn metrics_match_reference() {
        assert_eq!(metrics::ACTIVITY_BAR_WIDTH, 56);
        assert_eq!(metrics::LEFT_SIDEBAR_WIDTH, 220);
        assert_eq!(metrics::STATUS_BAR_HEIGHT, 22);
    }
}
