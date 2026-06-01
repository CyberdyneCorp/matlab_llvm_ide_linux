//! Builds the main IDE window and wires every panel to `MainViewModel`. Widgets
//! subscribe to the view models' `Property`s for updates and call verb methods
//! on user input — the views hold no application state of their own.

use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Label, ListBox, Notebook,
    Orientation, ScrolledWindow, TextView,
};

use matforge_core::models::{
    CompilerTarget, ConsoleLevel, NodeFileKind, OptimizationProfile, ProjectNode,
};
use matforge_core::services::highlighter::Language;
use matforge_core::viewmodels::{ActivityItem, MainViewModel};

use crate::highlight;
use crate::runner;

/// Build the full window content and attach it to `window`.
pub fn build(window: &ApplicationWindow, vm: Rc<MainViewModel>) {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    root.append(&build_toolbar(window, &vm));

    // Middle row: activity bar | sidebar | center | workspace
    let middle = GtkBox::new(Orientation::Horizontal, 0);
    middle.set_vexpand(true);
    middle.append(&build_activity_bar(&vm));
    middle.append(&build_sidebar(window, &vm));
    middle.append(&build_center(&vm));
    middle.append(&build_workspace(&vm));
    root.append(&middle);

    root.append(&build_status_bar(&vm));

    window.set_child(Some(&root));
}

// ---- Toolbar ---------------------------------------------------------------

fn build_toolbar(window: &ApplicationWindow, vm: &Rc<MainViewModel>) -> GtkBox {
    let toolbar = GtkBox::new(Orientation::Vertical, 2);
    toolbar.add_css_class("mf-toolbar");
    toolbar.set_margin_top(4);
    toolbar.set_margin_bottom(4);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);

    // Row 1: brand
    let brand = Label::new(Some("⬣ MatForge IDE"));
    brand.add_css_class("mf-brand");
    brand.set_halign(gtk::Align::Start);
    toolbar.append(&brand);

    // Row 2: controls
    let row = GtkBox::new(Orientation::Horizontal, 6);

    let open_btn = Button::with_label("Open Folder");
    open_btn.add_css_class("mf-toolbar-button");
    {
        let vm = vm.clone();
        let window = window.clone();
        open_btn.connect_clicked(move |_| pick_folder(&window, &vm));
    }
    row.append(&open_btn);

    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("mf-toolbar-button");
    {
        let vm = vm.clone();
        save_btn.connect_clicked(move |_| save_active(&vm));
    }
    row.append(&save_btn);

    row.append(&Separator());

    // Target + optimization pickers
    let target_labels: Vec<&str> = CompilerTarget::ALL.iter().map(|t| t.label()).collect();
    let target_dd = DropDown::from_strings(&target_labels);
    {
        let vm = vm.clone();
        target_dd.connect_selected_notify(move |dd| {
            vm.toolbar.set_target(CompilerTarget::ALL[dd.selected() as usize]);
        });
    }
    row.append(&Label::new(Some("Target:")));
    row.append(&target_dd);

    let opt_labels: Vec<&str> = OptimizationProfile::ALL.iter().map(|o| o.label()).collect();
    let opt_dd = DropDown::from_strings(&opt_labels);
    {
        let vm = vm.clone();
        opt_dd.connect_selected_notify(move |dd| {
            vm.toolbar.set_optimization(OptimizationProfile::ALL[dd.selected() as usize]);
        });
    }
    row.append(&opt_dd);

    row.append(&Separator());

    let compile_btn = Button::with_label("Compile");
    compile_btn.add_css_class("mf-toolbar-button");
    {
        let vm = vm.clone();
        compile_btn.connect_clicked(move |_| runner::compile(&vm));
    }
    row.append(&compile_btn);

    let run_btn = Button::with_label("▶ Run");
    run_btn.add_css_class("mf-run");
    run_btn.add_css_class("mf-toolbar-button");
    {
        let vm = vm.clone();
        run_btn.connect_clicked(move |_| {
            let settings = vm.settings.clone();
            runner::run(&vm, &settings);
        });
    }
    row.append(&run_btn);

    toolbar.append(&row);
    toolbar
}

