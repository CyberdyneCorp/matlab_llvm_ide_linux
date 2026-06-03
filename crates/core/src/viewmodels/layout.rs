//! Pane visibility + sizing for the main window. Mirrors `LayoutViewModel`.
//! Widths are clamped to the reference min/max so drags can't collapse a pane.

use crate::models::CenterLayoutMode;
use crate::observable::Property;

pub const LEFT_MIN: i32 = 200;
pub const LEFT_MAX: i32 = 400;
pub const SIDE_MIN: i32 = 300;
pub const SIDE_MAX: i32 = 600;

pub struct LayoutViewModel {
    pub left_sidebar_width: Property<i32>,
    pub workspace_width: Property<i32>,
    pub plots_width: Property<i32>,
    pub sidebar_visible: Property<bool>,
    pub workspace_visible: Property<bool>,
    pub plots_visible: Property<bool>,
    pub center_mode: Property<CenterLayoutMode>,
    /// Distraction-free mode: hide the activity bar, sidebar, and right panels,
    /// leaving just the editor + console. Overlays the per-panel visibility.
    pub zen: Property<bool>,
    /// Whether the flowchart editor's BLOCKS palette is shown (persisted).
    pub flow_palette_visible: Property<bool>,
}

impl Default for LayoutViewModel {
    fn default() -> Self {
        LayoutViewModel::new()
    }
}

impl LayoutViewModel {
    pub fn new() -> LayoutViewModel {
        LayoutViewModel {
            left_sidebar_width: Property::new(crate::theme::metrics::LEFT_SIDEBAR_WIDTH),
            workspace_width: Property::new(crate::theme::metrics::WORKSPACE_COLUMN_WIDTH),
            plots_width: Property::new(crate::theme::metrics::PLOTS_COLUMN_WIDTH),
            sidebar_visible: Property::new(true),
            workspace_visible: Property::new(true),
            plots_visible: Property::new(true),
            center_mode: Property::new(CenterLayoutMode::Split),
            zen: Property::new(false),
            flow_palette_visible: Property::new(true),
        }
    }

    pub fn toggle_sidebar(&self) {
        self.sidebar_visible.update(|v| *v = !*v);
    }

    pub fn toggle_flow_palette(&self) {
        self.flow_palette_visible.update(|v| *v = !*v);
    }

    pub fn toggle_zen(&self) {
        self.zen.update(|v| *v = !*v);
    }

    /// Whether the activity bar should show (hidden in zen mode).
    pub fn chrome_visible(&self) -> bool {
        !self.zen.get()
    }

    /// Effective sidebar visibility (its flag, suppressed by zen).
    pub fn sidebar_effective(&self) -> bool {
        !self.zen.get() && self.sidebar_visible.get()
    }

    /// Effective right-column visibility (workspace or plots, suppressed by zen).
    pub fn right_effective(&self) -> bool {
        !self.zen.get() && (self.workspace_visible.get() || self.plots_visible.get())
    }

    pub fn toggle_workspace(&self) {
        self.workspace_visible.update(|v| *v = !*v);
    }

    pub fn toggle_plots(&self) {
        self.plots_visible.update(|v| *v = !*v);
    }

    pub fn set_center_mode(&self, mode: CenterLayoutMode) {
        self.center_mode.set_if_changed(mode);
    }

    pub fn set_left_width(&self, width: i32) {
        self.left_sidebar_width.set(width.clamp(LEFT_MIN, LEFT_MAX));
    }

    pub fn set_workspace_width(&self, width: i32) {
        self.workspace_width.set(width.clamp(SIDE_MIN, SIDE_MAX));
    }

    pub fn set_plots_width(&self, width: i32) {
        self.plots_width.set(width.clamp(SIDE_MIN, SIDE_MAX));
    }

    /// Restore every pane to its default size/visibility.
    pub fn reset(&self) {
        self.left_sidebar_width.set(crate::theme::metrics::LEFT_SIDEBAR_WIDTH);
        self.workspace_width.set(crate::theme::metrics::WORKSPACE_COLUMN_WIDTH);
        self.plots_width.set(crate::theme::metrics::PLOTS_COLUMN_WIDTH);
        self.sidebar_visible.set(true);
        self.workspace_visible.set(true);
        self.plots_visible.set(true);
        self.center_mode.set(CenterLayoutMode::Split);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let vm = LayoutViewModel::new();
        assert!(vm.workspace_visible.get());
        assert!(vm.plots_visible.get());
        assert!(vm.sidebar_visible.get());
        assert_eq!(vm.center_mode.get(), CenterLayoutMode::Split);
        assert!(!vm.zen.get());
    }

    #[test]
    fn zen_overrides_panel_visibility() {
        let vm = LayoutViewModel::new();
        assert!(vm.sidebar_effective() && vm.right_effective() && vm.chrome_visible());
        vm.toggle_zen();
        assert!(vm.zen.get());
        // panels suppressed even though their own flags are still true
        assert!(!vm.sidebar_effective());
        assert!(!vm.right_effective());
        assert!(!vm.chrome_visible());
        assert!(vm.sidebar_visible.get()); // flag untouched
        vm.toggle_zen();
        assert!(vm.sidebar_effective() && vm.right_effective());
    }

    #[test]
    fn toggles_flip_visibility() {
        let vm = LayoutViewModel::new();
        vm.toggle_workspace();
        assert!(!vm.workspace_visible.get());
        vm.toggle_plots();
        assert!(!vm.plots_visible.get());
        vm.toggle_sidebar();
        assert!(!vm.sidebar_visible.get());
        assert!(vm.flow_palette_visible.get());
        vm.toggle_flow_palette();
        assert!(!vm.flow_palette_visible.get());
    }

    #[test]
    fn widths_are_clamped() {
        let vm = LayoutViewModel::new();
        vm.set_left_width(50);
        assert_eq!(vm.left_sidebar_width.get(), LEFT_MIN);
        vm.set_left_width(9999);
        assert_eq!(vm.left_sidebar_width.get(), LEFT_MAX);
        vm.set_workspace_width(10_000);
        assert_eq!(vm.workspace_width.get(), SIDE_MAX);
        vm.set_plots_width(0);
        assert_eq!(vm.plots_width.get(), SIDE_MIN);
    }

    #[test]
    fn reset_restores_defaults() {
        let vm = LayoutViewModel::new();
        vm.toggle_workspace();
        vm.set_center_mode(CenterLayoutMode::EditorOnly);
        vm.set_left_width(400);
        vm.reset();
        assert!(vm.workspace_visible.get());
        assert_eq!(vm.center_mode.get(), CenterLayoutMode::Split);
        assert_eq!(vm.left_sidebar_width.get(), crate::theme::metrics::LEFT_SIDEBAR_WIDTH);
    }
}
