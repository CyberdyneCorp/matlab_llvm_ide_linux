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

    /// WCAG relative luminance (0.0 black … 1.0 white).
    pub fn relative_luminance(self) -> f64 {
        fn lin(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG contrast ratio between two colors (1.0 = identical … 21.0 = black/white).
    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.relative_luminance(), other.relative_luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
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

/// The built-in themes the user can switch between at runtime.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ThemeId {
    /// The signature dark theme (the original palette, refined).
    Midnight,
    /// A clean light theme for bright rooms / projectors.
    Daylight,
    /// Maximum-contrast dark theme for accessibility.
    HighContrast,
    /// Black surfaces, green phosphor text — the "terminal / Matrix" look.
    Matrix,
}

impl ThemeId {
    pub const ALL: [ThemeId; 4] =
        [ThemeId::Midnight, ThemeId::Daylight, ThemeId::HighContrast, ThemeId::Matrix];

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Midnight => "Midnight",
            ThemeId::Daylight => "Daylight",
            ThemeId::HighContrast => "High Contrast",
            ThemeId::Matrix => "Matrix",
        }
    }

    /// Stable id for persistence.
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Midnight => "midnight",
            ThemeId::Daylight => "daylight",
            ThemeId::HighContrast => "high-contrast",
            ThemeId::Matrix => "matrix",
        }
    }

    pub fn from_key(key: &str) -> ThemeId {
        match key {
            "daylight" => ThemeId::Daylight,
            "high-contrast" => ThemeId::HighContrast,
            "matrix" => ThemeId::Matrix,
            _ => ThemeId::Midnight,
        }
    }
}

/// The brand accent hue the user can pick — recolors the single accent used by
/// the logo, panel headers, the selected activity item, and primary buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Accent {
    Amber,
    Blue,
    Green,
    Violet,
    Cyan,
    Red,
}

impl Accent {
    pub const ALL: [Accent; 6] =
        [Accent::Amber, Accent::Blue, Accent::Green, Accent::Violet, Accent::Cyan, Accent::Red];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Amber => "Amber",
            Accent::Blue => "Blue",
            Accent::Green => "Green",
            Accent::Violet => "Violet",
            Accent::Cyan => "Cyan",
            Accent::Red => "Red",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Accent::Amber => "amber",
            Accent::Blue => "blue",
            Accent::Green => "green",
            Accent::Violet => "violet",
            Accent::Cyan => "cyan",
            Accent::Red => "red",
        }
    }

    pub fn from_key(key: &str) -> Accent {
        match key {
            "blue" => Accent::Blue,
            "green" => Accent::Green,
            "violet" => Accent::Violet,
            "cyan" => Accent::Cyan,
            "red" => Accent::Red,
            _ => Accent::Amber,
        }
    }

    /// The accent's representative color.
    pub fn rgb(self) -> Rgb {
        match self {
            Accent::Amber => Rgb::hex(0xE08A45),
            Accent::Blue => Rgb::hex(0x4FA3E3),
            Accent::Green => Rgb::hex(0x5EBE6E),
            Accent::Violet => Rgb::hex(0xC678DD),
            Accent::Cyan => Rgb::hex(0x64C8D6),
            Accent::Red => Rgb::hex(0xE05B5B),
        }
    }
}

/// A resolved set of every color the UI needs — the single source of truth a
/// theme provides. The GTK chrome renders these into CSS `@define-color`s and
/// the Cairo renderers read them directly, so switching a theme re-tints the
/// whole surface. Colors only; font sizes are scaled at render time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThemeTokens {
    pub id: ThemeId,
    pub dark: bool,
    // Surfaces
    pub window_background: Rgb,
    pub chrome: Rgb,
    pub panel: Rgb,
    pub panel_alt: Rgb,
    pub editor_bg: Rgb,
    pub card: Rgb,
    pub border: Rgb,
    pub border_soft: Rgb,
    // Text
    pub text_primary: Rgb,
    pub text_secondary: Rgb,
    pub text_muted: Rgb,
    // Brand accent (recolored by `Accent`)
    pub accent: Rgb,
    // Terminal (matrix-retro console)
    pub term_bg: Rgb,
    pub term_fg: Rgb,
    // Named accents / status
    pub orange: Rgb,
    pub amber: Rgb,
    pub green: Rgb,
    pub green_deep: Rgb,
    pub blue: Rgb,
    pub cyan: Rgb,
    pub red: Rgb,
    pub yellow: Rgb,
    pub magenta: Rgb,
    // Interaction
    pub selection: Rgb,
    pub hover: Rgb,
    // Syntax
    pub syn_plain: Rgb,
    pub syn_keyword: Rgb,
    pub syn_control: Rgb,
    pub syn_number: Rgb,
    pub syn_string: Rgb,
    pub syn_comment: Rgb,
    pub syn_function: Rgb,
    pub syn_identifier: Rgb,
    pub syn_operator: Rgb,
    pub syn_line_number: Rgb,
}

