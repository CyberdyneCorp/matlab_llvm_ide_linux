//! Status-bar state, center-layout mode, explorer actions, and search config.
//! Mirrors the small value types in `Models.swift`.

use std::path::PathBuf;

/// Status-bar fields shown along the bottom edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusBarState {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub indent: String,
    pub encoding: String,
    pub line_ending: String,
    pub language: String,
}

impl Default for StatusBarState {
    fn default() -> Self {
        StatusBarState {
            message: "Ready".into(),
            line: 1,
            column: 1,
            indent: "Spaces: 4".into(),
            encoding: "UTF-8".into(),
            line_ending: "LF".into(),
            language: "MATLAB".into(),
        }
    }
}

/// Which panes are visible in the center column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CenterLayoutMode {
    Split,
    EditorOnly,
    ConsoleOnly,
}

impl CenterLayoutMode {
    pub const ALL: [CenterLayoutMode; 3] =
        [CenterLayoutMode::Split, CenterLayoutMode::EditorOnly, CenterLayoutMode::ConsoleOnly];

    pub fn label(self) -> &'static str {
        match self {
            CenterLayoutMode::Split => "Split",
            CenterLayoutMode::EditorOnly => "Editor",
            CenterLayoutMode::ConsoleOnly => "Console",
        }
    }
}

/// Right-click options on a project tree node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExplorerAction {
    RevealInFiles,
    CopyPath,
    CopyName,
    Duplicate,
    Delete,
    Rename,
}

/// What the search matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SearchMode {
    Filename,
    Content,
    Both,
}

impl SearchMode {
    pub const ALL: [SearchMode; 3] = [SearchMode::Filename, SearchMode::Content, SearchMode::Both];

    pub fn label(self) -> &'static str {
        match self {
            SearchMode::Filename => "File names",
            SearchMode::Content => "In files",
            SearchMode::Both => "Both",
        }
    }

    pub fn matches_filenames(self) -> bool {
        matches!(self, SearchMode::Filename | SearchMode::Both)
    }
    pub fn matches_content(self) -> bool {
        matches!(self, SearchMode::Content | SearchMode::Both)
    }
}

/// Where the search runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SearchScope {
    Project,
    Folder(PathBuf),
}

impl SearchScope {
    pub fn label(&self) -> String {
        match self {
            SearchScope::Project => "Entire project".into(),
            SearchScope::Folder(url) => url
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| url.to_string_lossy().into_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_defaults_to_ready() {
        let s = StatusBarState::default();
        assert_eq!(s.message, "Ready");
        assert_eq!((s.line, s.column), (1, 1));
    }

    #[test]
    fn layout_labels() {
        assert_eq!(CenterLayoutMode::Split.label(), "Split");
        assert_eq!(CenterLayoutMode::ALL.len(), 3);
    }

    #[test]
    fn search_mode_predicates() {
        assert!(SearchMode::Both.matches_filenames());
        assert!(SearchMode::Both.matches_content());
        assert!(SearchMode::Filename.matches_filenames());
        assert!(!SearchMode::Filename.matches_content());
    }

    #[test]
    fn search_scope_label() {
        assert_eq!(SearchScope::Project.label(), "Entire project");
        assert_eq!(SearchScope::Folder(PathBuf::from("/a/b/proj")).label(), "proj");
    }
}
