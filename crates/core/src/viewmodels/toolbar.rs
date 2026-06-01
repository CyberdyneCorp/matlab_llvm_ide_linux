//! Toolbar state: compile target / optimization / numeric pickers and the
//! run/debug activity flags that enable the Stop button. Mirrors the toolbar
//! slice of `SmallViewModels.swift`.

use crate::models::{CompilerTarget, NumericMode, OptimizationProfile};
use crate::observable::Property;

pub struct ToolbarViewModel {
    pub target: Property<CompilerTarget>,
    pub optimization: Property<OptimizationProfile>,
    pub numeric_mode: Property<NumericMode>,
    pub is_running: Property<bool>,
    pub is_debugging: Property<bool>,
}

impl Default for ToolbarViewModel {
    fn default() -> Self {
        ToolbarViewModel::new()
    }
}

impl ToolbarViewModel {
    pub fn new() -> ToolbarViewModel {
        ToolbarViewModel {
            target: Property::new(CompilerTarget::Cpp),
            optimization: Property::new(OptimizationProfile::O0),
            numeric_mode: Property::new(NumericMode::Float64),
            is_running: Property::new(false),
            is_debugging: Property::new(false),
        }
    }

    pub fn set_target(&self, target: CompilerTarget) {
        self.target.set_if_changed(target);
    }

    pub fn set_optimization(&self, opt: OptimizationProfile) {
        self.optimization.set_if_changed(opt);
    }

    pub fn set_numeric_mode(&self, mode: NumericMode) {
        self.numeric_mode.set_if_changed(mode);
    }

    /// The Stop button is enabled while a run or debug session is live.
    pub fn can_stop(&self) -> bool {
        self.is_running.get() || self.is_debugging.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let vm = ToolbarViewModel::new();
        assert_eq!(vm.target.get(), CompilerTarget::Cpp);
        assert_eq!(vm.optimization.get(), OptimizationProfile::O0);
        assert!(!vm.can_stop());
    }

    #[test]
    fn pickers_update() {
        let vm = ToolbarViewModel::new();
        vm.set_target(CompilerTarget::Llvm);
        vm.set_optimization(OptimizationProfile::O2);
        vm.set_numeric_mode(NumericMode::Strict);
        assert_eq!(vm.target.get(), CompilerTarget::Llvm);
        assert_eq!(vm.optimization.get(), OptimizationProfile::O2);
        assert_eq!(vm.numeric_mode.get(), NumericMode::Strict);
    }

    #[test]
    fn can_stop_when_running_or_debugging() {
        let vm = ToolbarViewModel::new();
        vm.is_running.set(true);
        assert!(vm.can_stop());
        vm.is_running.set(false);
        vm.is_debugging.set(true);
        assert!(vm.can_stop());
    }
}
