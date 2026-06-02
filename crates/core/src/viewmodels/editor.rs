//! Editor tab management. Mirrors `EditorViewModel`: the open-tab list, the
//! active tab, dirty tracking, per-line breakpoints, and the execution-line
//! marker. Opening a file already open re-focuses it rather than duplicating.

use std::path::Path;

use crate::models::EditorTab;
use crate::observable::Property;
use crate::services::filesystem::FileSystem;

pub struct EditorViewModel {
    pub tabs: Property<Vec<EditorTab>>,
    pub active_id: Property<Option<u64>>,
    /// A `(tab_id, line)` jump request (1-indexed) — e.g. clicking a diagnostic
    /// in the PROBLEMS pane. The code view scrolls + places the cursor.
    pub goto_request: Property<Option<(u64, usize)>>,
}

impl Default for EditorViewModel {
    fn default() -> Self {
        EditorViewModel::new()
    }
}

impl EditorViewModel {
    pub fn new() -> EditorViewModel {
        EditorViewModel {
            tabs: Property::new(Vec::new()),
            active_id: Property::new(None),
            goto_request: Property::new(None),
        }
    }

    /// Ask the view to scroll to a 1-indexed line in a tab.
    pub fn request_goto(&self, tab_id: u64, line: usize) {
        self.set_active(tab_id);
        self.goto_request.set(Some((tab_id, line)));
    }

    /// Add a text tab and focus it; returns its id.
    pub fn open_text(
        &self,
        name: impl Into<String>,
        language: impl Into<String>,
        contents: impl Into<String>,
    ) -> u64 {
        let tab = EditorTab::text(name, language, contents);
        let id = tab.id;
        self.tabs.update(|t| t.push(tab));
        self.active_id.set(Some(id));
        id
    }

    /// Open a file from disk. If a tab with the same URL is already open, focus
    /// it instead of re-reading. Returns the tab id.
    pub fn open_file(&self, fs: &dyn FileSystem, path: &Path) -> std::io::Result<u64> {
        if let Some(existing) = self.tabs.with(|tabs| {
            tabs.iter().find(|t| t.url.as_deref() == Some(path)).map(|t| t.id)
        }) {
            self.active_id.set(Some(existing));
            return Ok(existing);
        }
        let contents = fs.read_to_string(path)?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let tab = EditorTab::text(name, language_label(&ext), contents).with_url(path.to_path_buf());
        let id = tab.id;
        self.tabs.update(|t| t.push(tab));
        self.active_id.set(Some(id));
        Ok(id)
    }

    pub fn set_active(&self, id: u64) {
        self.active_id.set(Some(id));
    }

    pub fn active_tab(&self) -> Option<EditorTab> {
        let active = self.active_id.get()?;
        self.tabs.with(|t| t.iter().find(|tab| tab.id == active).cloned())
    }

    /// Close a tab; if it was active, focus the previous one (or none).
    pub fn close(&self, id: u64) {
        let mut new_active = self.active_id.get();
        self.tabs.update(|tabs| {
            if let Some(pos) = tabs.iter().position(|t| t.id == id) {
                tabs.remove(pos);
                if new_active == Some(id) {
                    new_active = tabs.get(pos.saturating_sub(1)).or_else(|| tabs.last()).map(|t| t.id);
                }
            }
        });
        self.active_id.set(new_active);
    }

    /// Replace a tab's contents and mark it dirty.
    pub fn update_contents(&self, id: u64, contents: impl Into<String>) {
        let contents = contents.into();
        self.mutate(id, |t| {
            t.contents = contents;
            t.is_dirty = true;
        });
    }

    pub fn mark_saved(&self, id: u64) {
        self.mutate(id, |t| t.is_dirty = false);
    }

    pub fn toggle_breakpoint(&self, id: u64, line: usize) {
        self.mutate(id, |t| {
            t.toggle_breakpoint(line);
        });
    }

    /// Set the paused execution line on `id` and clear it on every other tab.
    pub fn set_execution_line(&self, id: u64, line: Option<usize>) {
        self.tabs.update(|tabs| {
            for t in tabs.iter_mut() {
                t.execution_line = if t.id == id { line } else { None };
            }
        });
    }

    pub fn clear_execution_lines(&self) {
        self.tabs.update(|tabs| {
            for t in tabs.iter_mut() {
                t.execution_line = None;
            }
        });
    }

    pub fn has_dirty(&self) -> bool {
        self.tabs.with(|t| t.iter().any(|tab| tab.is_dirty))
    }

