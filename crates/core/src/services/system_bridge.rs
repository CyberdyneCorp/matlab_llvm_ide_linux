//! Clipboard + file-picker abstractions. The view models depend on these
//! traits; the real GTK implementations live in the app crate, and the in-crate
//! fakes drive the unit tests. Mirrors `SystemBridge.swift` / `FilePickerService.swift`.

use std::cell::RefCell;
use std::path::PathBuf;

/// System clipboard write access.
pub trait Clipboard {
    fn set_text(&self, text: &str);
}

/// Records the last copied text — for asserting copy actions in tests.
#[derive(Default)]
pub struct FakeClipboard {
    pub last: RefCell<Option<String>>,
}

impl FakeClipboard {
    pub fn new() -> FakeClipboard {
        FakeClipboard::default()
    }
}

impl Clipboard for FakeClipboard {
    fn set_text(&self, text: &str) {
        *self.last.borrow_mut() = Some(text.to_string());
    }
}

/// Native open/save dialogs. Real impl is async/GTK in the app; the view models
/// treat picking as "returns a path or None" and are tested with the fake.
pub trait FilePicker {
    fn open_file(&self) -> Option<PathBuf>;
    fn open_folder(&self) -> Option<PathBuf>;
    fn save_file(&self, suggested_name: &str) -> Option<PathBuf>;
}

/// Scripted picker: pops queued responses in order.
#[derive(Default)]
pub struct FakeFilePicker {
    pub open_file: RefCell<Vec<PathBuf>>,
    pub open_folder: RefCell<Vec<PathBuf>>,
    pub save_file: RefCell<Vec<PathBuf>>,
    pub save_suggestions: RefCell<Vec<String>>,
}

impl FakeFilePicker {
    pub fn new() -> FakeFilePicker {
        FakeFilePicker::default()
    }
    pub fn queue_open_file(&self, path: impl Into<PathBuf>) {
        self.open_file.borrow_mut().push(path.into());
    }
    pub fn queue_open_folder(&self, path: impl Into<PathBuf>) {
        self.open_folder.borrow_mut().push(path.into());
    }
    pub fn queue_save_file(&self, path: impl Into<PathBuf>) {
        self.save_file.borrow_mut().push(path.into());
    }
}

impl FilePicker for FakeFilePicker {
    fn open_file(&self) -> Option<PathBuf> {
        pop_front(&self.open_file)
    }
    fn open_folder(&self) -> Option<PathBuf> {
        pop_front(&self.open_folder)
    }
    fn save_file(&self, suggested_name: &str) -> Option<PathBuf> {
        self.save_suggestions.borrow_mut().push(suggested_name.to_string());
        pop_front(&self.save_file)
    }
}

fn pop_front(q: &RefCell<Vec<PathBuf>>) -> Option<PathBuf> {
    let mut q = q.borrow_mut();
    if q.is_empty() {
        None
    } else {
        Some(q.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_records_last_text() {
        let cb = FakeClipboard::new();
        cb.set_text("/proj/a.m");
        assert_eq!(cb.last.borrow().as_deref(), Some("/proj/a.m"));
    }

    #[test]
    fn picker_pops_queued_responses_in_order() {
        let p = FakeFilePicker::new();
        p.queue_open_file("/a.m");
        p.queue_open_file("/b.m");
        assert_eq!(p.open_file(), Some(PathBuf::from("/a.m")));
        assert_eq!(p.open_file(), Some(PathBuf::from("/b.m")));
        assert_eq!(p.open_file(), None);
    }

    #[test]
    fn save_file_records_suggestion() {
        let p = FakeFilePicker::new();
        p.queue_save_file("/out/x.m");
        assert_eq!(p.save_file("x.m"), Some(PathBuf::from("/out/x.m")));
        assert_eq!(p.save_suggestions.borrow().as_slice(), ["x.m"]);
    }

    #[test]
    fn folder_picker_independent_queue() {
        let p = FakeFilePicker::new();
        p.queue_open_folder("/proj");
        assert_eq!(p.open_folder(), Some(PathBuf::from("/proj")));
        assert_eq!(p.open_file(), None);
    }
}