impl ThemeTokens {
    /// The signature dark theme — reuses the original `palette`/`code` constants
    /// verbatim so the default look does not change.
    pub fn midnight() -> ThemeTokens {
        ThemeTokens {
            id: ThemeId::Midnight,
            dark: true,
            window_background: palette::WINDOW_BACKGROUND,
            chrome: palette::CHROME,
            panel: palette::PANEL,
            panel_alt: palette::PANEL_ALT,
            editor_bg: palette::EDITOR_BACKGROUND,
            card: palette::CARD,
            border: palette::BORDER,
            border_soft: palette::BORDER_SOFT,
            text_primary: Rgb::hex(0xE3E9F3),
            text_secondary: Rgb::hex(0xA3B2C8),
            text_muted: Rgb::hex(0x8090A8),
            accent: palette::ACCENT_ORANGE,
            term_bg: Rgb::hex(0x04070A),
            term_fg: Rgb::hex(0x43D459),
            orange: palette::ACCENT_ORANGE,
            amber: palette::ACCENT_AMBER,
            green: palette::ACCENT_GREEN,
            green_deep: palette::ACCENT_GREEN_DEEP,
            blue: palette::ACCENT_BLUE,
            cyan: palette::ACCENT_CYAN,
            red: palette::ACCENT_RED,
            yellow: palette::ACCENT_YELLOW,
            magenta: palette::ACCENT_MAGENTA,
            selection: palette::SELECTION,
            hover: palette::HOVER,
            syn_plain: code::PLAIN,
            syn_keyword: code::KEYWORD,
            syn_control: code::CONTROL,
            syn_number: code::NUMBER,
            syn_string: code::STRING,
            syn_comment: code::COMMENT,
            syn_function: code::FUNCTION,
            syn_identifier: code::IDENTIFIER,
            syn_operator: code::OPERATOR,
            syn_line_number: code::LINE_NUMBER,
        }
    }

    /// A clean, low-contrast light theme.
    pub fn daylight() -> ThemeTokens {
        ThemeTokens {
            id: ThemeId::Daylight,
            dark: false,
            window_background: Rgb::hex(0xF4F6FA),
            chrome: Rgb::hex(0xE9EEF4),
            panel: Rgb::hex(0xFFFFFF),
            panel_alt: Rgb::hex(0xEFF3F8),
            editor_bg: Rgb::hex(0xFFFFFF),
            card: Rgb::hex(0xF6F8FB),
            border: Rgb::hex(0xCBD5E3),
            border_soft: Rgb::hex(0xDCE3EC),
            text_primary: Rgb::hex(0x1B2430),
            text_secondary: Rgb::hex(0x4A586B),
            text_muted: Rgb::hex(0x8492A6),
            accent: Rgb::hex(0xC9702C),
            term_bg: Rgb::hex(0x0E1622),
            term_fg: Rgb::hex(0x46D267),
            orange: Rgb::hex(0xC9702C),
            amber: Rgb::hex(0xB06A2C),
            green: Rgb::hex(0x2E9E4B),
            green_deep: Rgb::hex(0x1F7A38),
            blue: Rgb::hex(0x2A7FD0),
            cyan: Rgb::hex(0x1597A8),
            red: Rgb::hex(0xCB4242),
            yellow: Rgb::hex(0xB8862A),
            magenta: Rgb::hex(0x9B4DB8),
            selection: Rgb::hex(0xCBE0F7),
            hover: Rgb::hex(0xE6ECF4),
            syn_plain: Rgb::hex(0x1B2430),
            syn_keyword: Rgb::hex(0x9B4DB8),
            syn_control: Rgb::hex(0xC0392B),
            syn_number: Rgb::hex(0x2E7D32),
            syn_string: Rgb::hex(0xB5651D),
            syn_comment: Rgb::hex(0x97A2B2),
            syn_function: Rgb::hex(0x1565C0),
            syn_identifier: Rgb::hex(0x1B2430),
            syn_operator: Rgb::hex(0x546177),
            syn_line_number: Rgb::hex(0xB0BAC8),
        }
    }

