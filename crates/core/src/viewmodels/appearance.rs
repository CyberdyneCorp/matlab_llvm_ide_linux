//! Appearance / personalization view model: the active theme, brand accent, UI
//! font scale, and code font family. GTK subscribes to `revision` (bumped on any
//! change) to re-render the CSS + re-tint the Cairo renderers, and the Settings
//! dialog drives the verb methods. Pure + tested; no GTK.

use crate::observable::Property;
use crate::theme::{Accent, ThemeId, ThemeTokens};

/// Font scale bounds (1.0 = the 12px baseline).
pub const FONT_SCALE_MIN: f64 = 0.8;
pub const FONT_SCALE_MAX: f64 = 1.8;
const FONT_SCALE_STEP: f64 = 0.1;

pub struct AppearanceViewModel {
    pub theme_id: Property<ThemeId>,
    pub accent: Property<Accent>,
    /// UI (chrome) font multiplier, clamped to [`FONT_SCALE_MIN`, `FONT_SCALE_MAX`].
    pub font_scale: Property<f64>,
    /// Code editor / console font multiplier, independent of the UI scale.
    pub code_font_scale: Property<f64>,
    pub code_font_family: Property<String>,
    /// Monotonically bumped whenever any appearance field changes, so a single
    /// subscription can trigger one CSS re-render.
    pub revision: Property<u64>,
}

impl Default for AppearanceViewModel {
    fn default() -> Self {
        AppearanceViewModel::new()
    }
}

impl AppearanceViewModel {
    pub fn new() -> AppearanceViewModel {
        AppearanceViewModel {
            theme_id: Property::new(ThemeId::Midnight),
            accent: Property::new(Accent::Amber),
            font_scale: Property::new(1.0),
            code_font_scale: Property::new(1.0),
            code_font_family: Property::new("JetBrains Mono".to_string()),
            revision: Property::new(0),
        }
    }

    fn bump(&self) {
        self.revision.update(|r| *r += 1);
    }

    pub fn set_theme(&self, id: ThemeId) {
        if self.theme_id.get() != id {
            self.theme_id.set(id);
            self.bump();
        }
    }

    pub fn set_accent(&self, accent: Accent) {
        if self.accent.get() != accent {
            self.accent.set(accent);
            self.bump();
        }
    }

    pub fn set_code_font(&self, family: impl Into<String>) {
        let family = family.into();
        if self.code_font_family.get() != family {
            self.code_font_family.set(family);
            self.bump();
        }
    }

    /// Set the font scale (clamped). Returns the applied value.
    pub fn set_font_scale(&self, scale: f64) -> f64 {
        let clamped = scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX);
        if (self.font_scale.get() - clamped).abs() > f64::EPSILON {
            self.font_scale.set(clamped);
            self.bump();
        }
        clamped
    }

    pub fn zoom_in(&self) {
        self.set_font_scale(self.font_scale.get() + FONT_SCALE_STEP);
    }

    pub fn zoom_out(&self) {
        self.set_font_scale(self.font_scale.get() - FONT_SCALE_STEP);
    }

    pub fn zoom_reset(&self) {
        self.set_font_scale(1.0);
    }

    /// Set the code editor font scale (clamped). Returns the applied value.
    pub fn set_code_font_scale(&self, scale: f64) -> f64 {
        let clamped = scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX);
        if (self.code_font_scale.get() - clamped).abs() > f64::EPSILON {
            self.code_font_scale.set(clamped);
            self.bump();
        }
        clamped
    }

    /// Apply a full set of persisted values at once (one revision bump).
    pub fn apply(
        &self,
        id: ThemeId,
        accent: Accent,
        font_scale: f64,
        code_font_scale: f64,
        code_font: impl Into<String>,
    ) {
        self.theme_id.set(id);
        self.accent.set(accent);
        self.font_scale.set(font_scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX));
        self.code_font_scale.set(code_font_scale.clamp(FONT_SCALE_MIN, FONT_SCALE_MAX));
        self.code_font_family.set(code_font.into());
        self.bump();
    }

    /// The resolved tokens for the current theme + accent.
    pub fn tokens(&self) -> ThemeTokens {
        ThemeTokens::for_id(self.theme_id.get()).with_accent(self.accent.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_midnight_amber_1x() {
        let vm = AppearanceViewModel::new();
        assert_eq!(vm.theme_id.get(), ThemeId::Midnight);
        assert_eq!(vm.accent.get(), Accent::Amber);
        assert_eq!(vm.font_scale.get(), 1.0);
        assert_eq!(vm.tokens().accent, Accent::Amber.rgb());
    }

    #[test]
    fn changes_bump_the_revision() {
        let vm = AppearanceViewModel::new();
        let r0 = vm.revision.get();
        vm.set_theme(ThemeId::Daylight);
        assert!(vm.revision.get() > r0);
        let r1 = vm.revision.get();
        vm.set_theme(ThemeId::Daylight); // no-op, no bump
        assert_eq!(vm.revision.get(), r1);
        vm.set_accent(Accent::Blue);
        assert!(vm.revision.get() > r1);
        assert_eq!(vm.tokens().id, ThemeId::Daylight);
        assert_eq!(vm.tokens().accent, Accent::Blue.rgb());
    }

    #[test]
    fn font_scale_clamps_and_steps() {
        let vm = AppearanceViewModel::new();
        assert_eq!(vm.set_font_scale(5.0), FONT_SCALE_MAX);
        assert_eq!(vm.set_font_scale(0.1), FONT_SCALE_MIN);
        vm.zoom_reset();
        assert_eq!(vm.font_scale.get(), 1.0);
        vm.zoom_in();
        assert!(vm.font_scale.get() > 1.0);
        vm.zoom_out();
        assert!((vm.font_scale.get() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn code_font_scale_is_independent_and_clamped() {
        let vm = AppearanceViewModel::new();
        assert_eq!(vm.code_font_scale.get(), 1.0);
        vm.set_font_scale(1.4); // UI zoom doesn't touch the editor scale
        assert_eq!(vm.code_font_scale.get(), 1.0);
        assert_eq!(vm.set_code_font_scale(9.0), FONT_SCALE_MAX);
        assert_eq!(vm.font_scale.get(), 1.4); // and vice versa
    }

    #[test]
    fn apply_sets_all_with_single_bump() {
        let vm = AppearanceViewModel::new();
        let r0 = vm.revision.get();
        vm.apply(ThemeId::HighContrast, Accent::Green, 1.3, 1.2, "Fira Code");
        assert_eq!(vm.revision.get(), r0 + 1);
        assert_eq!(vm.code_font_scale.get(), 1.2);
        assert_eq!(vm.theme_id.get(), ThemeId::HighContrast);
        assert_eq!(vm.accent.get(), Accent::Green);
        assert_eq!(vm.font_scale.get(), 1.3);
        assert_eq!(vm.code_font_family.get(), "Fira Code");
    }
}
