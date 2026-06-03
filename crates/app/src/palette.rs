//! Fuzzy pickers: the command palette (`Ctrl+Shift+P`) and quick-open
//! (`Ctrl+P`). Both ride a reusable `open_picker` — a frameless modal with a
//! search field over a fuzzy-filtered list (core `fuzzy`). Arrow keys move the
//! selection, Enter runs it, Esc closes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{ApplicationWindow, Box as GtkBox, Entry, Label, ListBox, Orientation, ScrolledWindow, Window};

use matforge_core::services::fuzzy;

use crate::app_state::AppState;

/// One pickable entry: a label to fuzzy-match and an action to run.
pub struct PickEntry {
    pub label: String,
    pub detail: Option<String>,
    pub run: Rc<dyn Fn()>,
}

impl PickEntry {
    fn new(label: impl Into<String>, run: Rc<dyn Fn()>) -> PickEntry {
        PickEntry { label: label.into(), detail: None, run }
    }
    fn with_detail(mut self, detail: impl Into<String>) -> PickEntry {
        self.detail = Some(detail.into());
        self
    }
}

/// The frameless fuzzy picker shared by the palette + quick-open.
pub fn open_picker(parent: &ApplicationWindow, placeholder: &str, entries: Vec<PickEntry>) {
    let win = Window::builder()
        .transient_for(parent)
        .modal(true)
        .decorated(false)
        .default_width(580)
        .default_height(400)
        .build();
    win.add_css_class("mf-root");

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");
    root.add_css_class("mf-palette");

    let entry = Entry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_margin_top(8);
    entry.set_margin_start(8);
    entry.set_margin_end(8);
    entry.set_margin_bottom(6);
    root.append(&entry);

    let list = ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    root.append(&scroll);
    win.set_child(Some(&root));

    let entries = Rc::new(entries);
    let filtered: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let sel = Rc::new(Cell::new(0usize));

    let rebuild = {
        let entries = entries.clone();
        let filtered = filtered.clone();
        let sel = sel.clone();
        let list = list.clone();
        move |query: &str| {
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            let pairs: Vec<(usize, &str)> =
                entries.iter().enumerate().map(|(i, e)| (i, e.label.as_str())).collect();
            let order = fuzzy::filter_sort(query, pairs, |p| p.1);
            let idxs: Vec<usize> = order.iter().map(|(i, _)| *i).collect();
            for &idx in idxs.iter().take(50) {
                list.append(&entry_row(&entries[idx]));
            }
            *filtered.borrow_mut() = idxs;
            sel.set(0);
            if let Some(row) = list.row_at_index(0) {
                list.select_row(Some(&row));
            }
        }
    };
    rebuild("");
    {
        let rebuild = rebuild.clone();
        entry.connect_changed(move |e| rebuild(&e.text()));
    }

    let run_selected = {
        let entries = entries.clone();
        let filtered = filtered.clone();
        let sel = sel.clone();
        let win = win.clone();
        move || {
            let pos = sel.get();
            if let Some(&idx) = filtered.borrow().get(pos) {
                let run = entries[idx].run.clone();
                win.close();
                run();
            }
        }
    };

    // Click a row → run it.
    {
        let sel = sel.clone();
        let run_selected = run_selected.clone();
        list.connect_row_activated(move |_l, row| {
            sel.set(row.index().max(0) as usize);
            run_selected();
        });
    }

    // Keyboard: Up/Down move, Enter runs, Esc closes — while typing in the entry.
    let keys = gtk::EventControllerKey::new();
    {
        let filtered = filtered.clone();
        let sel = sel.clone();
        let list = list.clone();
        let win = win.clone();
        let run_selected = run_selected.clone();
        keys.connect_key_pressed(move |_c, key, _code, _state| {
            use gtk::gdk::Key;
            let len = filtered.borrow().len();
            match key {
                Key::Down => {
                    if len > 0 {
                        sel.set((sel.get() + 1).min(len - 1));
                        select_row(&list, sel.get());
                    }
                    glib_stop()
                }
                Key::Up => {
                    sel.set(sel.get().saturating_sub(1));
                    select_row(&list, sel.get());
                    glib_stop()
                }
                Key::Return | Key::KP_Enter => {
                    run_selected();
                    glib_stop()
                }
                Key::Escape => {
                    win.close();
                    glib_stop()
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
    }
    entry.add_controller(keys);

    win.present();
    entry.grab_focus();
}

fn select_row(list: &ListBox, i: usize) {
    if let Some(row) = list.row_at_index(i as i32) {
        list.select_row(Some(&row));
        row.grab_focus();
    }
}

fn glib_stop() -> gtk::glib::Propagation {
    gtk::glib::Propagation::Stop
}

fn entry_row(e: &PickEntry) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 0);
    row.add_css_class("mf-palette-row");
    let label = Label::new(Some(&e.label));
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    row.append(&label);
    if let Some(detail) = &e.detail {
        let d = Label::new(Some(detail));
        d.set_halign(gtk::Align::Start);
        d.set_xalign(0.0);
        d.add_css_class("mf-text-muted");
        d.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        row.append(&d);
    }
    row
}

/// Open the command palette (every menu command + theme switches).
pub fn open_command_palette(app: &Rc<AppState>, window: &ApplicationWindow) {
    use matforge_core::theme::ThemeId;

    let mut cmds: Vec<PickEntry> = Vec::new();
    // Menu actions, reused via the window's action group.
    let act = |label: &str, name: &'static str| {
        let w = window.clone();
        PickEntry::new(label, Rc::new(move || {
            let _ = WidgetExt::activate_action(&w, name, None);
        }))
    };
    cmds.push(act("New File", "win.new"));
    cmds.push(act("Open Folder…", "win.open"));
    cmds.push(act("Save", "win.save"));
    cmds.push(act("Close Tab", "win.close-tab"));
    cmds.push(act("Search in Files", "win.find"));
    cmds.push(act("Preferences", "win.preferences"));
    cmds.push(act("Compile", "win.compile"));
    cmds.push(act("Run", "win.run"));
    cmds.push(act("Stop", "win.stop"));
    cmds.push(act("Start Debugging", "win.debug"));
    cmds.push(act("Toggle Sidebar", "win.toggle-sidebar"));
    cmds.push(act("Toggle Workspace", "win.toggle-workspace"));
    cmds.push(act("Toggle Plots", "win.toggle-plots"));
    cmds.push(act("Focus Mode", "win.toggle-zen"));
    cmds.push(act("Zoom In", "win.zoom-in"));
    cmds.push(act("Zoom Out", "win.zoom-out"));
    cmds.push(act("Reset Zoom", "win.zoom-reset"));
    cmds.push(act("Quick Open File", "win.quick-open"));
    // Theme switches.
    for id in ThemeId::ALL {
        let app = app.clone();
        cmds.push(PickEntry::new(
            format!("Theme: {}", id.label()),
            Rc::new(move || app.vm.appearance.set_theme(id)),
        ));
    }

    open_picker(window, "Type a command…", cmds);
}

/// Open the fuzzy file finder over the current project.
pub fn open_quick_open(app: &Rc<AppState>, window: &ApplicationWindow) {
    let Some(root) = app.vm.project.root_url.get() else {
        app.vm.status_bar.set_message("Open a folder to quick-open files");
        return;
    };
    let mut files = Vec::new();
    collect_files(&root, 16, &mut files);

    let entries: Vec<PickEntry> = files
        .into_iter()
        .map(|path| {
            let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().into_owned();
            let app = app.clone();
            let open_path = path.clone();
            PickEntry::new(
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                Rc::new(move || crate::ui::open_file_path(&app, &open_path)),
            )
            .with_detail(rel)
        })
        .collect();

    open_picker(window, "Go to file…", entries);
}

fn collect_files(dir: &std::path::Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            if depth > 0 {
                collect_files(&path, depth - 1, out);
            }
        } else {
            out.push(path);
        }
    }
}
