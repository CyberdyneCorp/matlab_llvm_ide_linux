//! Renders the GTK stylesheet from the active [`ThemeTokens`] + UI font scale.
//!
//! `resources/theme.css` is a template: it carries the structural rules
//! (referencing `@mf_*` colors and `__FS_*__` font-size sentinels) but NOT the
//! color definitions. [`render`] prepends a generated `@define-color` block and
//! substitutes the scaled font sizes, producing a complete stylesheet to hand to
//! a `CssProvider`. Re-rendering on appearance change is what makes theme + zoom
//! switching instant.

use std::cell::Cell;

use matforge_core::theme::{Rgb, ThemeTokens};

const TEMPLATE: &str = include_str!("../resources/theme.css");

thread_local! {
    /// The active theme, mirrored here so the Cairo renderers (plots, flowchart,
    /// gutter) can read it without threading it through every draw call.
    static CURRENT: Cell<ThemeTokens> = Cell::new(ThemeTokens::midnight());
    /// The active code/editor font scale, for the Cairo gutter line numbers.
    static CODE_SCALE: Cell<f64> = const { Cell::new(1.0) };
}

/// Record the active theme (called whenever appearance changes).
pub fn set_current(tokens: ThemeTokens) {
    CURRENT.with(|c| c.set(tokens));
}

/// The active theme tokens for Cairo rendering.
pub fn current() -> ThemeTokens {
    CURRENT.with(|c| c.get())
}

/// Record the active editor font scale (for the Cairo gutter).
pub fn set_code_scale(scale: f64) {
    CODE_SCALE.with(|c| c.set(scale));
}

/// The active editor font scale.
pub fn code_scale() -> f64 {
    CODE_SCALE.with(|c| c.get())
}

/// Build the full CSS for `tokens` at the given UI + code font scales (1.0 = the
/// 12px baseline). `code_scale` sizes the editor/console; `ui_scale` the rest.
pub fn render(tokens: &ThemeTokens, ui_scale: f64, code_scale: f64) -> String {
    let mut css = String::with_capacity(TEMPLATE.len() + 1280);
    css.push_str(&color_block(tokens));
    css.push('\n');
    css.push_str(
        &TEMPLATE
            .replace("__FS_LG__", &px(13.0, ui_scale))
            .replace("__FS_BASE__", &px(12.0, ui_scale))
            .replace("__FS_SM__", &px(11.0, ui_scale))
            .replace("__FS_XS__", &px(10.0, ui_scale))
            .replace("__FS_XXS__", &px(9.0, ui_scale))
            .replace("__FS_CODE__", &px(12.0, code_scale)),
    );
    css
}

fn px(base: f64, scale: f64) -> String {
    format!("{}px", ((base * scale).round() as i64).max(1))
}

/// The generated `@define-color` header for one theme.
fn color_block(t: &ThemeTokens) -> String {
    let pairs: [(&str, Rgb); 26] = [
        ("mf_window_background", t.window_background),
        ("mf_chrome", t.chrome),
        ("mf_panel", t.panel),
        ("mf_panel_alt", t.panel_alt),
        ("mf_editor_bg", t.editor_bg),
        ("mf_card", t.card),
        ("mf_border", t.border),
        ("mf_border_soft", t.border_soft),
        ("mf_text_primary", t.text_primary),
        ("mf_text_secondary", t.text_secondary),
        ("mf_text_muted", t.text_muted),
        ("mf_accent", t.accent),
        ("mf_term_bg", t.term_bg),
        ("mf_term_fg", t.term_fg),
        ("mf_orange", t.orange),
        ("mf_amber", t.amber),
        ("mf_green", t.green),
        ("mf_blue", t.blue),
        ("mf_cyan", t.cyan),
        ("mf_red", t.red),
        ("mf_yellow", t.yellow),
        ("mf_magenta", t.magenta),
        ("mf_selection", t.selection),
        ("mf_hover", t.hover),
        ("mf_code_fg", t.syn_plain),
        ("mf_gutter", t.syn_line_number),
    ];
    let mut out = String::with_capacity(pairs.len() * 40);
    for (name, rgb) in pairs {
        out.push_str(&format!("@define-color {name} {};\n", rgb.to_css()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use matforge_core::theme::ThemeTokens;

    #[test]
    fn renders_color_block_and_no_sentinels_remain() {
        let css = render(&ThemeTokens::daylight(), 1.0, 1.0);
        assert!(css.contains("@define-color mf_window_background #f4f6fa;"));
        assert!(css.contains("@define-color mf_accent"));
        assert!(!css.contains("__FS_")); // every sentinel substituted
    }

    #[test]
    fn ui_and_code_scales_are_independent() {
        // UI 1.0, editor 1.5 → chrome stays 12px, the editor is 18px.
        let css = render(&ThemeTokens::midnight(), 1.0, 1.5);
        assert!(css.contains("font-size: 12px")); // chrome (__FS_BASE__)
        assert!(css.contains("font-size: 18px")); // editor (__FS_CODE__ = 12 * 1.5)
    }
}