#[allow(non_snake_case)]
fn Separator() -> gtk::Separator {
    gtk::Separator::new(Orientation::Vertical)
}

// ---- Activity bar ----------------------------------------------------------

fn build_activity_bar(vm: &Rc<MainViewModel>) -> GtkBox {
    let bar = GtkBox::new(Orientation::Vertical, 4);
    bar.add_css_class("mf-activity-bar");
    bar.set_size_request(56, -1);
    bar.set_margin_top(6);

    for item in ActivityItem::ALL {
        let btn = Button::with_label(short_caption(item));
        btn.add_css_class("mf-activity-item");
        btn.set_has_frame(false);
        let vm = vm.clone();
        btn.connect_clicked(move |_| vm.activity_bar.select(item));
        bar.append(&btn);
    }
    bar
}

fn short_caption(item: ActivityItem) -> &'static str {
    match item {
        ActivityItem::Explorer => "Files",
        ActivityItem::Search => "Find",
        ActivityItem::Run => "Run",
        ActivityItem::Compiler => "Comp",
        ActivityItem::Hdl => "HDL",
        ActivityItem::Debug => "Dbg",
        ActivityItem::Docs => "Docs",
        ActivityItem::Flowchart => "Flow",
    }
}

// ---- Sidebar (Explorer) ----------------------------------------------------

