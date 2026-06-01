//! Project tree node (`ProjectNode`). Mirrors `Models.swift`'s value type used
//! by the Explorer. Synthetic `id` (see [`super::ids`]) for stable identity.

use std::path::PathBuf;

use crate::theme::{palette, Rgb};

use super::flowchart::SchemaKind;
use super::ids::next_id;

/// File-kind tag used to pick an icon + accent color in the Explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeFileKind {
    Folder,
    Matlab,
    Header,
    Source,
    Build,
    Flowchart,
    Generic,
}

impl NodeFileKind {
    /// Accent color used by the Explorer row (matches `Theme.iconColor(for:)`).
    pub fn icon_color(self) -> Rgb {
        match self {
            NodeFileKind::Folder => palette::ACCENT_AMBER,
            NodeFileKind::Matlab => palette::ACCENT_ORANGE,
            NodeFileKind::Header => palette::ACCENT_BLUE,
            NodeFileKind::Source => palette::ACCENT_CYAN,
            NodeFileKind::Build => palette::ACCENT_YELLOW,
            NodeFileKind::Flowchart => palette::ACCENT_MAGENTA,
            NodeFileKind::Generic => palette::TEXT_SECONDARY,
        }
    }

    /// Classify a file by its extension (lowercased, no dot).
    pub fn from_extension(ext: &str) -> NodeFileKind {
        match ext.to_lowercase().as_str() {
            "m" => NodeFileKind::Matlab,
            "h" | "hpp" => NodeFileKind::Header,
            "c" | "cc" | "cpp" | "ll" | "py" | "ts" | "sv" | "mlir" => NodeFileKind::Source,
            "mflow" => NodeFileKind::Flowchart,
            "a" | "o" | "so" => NodeFileKind::Build,
            _ => NodeFileKind::Generic,
        }
    }
}

/// One node in the project tree (folder or file), recursive.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectNode {
    pub id: u64,
    pub name: String,
    pub kind: NodeFileKind,
    pub url: Option<PathBuf>,
    pub children: Vec<ProjectNode>,
    pub is_expanded: bool,
    /// For `.mflow` files only — the document dialect, peeked during the scan.
    pub mflow_kind: Option<SchemaKind>,
}

impl ProjectNode {
    pub fn new(name: impl Into<String>, kind: NodeFileKind) -> ProjectNode {
        ProjectNode {
            id: next_id(),
            name: name.into(),
            kind,
            url: None,
            children: Vec::new(),
            is_expanded: true,
            mflow_kind: None,
        }
    }

    pub fn folder(name: impl Into<String>, children: Vec<ProjectNode>) -> ProjectNode {
        let mut n = ProjectNode::new(name, NodeFileKind::Folder);
        n.children = children;
        n
    }

    pub fn with_url(mut self, url: PathBuf) -> ProjectNode {
        self.url = Some(url);
        self
    }

    pub fn is_folder(&self) -> bool {
        self.kind == NodeFileKind::Folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_extension_classifies() {
        assert_eq!(NodeFileKind::from_extension("m"), NodeFileKind::Matlab);
        assert_eq!(NodeFileKind::from_extension("MFLOW"), NodeFileKind::Flowchart);
        assert_eq!(NodeFileKind::from_extension("cpp"), NodeFileKind::Source);
        assert_eq!(NodeFileKind::from_extension("h"), NodeFileKind::Header);
        assert_eq!(NodeFileKind::from_extension("xyz"), NodeFileKind::Generic);
    }

    #[test]
    fn icon_color_mapping() {
        assert_eq!(NodeFileKind::Matlab.icon_color(), palette::ACCENT_ORANGE);
        assert_eq!(NodeFileKind::Flowchart.icon_color(), palette::ACCENT_MAGENTA);
    }

    #[test]
    fn nodes_have_unique_ids() {
        let a = ProjectNode::new("a", NodeFileKind::Matlab);
        let b = ProjectNode::new("b", NodeFileKind::Matlab);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn folder_holds_children_and_is_folder() {
        let f = ProjectNode::folder("src", vec![ProjectNode::new("a.m", NodeFileKind::Matlab)]);
        assert!(f.is_folder());
        assert_eq!(f.children.len(), 1);
    }
}
