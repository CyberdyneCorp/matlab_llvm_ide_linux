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
    pub workspace_visible: Property<bool>,
    pub plots_visible: Property<bool>,
    pub center_mode: Property<CenterLayoutMode>,
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
            workspace_visible: Property::new(true),
            plots_visible: Property::new(false),
            center_mode: Property::new(CenterLayoutMode::Split),
        }
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
        self.workspace_visible.set(true);
        self.plots_visible.set(false);
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
        assert!(!vm.plots_visible.get());
        assert_eq!(vm.center_mode.get(), CenterLayoutMode::Split);
    }

    #[test]
    fn toggles_flip_visibility() {
        let vm = LayoutViewModel::new();
        vm.toggle_workspace();
        assert!(!vm.workspace_visible.get());
        vm.toggle_plots();
        assert!(vm.plots_visible.get());
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