fn build_sidebar(_window: &ApplicationWindow, vm: &Rc<MainViewModel>) -> GtkBox {
    let sidebar = GtkBox::new(Orientation::Vertical, 4);
    sidebar.add_css_class("mf-panel");
    sidebar.add_css_class("mf-border-right");
    sidebar.set_size_request(220, -1);

    let header = Label::new(Some("EXPLORER"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    sidebar.append(&header);

    let list = ListBox::new();
    list.add_css_class("mf-panel");
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    sidebar.append(&scroll);

    // Rebuild rows whenever the project root changes.
    let vm_sub = vm.clone();
    let list_sub = list.clone();
    vm.project.root.bind(move |root| {
        while let Some(child) = list_sub.first_child() {
            list_sub.remove(&child);
        }
        if let Some(node) = root {
            for child in &node.children {
                append_node_rows(&list_sub, child, 0, &vm_sub);
            }
        }
    });
    sidebar
}

fn append_node_rows(list: &ListBox, node: &ProjectNode, depth: i32, vm: &Rc<MainViewModel>) {
    let row = GtkBox::new(Orientation::Horizontal, 4);
    row.set_margin_start(8 + depth * 12);
    let glyph = if node.is_folder() {
        if node.is_expanded { "▾ 📁" } else { "▸ 📁" }
    } else {
        file_glyph(node.kind)
    };
    let btn = Button::with_label(&format!("{glyph} {}", node.name));
    btn.set_has_frame(false);
    btn.set_halign(gtk::Align::Start);
    btn.add_css_class("mf-row");
    {
        let vm = vm.clone();
        let id = node.id;
        let url = node.url.clone();
        let is_folder = node.is_folder();
        btn.connect_clicked(move |_| {
            if is_folder {
                vm.project.toggle_expand(id);
            } else if let Some(path) = &url {
                open_file_in_editor(&vm, path);
            }
        });
    }
    row.append(&btn);
    list.append(&row);

    if node.is_folder() && node.is_expanded {
        for child in &node.children {
            append_node_rows(list, child, depth + 1, vm);
        }
    }
}

fn file_glyph(kind: NodeFileKind) -> &'static str {
    match kind {
        NodeFileKind::Matlab => "ƒ",
        NodeFileKind::Header => "h",
        NodeFileKind::Source => "</>",
        NodeFileKind::Build => "⚒",
        NodeFileKind::Flowchart => "◇",
        _ => "·",
    }
}

// ---- Center (editor + console) --------------------------------------------

thread_local! {
    static EDITOR_NB: std::cell::RefCell<Option<Notebook>> = const { std::cell::RefCell::new(None) };
}

fn build_center(vm: &Rc<MainViewModel>) -> GtkBox {
    let center = GtkBox::new(Orientation::Vertical, 0);
    center.set_hexpand(true);

    let editor_nb = Notebook::new();
    editor_nb.set_vexpand(true);
    editor_nb.add_css_class("mf-editor");
    EDITOR_NB.with(|e| *e.borrow_mut() = Some(editor_nb.clone()));

    center.append(&editor_nb);
    center.append(&build_console(vm));
    center
}

/// Public wrapper so `main` can open a file at startup (demo / verification).
pub fn open_file_path(vm: &Rc<MainViewModel>, path: &Path) {
    open_file_in_editor(vm, path);
}

fn open_file_in_editor(vm: &Rc<MainViewModel>, path: &Path) {
    let Ok(id) = vm.open_file(path) else {
        vm.console.log(ConsoleLevel::Error, format!("could not open {}", path.display()));
        return;
    };
    let Some(tab) = vm.editor.active_tab() else { return };

    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        let Some(nb) = nb.as_ref() else { return };

        let view = TextView::new();
        view.set_monospace(true);
        view.add_css_class("mf-code");
        let buffer = view.buffer();
        buffer.set_text(&tab.contents);
        let language = Language::from_label(&tab.language);
        highlight::apply(&buffer, language);

        // Re-highlight + sync content/dirty on edits.
        {
            let vm = vm.clone();
            buffer.connect_changed(move |b| {
                highlight::apply(b, language);
                let text = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
                vm.editor.update_contents(id, text);
            });
        }
        // Cursor position → status bar.
        {
            let vm = vm.clone();
            buffer.connect_cursor_position_notify(move |b| {
                let iter = b.iter_at_offset(b.cursor_position());
                vm.status_bar.set_cursor(iter.line() as usize + 1, iter.line_offset() as usize + 1);
            });
        }

        let scroll = ScrolledWindow::new();
        scroll.set_child(Some(&view));
        scroll.set_vexpand(true);
        let label = Label::new(Some(&tab.name));
        let page = nb.append_page(&scroll, Some(&label));
        nb.set_current_page(Some(page));
    });
}

fn save_active(vm: &Rc<MainViewModel>) {
    let Some(tab) = vm.editor.active_tab() else { return };
    let Some(url) = tab.url else {
        vm.status_bar.set_message("Save As is not wired yet");
        return;
    };
    match std::fs::write(&url, &tab.contents) {
        Ok(()) => {
            vm.editor.mark_saved(tab.id);
            vm.status_bar.set_message(format!("Saved {}", url.display()));
        }
        Err(e) => vm.console.log(ConsoleLevel::Error, format!("save failed: {e}")),
    }
}

// ---- Console + REPL --------------------------------------------------------

