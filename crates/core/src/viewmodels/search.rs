//! Find-in-files view model. Mirrors `SearchViewModel`: a query, a match mode
//! (filenames / content / both), and the result list produced by walking the
//! project tree through the `FileSystem` service.

use std::path::{Path, PathBuf};

use crate::models::SearchMode;
use crate::observable::Property;
use crate::services::filesystem::FileSystem;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub path: PathBuf,
    /// 1-indexed line for content matches; `None` for filename matches.
    pub line: Option<usize>,
    pub preview: String,
}

pub struct SearchViewModel {
    pub query: Property<String>,
    pub mode: Property<SearchMode>,
    pub results: Property<Vec<SearchResult>>,
}

impl Default for SearchViewModel {
    fn default() -> Self {
        SearchViewModel::new()
    }
}

impl SearchViewModel {
    pub fn new() -> SearchViewModel {
        SearchViewModel {
            query: Property::new(String::new()),
            mode: Property::new(SearchMode::Both),
            results: Property::new(Vec::new()),
        }
    }

    pub fn set_query(&self, query: impl Into<String>) {
        self.query.set(query.into());
    }

    pub fn set_mode(&self, mode: SearchMode) {
        self.mode.set_if_changed(mode);
    }

    /// Run the search under `root`, populating `results`. A blank query clears.
    pub fn run(&self, fs: &dyn FileSystem, root: &Path) {
        let query = self.query.get();
        if query.trim().is_empty() {
            self.results.set(Vec::new());
            return;
        }
        let mode = self.mode.get();
        let mut files = Vec::new();
        collect_files(fs, root, 16, &mut files);

        let mut out = Vec::new();
        let needle = query.to_lowercase();
        for path in files {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if mode.matches_filenames() && name.to_lowercase().contains(&needle) {
                out.push(SearchResult { path: path.clone(), line: None, preview: name.clone() });
            }
            if mode.matches_content() {
                if let Ok(text) = fs.read_to_string(&path) {
                    for (i, line) in text.lines().enumerate() {
                        if line.to_lowercase().contains(&needle) {
                            out.push(SearchResult {
                                path: path.clone(),
                                line: Some(i + 1),
                                preview: line.trim().to_string(),
                            });
                        }
                    }
                }
            }
        }
        self.results.set(out);
    }
}

fn collect_files(fs: &dyn FileSystem, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs.read_dir(dir) else { return };
    for entry in entries {
        let name = entry.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if entry.is_dir {
            if depth > 0 {
                collect_files(fs, &entry.path, depth - 1, out);
            }
        } else {
            out.push(entry.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::FakeFileSystem;

    fn fs() -> FakeFileSystem {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/alpha.m", "x = 1;\ndisp(total)\n")
            .add_file("/p/sub/beta.m", "total = 2;\n")
            .add_dir("/p");
        fs
    }

    #[test]
    fn blank_query_clears_results() {
        let vm = SearchViewModel::new();
        vm.set_query("   ");
        vm.run(&fs(), Path::new("/p"));
        assert!(vm.results.get().is_empty());
    }

    #[test]
    fn filename_match() {
        let vm = SearchViewModel::new();
        vm.set_mode(SearchMode::Filename);
        vm.set_query("beta");
        vm.run(&fs(), Path::new("/p"));
        let results = vm.results.get();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.ends_with("beta.m"));
        assert!(results[0].line.is_none());
    }

    #[test]
    fn content_match_reports_line() {
        let vm = SearchViewModel::new();
        vm.set_mode(SearchMode::Content);
        vm.set_query("total");
        vm.run(&fs(), Path::new("/p"));
        let results = vm.results.get();
        // "disp(total)" in alpha.m line 2 and "total = 2;" in beta.m line 1
        assert!(results.iter().any(|r| r.line == Some(2) && r.path.ends_with("alpha.m")));
        assert!(results.iter().any(|r| r.line == Some(1) && r.path.ends_with("beta.m")));
    }

    #[test]
    fn both_mode_matches_name_and_content() {
        let vm = SearchViewModel::new();
        vm.set_mode(SearchMode::Both);
        vm.set_query("alpha");
        vm.run(&fs(), Path::new("/p"));
        // matches the filename alpha.m (no content has "alpha")
        assert_eq!(vm.results.get().len(), 1);
    }

    #[test]
    fn search_is_case_insensitive() {
        let vm = SearchViewModel::new();
        vm.set_mode(SearchMode::Content);
        vm.set_query("TOTAL");
        vm.run(&fs(), Path::new("/p"));
        assert!(!vm.results.get().is_empty());
    }
}
