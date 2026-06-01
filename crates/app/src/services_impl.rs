//! GTK-backed implementations of the core service traits that need the display
//! (clipboard) plus a no-op file picker (the app drives GTK's async file dialogs
//! directly rather than through the synchronous `FilePicker` trait, which exists
//! mainly for view-model unit tests).

use std::path::PathBuf;

use gtk::prelude::*;
use matforge_core::services::system_bridge::{Clipboard, FilePicker};

/// Clipboard backed by the default GDK display.
pub struct GtkClipboard;

impl Clipboard for GtkClipboard {
    fn set_text(&self, text: &str) {
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(text);
        }
    }
}

/// The app handles file dialogs itself (GTK4 dialogs are async), so the
/// view-model-facing picker is a no-op here.
pub struct NoopFilePicker;

impl FilePicker for NoopFilePicker {
    fn open_file(&self) -> Option<PathBuf> {
        None
    }
    fn open_folder(&self) -> Option<PathBuf> {
        None
    }
    fn save_file(&self, _suggested_name: &str) -> Option<PathBuf> {
        None
    }
}