fn build_console(vm: &Rc<MainViewModel>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-top");
    panel.set_size_request(-1, 220);

    let nb = Notebook::new();
    nb.set_vexpand(true);

    // CONSOLE tab — shows compile logs + REPL transcript combined.
    let console_view = TextView::new();
    console_view.set_monospace(true);
    console_view.set_editable(false);
    console_view.add_css_class("mf-code");
    let console_scroll = ScrolledWindow::new();
    console_scroll.set_child(Some(&console_view));
    nb.append_page(&console_scroll, Some(&Label::new(Some("CONSOLE"))));

    let render = {
        let vm = vm.clone();
        let buf = console_view.buffer();
        move || {
            let mut text = String::new();
            for m in vm.console.messages.get() {
                text.push_str(&m.text);
                text.push('\n');
            }
            for m in vm.repl.transcript.get() {
                text.push_str(&m.text);
                text.push('\n');
            }
            buf.set_text(&text);
        }
    };
    {
        let render = render.clone();
        vm.console.messages.subscribe(move |_| render());
    }
    {
        let render = render.clone();
        vm.repl.transcript.subscribe(move |_| render());
    }

    // Artifact tabs appear as buffers are populated.
    {
        let nb_artifacts = nb.clone();
        vm.console.artifacts.subscribe(move |artifacts| {
            // Remove all pages after CONSOLE, then re-add current artifacts.
            while nb_artifacts.n_pages() > 1 {
                nb_artifacts.remove_page(Some(nb_artifacts.n_pages() - 1));
            }
            for (tab, text) in artifacts {
                let view = TextView::new();
                view.set_monospace(true);
                view.set_editable(false);
                view.add_css_class("mf-code");
                view.buffer().set_text(text);
                let scroll = ScrolledWindow::new();
                scroll.set_child(Some(&view));
                nb_artifacts.append_page(&scroll, Some(&Label::new(Some(tab.label()))));
            }
        });
    }

    panel.append(&nb);

    // REPL input row.
    let input_row = GtkBox::new(Orientation::Horizontal, 4);
    let prompt = Label::new(Some(">>"));
    prompt.add_css_class("mf-text-secondary");
    let entry = Entry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("MATLAB command…"));
    {
        let vm = vm.clone();
        let entry2 = entry.clone();
        entry.connect_activate(move |_| {
            vm.repl.input.set(entry2.text().to_string());
            if vm.repl.submit().is_some() {
                entry2.set_text("");
                vm.repl
                    .transcript
                    .update(|t| t.push(matforge_core::models::ConsoleMessage::new(
                        ConsoleLevel::Info,
                        "(live REPL evaluation lands in a later phase)",
                    )));
            }
        });
    }
    input_row.append(&prompt);
    input_row.append(&entry);
    input_row.set_margin_start(8);
    input_row.set_margin_end(8);
    input_row.set_margin_top(2);
    input_row.set_margin_bottom(2);
    panel.append(&input_row);

    panel
}

// ---- Workspace -------------------------------------------------------------

fn build_workspace(vm: &Rc<MainViewModel>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-left");
    panel.set_size_request(380, -1);

    let header = Label::new(Some("WORKSPACE"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    panel.append(&header);

    let list = ListBox::new();
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    let vm_sub = vm.clone();
    vm.workspace.variables.bind(move |vars| {
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        for v in vars {
            let label = Label::new(Some(&format!(
                "{:<12} {:<8} {}",
                v.name,
                v.size,
                v.dtype.display_name()
            )));
            label.set_halign(gtk::Align::Start);
            label.add_css_class("mf-row");
            let row = GtkBox::new(Orientation::Horizontal, 0);
            row.append(&label);
            list.append(&row);
        }
        let _ = &vm_sub;
    });
    panel
}

// ---- Status bar ------------------------------------------------------------

fn build_status_bar(vm: &Rc<MainViewModel>) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 12);
    bar.add_css_class("mf-status-bar");
    bar.set_size_request(-1, 22);

    let label = Label::new(Some("Ready"));
    label.set_margin_start(8);
    bar.append(&label);

    vm.status_bar.state.bind(move |s| {
        label.set_text(&format!(
            "Ln {}, Col {}   |   {}   |   {}   |   {}",
            s.line, s.column, s.message, s.language, s.encoding
        ));
    });
    bar
}

// ---- Folder picker ---------------------------------------------------------

fn pick_folder(window: &ApplicationWindow, vm: &Rc<MainViewModel>) {
    let dialog = gtk::FileDialog::builder().title("Open Folder").build();
    let vm = vm.clone();
    dialog.select_folder(Some(window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                let _ = vm.open_folder(&path);
            }
        }
    });
}
