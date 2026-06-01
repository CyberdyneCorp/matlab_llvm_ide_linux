//! Project tree view model. Mirrors `ProjectExplorerViewModel`: it scans a
//! chosen folder into a `ProjectNode` tree via the `FileSystem` service, tracks
//! the selected node, and toggles folder expansion.

use std::path::{Path, PathBuf};

use crate::models::ProjectNode;
use crate::observable::Property;
use crate::services::filesystem::{scan_tree, FileSystem};

pub struct ProjectExplorerViewModel {
    pub root: Property<Option<ProjectNode>>,
    pub root_url: Property<Option<PathBuf>>,
    pub selected_id: Property<Option<u64>>,
}

impl Default for ProjectExplorerViewModel {
    fn default() -> Self {
        ProjectExplorerViewModel::new()
    }
}

impl ProjectExplorerViewModel {
    pub fn new() -> ProjectExplorerViewModel {
        ProjectExplorerViewModel {
            root: Property::new(None),
            root_url: Property::new(None),
            selected_id: Property::new(None),
        }
    }

    /// Scan `path` into the tree and make it the active root.
    pub fn open_folder(&self, fs: &dyn FileSystem, path: &Path) -> std::io::Result<()> {
        let tree = scan_tree(fs, path, 16)?;
        self.root.set(Some(tree));
        self.root_url.set(Some(path.to_path_buf()));
        self.selected_id.set(None);
        Ok(())
    }

    /// Re-scan the current root (e.g. the refresh button).
    pub fn refresh(&self, fs: &dyn FileSystem) -> std::io::Result<()> {
        if let Some(url) = self.root_url.get() {
            let tree = scan_tree(fs, &url, 16)?;
            self.root.set(Some(tree));
        }
        Ok(())
    }

    pub fn select(&self, id: u64) {
        self.selected_id.set(Some(id));
    }

    pub fn selected_node(&self) -> Option<ProjectNode> {
        let id = self.selected_id.get()?;
        self.root.with(|r| r.as_ref().and_then(|node| find_node(node, id).cloned()))
    }

    /// Toggle the expansion of the folder node with `id`.
    pub fn toggle_expand(&self, id: u64) {
        self.root.update(|root| {
            if let Some(node) = root.as_mut() {
                toggle_node(node, id);
            }
        });
    }
}

fn find_node(node: &ProjectNode, id: u64) -> Option<&ProjectNode> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|c| find_node(c, id))
}

fn toggle_node(node: &mut ProjectNode, id: u64) -> bool {
    if node.id == id {
        node.is_expanded = !node.is_expanded;
        return true;
    }
    node.children.iter_mut().any(|c| toggle_node(c, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::FakeFileSystem;

    fn fs() -> FakeFileSystem {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/proj/main.m", "x=1;").add_file("/proj/src/util.m", "y=2;").add_dir("/proj");
        fs
    }

    #[test]
    fn open_folder_builds_tree() {
        let vm = ProjectExplorerViewModel::new();
        vm.open_folder(&fs(), Path::new("/proj")).unwrap();
        let root = vm.root.get().unwrap();
        assert_eq!(root.name, "proj");
        assert!(!root.children.is_empty());
        assert_eq!(vm.root_url.get(), Some(PathBuf::from("/proj")));
    }

    #[test]
    fn selection_finds_node() {
        let vm = ProjectExplorerViewModel::new();
        vm.open_folder(&fs(), Path::new("/proj")).unwrap();
        let main_id = vm.root.with(|r| {
            r.as_ref().unwrap().children.iter().find(|c| c.name == "main.m").unwrap().id
        });
        vm.select(main_id);
        assert_eq!(vm.selected_node().unwrap().name, "main.m");
    }

    #[test]
    fn toggle_expand_flips_folder() {
        let vm = ProjectExplorerViewModel::new();
        vm.open_folder(&fs(), Path::new("/proj")).unwrap();
        let src_id = vm.root.with(|r| {
            r.as_ref().unwrap().children.iter().find(|c| c.name == "src").unwrap().id
        });
        let before = vm.root.with(|r| {
            r.as_ref().unwrap().children.iter().find(|c| c.id == src_id).unwrap().is_expanded
        });
        vm.toggle_expand(src_id);
        let after = vm.root.with(|r| {
            r.as_ref().unwrap().children.iter().find(|c| c.id == src_id).unwrap().is_expanded
        });
        assert_ne!(before, after);
    }

    #[test]
    fn refresh_rescans_root() {
        let vm = ProjectExplorerViewModel::new();
        vm.open_folder(&fs(), Path::new("/proj")).unwrap();
        // refresh should not error and keep the tree
        vm.refresh(&fs()).unwrap();
        assert!(vm.root.get().is_some());
    }
}
