//! File-system access behind a trait so the Explorer view model can be tested
//! against an in-memory fake. Includes a pure tree scanner that turns a
//! directory into a `ProjectNode` tree (classifying files by extension and
//! peeking `.mflow` dialect). Mirrors `FileSystemService.swift`.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use crate::models::flowchart::SchemaKind;
use crate::models::{NodeFileKind, ProjectNode};
use crate::services::flowchart_codec;

/// One directory entry returned by [`FileSystem::read_dir`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Abstracts file/directory access for the view models.
pub trait FileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
    /// Immediate children of `path` (not recursive), sorted folders-first.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>>;
    fn exists(&self, path: &Path) -> bool;
}

/// Real file system via `std::fs`.
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        std::fs::write(path, contents)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut entries = Vec::new();
        for e in std::fs::read_dir(path)? {
            let e = e?;
            entries.push(DirEntry { path: e.path(), is_dir: e.file_type()?.is_dir() });
        }
        sort_entries(&mut entries);
        Ok(entries)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// In-memory file system for tests.
#[derive(Default)]
pub struct FakeFileSystem {
    files: BTreeMap<PathBuf, String>,
    dirs: BTreeSet<PathBuf>,
}

impl FakeFileSystem {
    pub fn new() -> FakeFileSystem {
        FakeFileSystem::default()
    }

    /// Add a file (creating ancestor directories).
    pub fn add_file(&mut self, path: impl Into<PathBuf>, contents: impl Into<String>) -> &mut Self {
        let path = path.into();
        let mut ancestor = path.parent();
        while let Some(dir) = ancestor {
            self.dirs.insert(dir.to_path_buf());
            ancestor = dir.parent();
        }
        self.files.insert(path, contents.into());
        self
    }

    pub fn add_dir(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.dirs.insert(path.into());
        self
    }
}

impl FileSystem for FakeFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such file"))
    }
    fn write(&self, _path: &Path, _contents: &str) -> io::Result<()> {
        // Writes are recorded by the real FS; the fake treats them as no-ops
        // unless a test wraps it. Kept &self so VMs can hold it behind an Rc.
        Ok(())
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        if !self.dirs.contains(path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such dir"));
        }
        let mut entries = Vec::new();
        for d in &self.dirs {
            if d.parent() == Some(path) {
                entries.push(DirEntry { path: d.clone(), is_dir: true });
            }
        }
        for f in self.files.keys() {
            if f.parent() == Some(path) {
                entries.push(DirEntry { path: f.clone(), is_dir: false });
            }
        }
        sort_entries(&mut entries);
        Ok(entries)
    }
    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.dirs.contains(path)
    }
}

fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
}

/// Recursively scan `root` into a `ProjectNode` tree, up to `max_depth` levels.
/// Files are classified by extension; `.mflow` files have their dialect peeked.
pub fn scan_tree(fs: &dyn FileSystem, root: &Path, max_depth: usize) -> io::Result<ProjectNode> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let mut node = ProjectNode::folder(name, Vec::new()).with_url(root.to_path_buf());
    node.children = scan_children(fs, root, max_depth)?;
    Ok(node)
}

fn scan_children(fs: &dyn FileSystem, dir: &Path, depth: usize) -> io::Result<Vec<ProjectNode>> {
    let mut out = Vec::new();
    for entry in fs.read_dir(dir)? {
        let name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with('.') {
            continue; // skip dotfiles
        }
        if entry.is_dir {
            let mut child = ProjectNode::folder(name, Vec::new()).with_url(entry.path.clone());
            // Subfolders start collapsed; the user expands them on demand. The
            // children are still scanned so expanding is instant.
            child.is_expanded = false;
            if depth > 0 {
                child.children = scan_children(fs, &entry.path, depth - 1)?;
            }
            out.push(child);
        } else {
            let ext = entry.path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            let kind = NodeFileKind::from_extension(&ext);
            let mut child = ProjectNode::new(name, kind).with_url(entry.path.clone());
            if kind == NodeFileKind::Flowchart {
                child.mflow_kind = peek_mflow_kind(fs, &entry.path);
            }
            out.push(child);
        }
    }
    Ok(out)
}