    /// Maximum-contrast dark theme for accessibility.
    pub fn high_contrast() -> ThemeTokens {
        ThemeTokens {
            id: ThemeId::HighContrast,
            dark: true,
            window_background: Rgb::hex(0x000000),
            chrome: Rgb::hex(0x050505),
            panel: Rgb::hex(0x0A0A0A),
            panel_alt: Rgb::hex(0x141414),
            editor_bg: Rgb::hex(0x000000),
            card: Rgb::hex(0x161616),
            border: Rgb::hex(0x3A3A3A),
            border_soft: Rgb::hex(0x2A2A2A),
            text_primary: Rgb::hex(0xFFFFFF),
            text_secondary: Rgb::hex(0xD6D6D6),
            text_muted: Rgb::hex(0xA6A6A6),
            accent: Rgb::hex(0xFFB000),
            term_bg: Rgb::hex(0x000000),
            term_fg: Rgb::hex(0x00FF66),
            orange: Rgb::hex(0xFFB000),
            amber: Rgb::hex(0xFFC844),
            green: Rgb::hex(0x33FF66),
            green_deep: Rgb::hex(0x22CC55),
            blue: Rgb::hex(0x4DA6FF),
            cyan: Rgb::hex(0x33E0E0),
            red: Rgb::hex(0xFF5555),
            yellow: Rgb::hex(0xFFD740),
            magenta: Rgb::hex(0xE066FF),
            selection: Rgb::hex(0x224488),
            hover: Rgb::hex(0x242424),
            syn_plain: Rgb::hex(0xF2F2F2),
            syn_keyword: Rgb::hex(0xE066FF),
            syn_control: Rgb::hex(0xFF6E6E),
            syn_number: Rgb::hex(0x7CFF7C),
            syn_string: Rgb::hex(0xFFC07A),
            syn_comment: Rgb::hex(0x9AA0A6),
            syn_function: Rgb::hex(0x6EC1FF),
            syn_identifier: Rgb::hex(0xF2F2F2),
            syn_operator: Rgb::hex(0xD0D0D0),
            syn_line_number: Rgb::hex(0x6A6A6A),
        }
    }

    /// Black surfaces, green phosphor text — the classic terminal aesthetic.
    pub fn matrix() -> ThemeTokens {
        let green = Rgb::hex(0x00FF41);
        ThemeTokens {
            id: ThemeId::Matrix,
            dark: true,
            window_background: Rgb::hex(0x000000),
            chrome: Rgb::hex(0x010601),
            panel: Rgb::hex(0x020B04),
            panel_alt: Rgb::hex(0x05180A),
            editor_bg: Rgb::hex(0x000300),
            card: Rgb::hex(0x07210D),
            border: Rgb::hex(0x0E5A22),
            border_soft: Rgb::hex(0x093D17),
            text_primary: Rgb::hex(0x3BFF6B),
            text_secondary: Rgb::hex(0x23C24A),
            text_muted: Rgb::hex(0x18913A),
            accent: green,
            term_bg: Rgb::hex(0x000000),
            term_fg: green,
            orange: green,
            amber: Rgb::hex(0x9BE05A),
            green: Rgb::hex(0x33FF66),
            green_deep: Rgb::hex(0x1FA841),
            blue: Rgb::hex(0x2AE6A0),
            cyan: Rgb::hex(0x4DFFC4),
            red: Rgb::hex(0xFF4D4D),
            yellow: Rgb::hex(0xCFFF4D),
            magenta: Rgb::hex(0x7CFF8F),
            selection: Rgb::hex(0x0E4A1E),
            hover: Rgb::hex(0x0A3315),
            syn_plain: Rgb::hex(0x3BFF6B),
            syn_keyword: green,
            syn_control: Rgb::hex(0x66FF99),
            syn_number: Rgb::hex(0xB6FF6E),
            syn_string: Rgb::hex(0x9BFF5A),
            syn_comment: Rgb::hex(0x1F8A3C),
            syn_function: Rgb::hex(0x4DFFC4),
            syn_identifier: Rgb::hex(0x3BFF6B),
            syn_operator: Rgb::hex(0x66CC7A),
            syn_line_number: Rgb::hex(0x155C26),
        }
    }

    pub fn for_id(id: ThemeId) -> ThemeTokens {
        match id {
            ThemeId::Midnight => ThemeTokens::midnight(),
            ThemeId::Daylight => ThemeTokens::daylight(),
            ThemeId::HighContrast => ThemeTokens::high_contrast(),
            ThemeId::Matrix => ThemeTokens::matrix(),
        }
    }

    /// Recolor the brand accent to `accent`, leaving everything else intact.
    pub fn with_accent(mut self, accent: Accent) -> ThemeTokens {
        self.accent = accent.rgb();
        self
    }
}

