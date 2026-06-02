//! Workspace variable table + matrix inspector state. Mirrors
//! `WorkspaceViewModel`: it ingests `whos` / `disp` text through the
//! [`parsers`](crate::services::parsers) and exposes the table, selection,
//! current heatmap matrix, and the live-REPL flag.

use crate::models::{DType, DapVariable, MatrixView, WorkspaceVariable};
use crate::observable::Property;
use crate::services::parsers;

pub struct WorkspaceViewModel {
    pub variables: Property<Vec<WorkspaceVariable>>,
    pub selected_name: Property<Option<String>>,
    pub inspected_matrix: Property<Option<MatrixView>>,
    pub live: Property<bool>,
}

impl Default for WorkspaceViewModel {
    fn default() -> Self {
        WorkspaceViewModel::new()
    }
}

impl WorkspaceViewModel {
    pub fn new() -> WorkspaceViewModel {
        WorkspaceViewModel {
            variables: Property::new(Vec::new()),
            selected_name: Property::new(None),
            inspected_matrix: Property::new(None),
            live: Property::new(false),
        }
    }

    /// Replace the table from a captured `whos` block.
    pub fn update_from_whos(&self, text: &str) {
        self.variables.set(parsers::parse_workspace(text));
    }

    /// Mirror a paused debug frame's locals into the workspace table so the
    /// variables are visible there too (matching the macOS reference). The DAP
    /// shape hints (`indexed`/`named`) approximate the displayed size.
    pub fn set_from_debug_locals(&self, locals: &[DapVariable]) {
        let vars = locals
            .iter()
            .map(|v| {
                let dtype = v.type_hint.as_deref().map(DType::from_class).unwrap_or(DType::Double);
                let size = match (v.indexed_variables, v.named_variables) {
                    (Some(n), _) if n > 0 => format!("{n} elems"),
                    (_, Some(n)) if n > 0 => format!("{n} fields"),
                    _ => "1x1".to_string(),
                };
                WorkspaceVariable::new(&v.name, dtype, size, 0).with_preview(v.value.clone())
            })
            .collect();
        self.variables.set(vars);
        self.live.set(true);
    }

    pub fn select(&self, name: impl Into<String>) {
        self.selected_name.set(Some(name.into()));
    }

    pub fn clear_selection(&self) {
        self.selected_name.set(None);
        self.inspected_matrix.set(None);
    }

    /// Build the heatmap matrix for `name` from a captured `disp` block.
    pub fn set_matrix_from_disp(&self, name: impl Into<String>, text: &str) {
        let (_, _, cells) = parsers::parse_matrix(text);
        self.inspected_matrix.set(Some(MatrixView::new(name, cells)));
    }

    pub fn clear(&self) {
        self.variables.set(Vec::new());
        self.clear_selection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingests_whos_text() {
        let vm = WorkspaceViewModel::new();
        vm.update_from_whos("a  1x1  double\nb  2x2  int32");
        let vars = vm.variables.get();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "a");
    }

    #[test]
    fn selection_and_clear() {
        let vm = WorkspaceViewModel::new();
        vm.select("a");
        assert_eq!(vm.selected_name.get().as_deref(), Some("a"));
        vm.clear_selection();
        assert!(vm.selected_name.get().is_none());
    }

    #[test]
    fn matrix_from_disp_builds_heatmap() {
        let vm = WorkspaceViewModel::new();
        vm.set_matrix_from_disp("M", "1 2 3\n4 5 6");
        let m = vm.inspected_matrix.get().unwrap();
        assert_eq!((m.rows, m.cols), (2, 3));
        assert_eq!(m.title, "M");
        assert_eq!(m.value_range(), Some((1.0, 6.0)));
    }

    #[test]
    fn debug_locals_populate_workspace() {
        let vm = WorkspaceViewModel::new();
        let mut m = DapVariable::scalar("A", "2x2 double");
        m.type_hint = Some("double".into());
        m.indexed_variables = Some(4);
        let locals = vec![DapVariable::scalar("x", "3"), m];
        vm.set_from_debug_locals(&locals);
        let vars = vm.variables.get();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].name, "x");
        assert_eq!(vars[0].preview, "3");
        assert_eq!(vars[1].size, "4 elems");
        assert!(vm.live.get());
    }

    #[test]
    fn clear_resets_everything() {
        let vm = WorkspaceViewModel::new();
        vm.update_from_whos("a 1x1 double");
        vm.select("a");
        vm.clear();
        assert!(vm.variables.get().is_empty());
        assert!(vm.selected_name.get().is_none());
    }
}