    fn mutate(&self, id: u64, f: impl FnOnce(&mut EditorTab)) {
        self.tabs.update(|tabs| {
            if let Some(t) = tabs.iter_mut().find(|t| t.id == id) {
                f(t);
            }
        });
    }
}

/// Human language label from a file extension (used by tabs + highlighter).
pub fn language_label(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "m" => "Matlab",
        "c" => "C",
        "cpp" | "cc" | "cxx" => "C++",
        "h" | "hpp" => "Header",
        "py" => "Python",
        "ts" => "TypeScript",
        "ll" => "LLVM IR",
        "mlir" => "MLIR",
        "sv" | "v" | "va" => "Verilog",
        _ => "Plain",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TabKind;
    use crate::services::filesystem::FakeFileSystem;

    #[test]
    fn open_text_focuses_new_tab() {
        let vm = EditorViewModel::new();
        let id = vm.open_text("a.m", "Matlab", "x = 1;");
        assert_eq!(vm.active_id.get(), Some(id));
        assert_eq!(vm.tabs.get().len(), 1);
        assert_eq!(vm.active_tab().unwrap().kind, TabKind::Text);
    }

    #[test]
    fn open_file_reads_and_detects_language() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/foo.m", "y = 2;");
        let vm = EditorViewModel::new();
        let id = vm.open_file(&fs, Path::new("/p/foo.m")).unwrap();
        let tab = vm.active_tab().unwrap();
        assert_eq!(tab.id, id);
        assert_eq!(tab.language, "Matlab");
        assert_eq!(tab.contents, "y = 2;");
    }

    #[test]
    fn open_existing_file_refocuses() {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/foo.m", "y = 2;");
        let vm = EditorViewModel::new();
        let id1 = vm.open_file(&fs, Path::new("/p/foo.m")).unwrap();
        let id2 = vm.open_file(&fs, Path::new("/p/foo.m")).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(vm.tabs.get().len(), 1);
    }

    #[test]
    fn update_marks_dirty_and_save_clears() {
        let vm = EditorViewModel::new();
        let id = vm.open_text("a.m", "Matlab", "");
        vm.update_contents(id, "x = 1;");
        assert!(vm.has_dirty());
        vm.mark_saved(id);
        assert!(!vm.has_dirty());
    }

    #[test]
    fn close_active_focuses_previous() {
        let vm = EditorViewModel::new();
        let a = vm.open_text("a", "Matlab", "");
        let b = vm.open_text("b", "Matlab", "");
        assert_eq!(vm.active_id.get(), Some(b));
        vm.close(b);
        assert_eq!(vm.active_id.get(), Some(a));
        vm.close(a);
        assert!(vm.active_id.get().is_none());
    }

    #[test]
    fn execution_line_is_exclusive() {
        let vm = EditorViewModel::new();
        let a = vm.open_text("a", "Matlab", "");
        let b = vm.open_text("b", "Matlab", "");
        vm.set_execution_line(a, Some(5));
        vm.set_execution_line(b, Some(9));
        let tabs = vm.tabs.get();
        assert_eq!(tabs.iter().find(|t| t.id == a).unwrap().execution_line, None);
        assert_eq!(tabs.iter().find(|t| t.id == b).unwrap().execution_line, Some(9));
        vm.clear_execution_lines();
        assert!(vm.tabs.get().iter().all(|t| t.execution_line.is_none()));
    }

    #[test]
    fn toggle_breakpoint_on_tab() {
        let vm = EditorViewModel::new();
        let id = vm.open_text("a", "Matlab", "");
        vm.toggle_breakpoint(id, 3);
        assert!(vm.active_tab().unwrap().breakpoints.contains_key(&3));
        vm.toggle_breakpoint(id, 3);
        assert!(vm.active_tab().unwrap().breakpoints.is_empty());
    }

    #[test]
    fn request_goto_sets_active_and_request() {
        let vm = EditorViewModel::new();
        let a = vm.open_text("a", "Matlab", "");
        let b = vm.open_text("b", "Matlab", "");
        vm.set_active(a);
        vm.request_goto(b, 7);
        assert_eq!(vm.active_id.get(), Some(b));
        assert_eq!(vm.goto_request.get(), Some((b, 7)));
    }

    #[test]
    fn language_label_mapping() {
        assert_eq!(language_label("m"), "Matlab");
        assert_eq!(language_label("CPP"), "C++");
        assert_eq!(language_label("zzz"), "Plain");
    }
}