impl Default for ThemeTokens {
    fn default() -> Self {
        ThemeTokens::midnight()
    }
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
    fn contrast_extremes() {
        let black = Rgb::hex(0x000000);
        let white = Rgb::hex(0xFFFFFF);
        assert!((black.contrast(white) - 21.0).abs() < 0.1);
        assert!((white.contrast(white) - 1.0).abs() < 0.01);
    }

    /// No theme should render text that's hard to read on its own surfaces —
    /// guards against the "text not visible" regressions across all themes.
    #[test]
    fn every_theme_has_readable_text() {
        for id in ThemeId::ALL {
            let t = ThemeTokens::for_id(id);
            let surfaces = [
                ("panel", t.panel),
                ("panel_alt", t.panel_alt),
                ("card", t.card),
                ("window_background", t.window_background),
                ("chrome", t.chrome),
                ("editor_bg", t.editor_bg),
            ];
            for (name, bg) in surfaces {
                // Primary text must clearly stand out (WCAG AA body ≈ 4.5).
                assert!(
                    t.text_primary.contrast(bg) >= 4.5,
                    "{id:?}: text_primary on {name} contrast {:.2} < 4.5",
                    t.text_primary.contrast(bg)
                );
                // Secondary / muted are large-text/UI-chrome (AA large ≈ 3.0).
                assert!(
                    t.text_secondary.contrast(bg) >= 3.0,
                    "{id:?}: text_secondary on {name} contrast {:.2} < 3.0",
                    t.text_secondary.contrast(bg)
                );
                assert!(
                    t.text_muted.contrast(bg) >= 2.4,
                    "{id:?}: text_muted on {name} contrast {:.2} < 2.4",
                    t.text_muted.contrast(bg)
                );
            }
            // The matrix-retro terminal: green on its own black must be legible.
            assert!(
                t.term_fg.contrast(t.term_bg) >= 4.5,
                "{id:?}: terminal text contrast {:.2} < 4.5",
                t.term_fg.contrast(t.term_bg)
            );
        }
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

    fn luminance(c: Rgb) -> f64 {
        let (r, g, b) = c.to_unit();
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    #[test]
    fn midnight_reuses_original_palette() {
        let t = ThemeTokens::midnight();
        assert_eq!(t.window_background, palette::WINDOW_BACKGROUND);
        assert_eq!(t.editor_bg, palette::EDITOR_BACKGROUND);
        assert_eq!(t.accent, palette::ACCENT_ORANGE);
        assert_eq!(t.syn_keyword, code::KEYWORD);
        assert!(t.dark);
    }

    #[test]
    fn daylight_is_actually_light_and_legible() {
        let t = ThemeTokens::daylight();
        assert!(!t.dark);
        // Light surfaces, dark text → high contrast the other way round.
        assert!(luminance(t.window_background) > 0.8);
        assert!(luminance(t.editor_bg) > 0.9);
        assert!(luminance(t.text_primary) < 0.3);
    }

    #[test]
    fn high_contrast_maximizes_text_contrast() {
        let t = ThemeTokens::high_contrast();
        assert_eq!(t.editor_bg, Rgb::hex(0x000000));
        assert_eq!(t.text_primary, Rgb::hex(0xFFFFFF));
        assert!(luminance(t.text_primary) - luminance(t.editor_bg) > 0.9);
    }

    #[test]
    fn matrix_is_black_with_green_text() {
        let t = ThemeTokens::matrix();
        assert!(t.dark);
        assert_eq!(t.window_background, Rgb::hex(0x000000));
        assert_eq!(t.accent, Rgb::hex(0x00FF41));
        // green-dominant text + syntax
        let g = |c: Rgb| c.g > c.r && c.g > c.b;
        assert!(g(t.text_primary) && g(t.term_fg) && g(t.syn_keyword) && g(t.syn_plain));
    }

    #[test]
    fn for_id_round_trips_through_keys() {
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::from_key(id.key()), id);
            assert_eq!(ThemeTokens::for_id(id).id, id);
            assert!(!id.label().is_empty());
        }
    }

    #[test]
    fn accent_recolors_only_the_accent() {
        let base = ThemeTokens::midnight();
        let blue = base.with_accent(Accent::Blue);
        assert_eq!(blue.accent, Accent::Blue.rgb());
        // everything else untouched
        assert_eq!(blue.window_background, base.window_background);
        assert_eq!(blue.text_primary, base.text_primary);
        for a in Accent::ALL {
            assert_eq!(Accent::from_key(a.key()), a);
            assert!(!a.label().is_empty());
        }
    }
}
