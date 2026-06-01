//! Function / data / exception breakpoint lists for the Debug panel. Mirrors
//! `BreakpointsViewModel`. Per-line breakpoints live on `EditorTab`; this VM
//! owns the panel-managed kinds.

use crate::models::{DataAccess, DataBreakpoint, ExceptionFilter, FunctionBreakpoint};
use crate::observable::Property;

pub struct BreakpointsViewModel {
    pub function_bps: Property<Vec<FunctionBreakpoint>>,
    pub data_bps: Property<Vec<DataBreakpoint>>,
    pub exception_filters: Property<Vec<ExceptionFilter>>,
}

impl Default for BreakpointsViewModel {
    fn default() -> Self {
        BreakpointsViewModel::new()
    }
}

impl BreakpointsViewModel {
    pub fn new() -> BreakpointsViewModel {
        BreakpointsViewModel {
            function_bps: Property::new(Vec::new()),
            data_bps: Property::new(Vec::new()),
            // matlabc ships one filter: `error`.
            exception_filters: Property::new(vec![ExceptionFilter {
                filter: "error".into(),
                label: "Uncaught errors".into(),
                enabled: false,
                condition: None,
                supports_condition: false,
            }]),
        }
    }

    pub fn add_function(&self, name: impl Into<String>) -> u64 {
        let bp = FunctionBreakpoint::new(name);
        let id = bp.id;
        self.function_bps.update(|b| b.push(bp));
        id
    }

    pub fn remove_function(&self, id: u64) {
        self.function_bps.update(|b| b.retain(|bp| bp.id != id));
    }

    pub fn toggle_function(&self, id: u64) {
        self.function_bps.update(|b| {
            if let Some(bp) = b.iter_mut().find(|bp| bp.id == id) {
                bp.enabled = !bp.enabled;
            }
        });
    }

    pub fn add_data(&self, name: impl Into<String>, access: DataAccess) -> u64 {
        let bp = DataBreakpoint::new(name, access);
        let id = bp.id;
        self.data_bps.update(|b| b.push(bp));
        id
    }

    pub fn remove_data(&self, id: u64) {
        self.data_bps.update(|b| b.retain(|bp| bp.id != id));
    }

    pub fn set_exception_enabled(&self, filter: &str, enabled: bool) {
        self.exception_filters.update(|f| {
            if let Some(ef) = f.iter_mut().find(|ef| ef.filter == filter) {
                ef.enabled = enabled;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ships_error_exception_filter() {
        let vm = BreakpointsViewModel::new();
        let filters = vm.exception_filters.get();
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].filter, "error");
        assert!(!filters[0].enabled);
    }

    #[test]
    fn add_remove_function_breakpoint() {
        let vm = BreakpointsViewModel::new();
        let id = vm.add_function("foo");
        assert_eq!(vm.function_bps.get().len(), 1);
        vm.remove_function(id);
        assert!(vm.function_bps.get().is_empty());
    }

    #[test]
    fn toggle_function_enabled() {
        let vm = BreakpointsViewModel::new();
        let id = vm.add_function("foo");
        vm.toggle_function(id);
        assert!(!vm.function_bps.get()[0].enabled);
        vm.toggle_function(id);
        assert!(vm.function_bps.get()[0].enabled);
    }

    #[test]
    fn add_remove_data_breakpoint() {
        let vm = BreakpointsViewModel::new();
        let id = vm.add_data("total", DataAccess::Write);
        assert_eq!(vm.data_bps.get()[0].access, DataAccess::Write);
        vm.remove_data(id);
        assert!(vm.data_bps.get().is_empty());
    }

    #[test]
    fn enable_exception_filter() {
        let vm = BreakpointsViewModel::new();
        vm.set_exception_enabled("error", true);
        assert!(vm.exception_filters.get()[0].enabled);
    }
}