/// Best-effort read of a `.mflow` file's dialect for the Explorer badge.
fn peek_mflow_kind(fs: &dyn FileSystem, path: &Path) -> Option<SchemaKind> {
    let text = fs.read_to_string(path).ok()?;
    let doc = flowchart_codec::decode_str(&text).ok()?;
    Some(doc.schema_kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::FlowchartDocument;

    fn sample_fs() -> FakeFileSystem {
        let mut fs = FakeFileSystem::new();
        fs.add_file("/proj/main.m", "x = 1;")
            .add_file("/proj/src/util.m", "y = 2;")
            .add_file("/proj/src/diagram.mflow", "")
            .add_file("/proj/.hidden", "secret")
            .add_dir("/proj");
        fs
    }

    #[test]
    fn fake_reads_and_lists() {
        let fs = sample_fs();
        assert_eq!(fs.read_to_string(Path::new("/proj/main.m")).unwrap(), "x = 1;");
        assert!(fs.exists(Path::new("/proj/src")));
        let entries = fs.read_dir(Path::new("/proj")).unwrap();
        // folder "src" sorts before files
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, PathBuf::from("/proj/src"));
    }

    #[test]
    fn read_missing_file_errors() {
        let fs = FakeFileSystem::new();
        assert!(fs.read_to_string(Path::new("/nope")).is_err());
    }

    #[test]
    fn scan_tree_builds_classified_tree() {
        let fs = sample_fs();
        let tree = scan_tree(&fs, Path::new("/proj"), 8).unwrap();
        assert!(tree.is_folder());
        // src folder + main.m, hidden skipped
        let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"src"));
        assert!(names.contains(&"main.m"));
        assert!(!names.contains(&".hidden"));
        let src = tree.children.iter().find(|c| c.name == "src").unwrap();
        let mflow = src.children.iter().find(|c| c.name == "diagram.mflow").unwrap();
        assert_eq!(mflow.kind, NodeFileKind::Flowchart);
    }

    #[test]
    fn scanned_subfolders_start_collapsed() {
        let fs = sample_fs();
        let tree = scan_tree(&fs, Path::new("/proj"), 8).unwrap();
        let src = tree.children.iter().find(|c| c.name == "src").unwrap();
        assert!(!src.is_expanded, "subfolders should be collapsed by default");
        // children are still scanned so expanding is instant
        assert!(!src.children.is_empty());
    }

    #[test]
    fn scan_peeks_mflow_dialect() {
        let doc = FlowchartDocument::empty("D", SchemaKind::SignalFlow);
        let json = flowchart_codec::encode_string(&doc).unwrap();
        let mut fs = FakeFileSystem::new();
        fs.add_file("/p/d.mflow", json).add_dir("/p");
        let tree = scan_tree(&fs, Path::new("/p"), 1).unwrap();
        let mflow = &tree.children[0];
        assert_eq!(mflow.mflow_kind, Some(SchemaKind::SignalFlow));
    }

    #[test]
    fn real_filesystem_write_read_list() {
        // Exercise RealFileSystem against a unique temp directory.
        let base = std::env::temp_dir()
            .join(format!("matforge_fs_test_{}_{}", std::process::id(), crate::models::next_id()));
        std::fs::create_dir_all(base.join("sub")).unwrap();
        let fs = RealFileSystem;
        let file = base.join("a.m");
        fs.write(&file, "x = 1;").unwrap();
        assert!(fs.exists(&file));
        assert_eq!(fs.read_to_string(&file).unwrap(), "x = 1;");
        let entries = fs.read_dir(&base).unwrap();
        // folder "sub" sorts before file "a.m"
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, base.join("sub"));
        assert!(entries.iter().any(|e| e.path == file && !e.is_dir));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn real_filesystem_read_missing_errors() {
        let fs = RealFileSystem;
        assert!(fs.read_to_string(Path::new("/nonexistent/matforge/x")).is_err());
        assert!(!fs.exists(Path::new("/nonexistent/matforge/x")));
    }

    #[test]
    fn scan_respects_max_depth() {
        let fs = sample_fs();
        let tree = scan_tree(&fs, Path::new("/proj"), 0).unwrap();
        let src = tree.children.iter().find(|c| c.name == "src").unwrap();
        assert!(src.children.is_empty()); // depth 0 stops recursion
    }
}
