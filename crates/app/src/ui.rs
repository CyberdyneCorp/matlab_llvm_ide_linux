//! Builds the main IDE window and wires every panel to `AppState`
//! (`MainViewModel` + live REPL/DAP sessions). Widgets subscribe to the view
//! models' `Property`s and call verb methods / `AppState` commands on input.

use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Image, Label, ListBox,
    Notebook, Orientation, ScrolledWindow, Stack, TextView,
};

use matforge_core::models::{
    CompilerTarget, ConsoleLevel, NodeFileKind, NumericMode, OptimizationProfile, ProjectNode,
};
use matforge_core::services::highlighter::Language;
use matforge_core::viewmodels::{ActivityItem, DebugState, FlowchartViewModel};

use crate::app_state::AppState;
use crate::editor_view;
use crate::icons::name as ic;
use crate::runner;

/// Build the full window content and attach it to `window`.
pub fn build(window: &ApplicationWindow, app: Rc<AppState>) {
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    root.append(&build_toolbar(window, &app));

    let middle = GtkBox::new(Orientation::Horizontal, 0);
    middle.set_vexpand(true);
    middle.append(&build_activity_bar(&app));
    middle.append(&build_sidebar(&app));
    middle.append(&build_center(&app));
    middle.append(&build_right_column(&app));
    root.append(&middle);

    root.append(&build_status_bar(&app));
    window.set_child(Some(&root));
}

// ---- Toolbar ---------------------------------------------------------------

fn build_toolbar(window: &ApplicationWindow, app: &Rc<AppState>) -> GtkBox {
    let toolbar = GtkBox::new(Orientation::Vertical, 4);
    toolbar.add_css_class("mf-toolbar");
    toolbar.set_margin_top(4);
    toolbar.set_margin_bottom(2);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);

    // Brand row.
    let brand_row = GtkBox::new(Orientation::Horizontal, 6);
    let logo = Label::new(Some("M"));
    logo.add_css_class("mf-logo");
    let brand = Label::new(Some("MatForge IDE"));
    brand.add_css_class("mf-brand");
    brand_row.append(&logo);
    brand_row.append(&brand);
    toolbar.append(&brand_row);

    // Control row.
    let row = GtkBox::new(Orientation::Horizontal, 4);
    row.set_margin_top(2);

    // File group.
    let new_btn = tool_button(ic::NEW, "New", None);
    {
        let app = app.clone();
        new_btn.connect_clicked(move |_| new_untitled(&app));
    }
    let open_btn = tool_button(ic::OPEN, "Open", None);
    {
        let app = app.clone();
        let window = window.clone();
        open_btn.connect_clicked(move |_| pick_folder(&window, &app));
    }
    let save_btn = tool_button(ic::SAVE, "Save", None);
    {
        let app = app.clone();
        save_btn.connect_clicked(move |_| save_active(&app));
    }
    row.append(&new_btn);
    row.append(&open_btn);
    row.append(&save_btn);
    row.append(&sep());

    // Run / Debug / Stop group.
    let run_btn = tool_button(ic::RUN, "Run", Some("mf-run"));
    {
        let app = app.clone();
        run_btn.connect_clicked(move |_| {
            let settings = app.settings.clone();
            runner::run(&app.vm, &settings);
        });
    }
    let debug_btn = tool_button(ic::DEBUG, "Debug", Some("mf-debug"));
    {
        let app = app.clone();
        debug_btn.connect_clicked(move |_| app.start_debug());
    }
    let stop_btn = tool_button(ic::STOP, "Stop", Some("mf-stop"));
    {
        let app = app.clone();
        stop_btn.connect_clicked(move |_| {
            app.stop_debug();
            app.vm.toolbar.is_running.set(false);
        });
    }
    row.append(&run_btn);
    row.append(&debug_btn);
    row.append(&stop_btn);
    row.append(&sep());

    // Target + Compile.
    row.append(&field_label("Target:"));
    let target_dd = DropDown::from_strings(&CompilerTarget::ALL.iter().map(|t| t.label()).collect::<Vec<_>>());
    {
        let app = app.clone();
        target_dd.connect_selected_notify(move |dd| {
            app.vm.toolbar.set_target(CompilerTarget::ALL[dd.selected() as usize]);
        });
    }
    row.append(&target_dd);
    let compile_btn = tool_button(ic::COMPILE, "Compile", Some("mf-compile-cta"));
    {
        let app = app.clone();
        compile_btn.connect_clicked(move |_| runner::compile(&app.vm));
    }
    row.append(&compile_btn);
    row.append(&sep());

    // Optimization + Numeric Mode (stacked labeled dropdowns).
    let opt_col = labeled_dropdown(
        "Optimization:",
        &OptimizationProfile::ALL.iter().map(|o| o.label()).collect::<Vec<_>>(),
        {
            let app = app.clone();
            move |i| app.vm.toolbar.set_optimization(OptimizationProfile::ALL[i])
        },
    );
    let num_col = labeled_dropdown(
        "Numeric Mode:",
        &NumericMode::ALL.iter().map(|n| n.label()).collect::<Vec<_>>(),
        {
            let app = app.clone();
            move |i| app.vm.toolbar.set_numeric_mode(NumericMode::ALL[i])
        },
    );
    row.append(&opt_col);
    row.append(&num_col);

    // Right-aligned Layouts + Help.
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);
    let layouts = tool_button(ic::LAYOUTS, "Layouts", None);
    {
        let app = app.clone();
        layouts.connect_clicked(move |_| app.vm.layout.toggle_plots());
    }
    let help = tool_button(ic::HELP, "Help", None);
    row.append(&layouts);
    row.append(&help);

    toolbar.append(&row);
    toolbar
}

/// An icon-over-label flat toolbar button.
fn tool_button(icon: &str, label: &str, css: Option<&str>) -> Button {
    let btn = Button::new();
    btn.set_has_frame(false);
    btn.add_css_class("mf-tool");
    if let Some(c) = css {
        btn.add_css_class(c);
    }
    let v = GtkBox::new(Orientation::Vertical, 1);
    v.set_halign(gtk::Align::Center);
    let img = Image::from_icon_name(icon);
    img.set_pixel_size(18);
    let lbl = Label::new(Some(label));
    lbl.add_css_class("mf-tool-label");
    v.append(&img);
    v.append(&lbl);
    btn.set_child(Some(&v));
    btn
}

fn field_label(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.add_css_class("mf-text-secondary");
    l
}

fn labeled_dropdown(label: &str, items: &[&str], on_change: impl Fn(usize) + 'static) -> GtkBox {
    let col = GtkBox::new(Orientation::Vertical, 1);
    let l = Label::new(Some(label));
    l.add_css_class("mf-tool-label");
    l.set_halign(gtk::Align::Start);
    let dd = DropDown::from_strings(items);
    dd.connect_selected_notify(move |dd| on_change(dd.selected() as usize));
    col.append(&l);
    col.append(&dd);
    col
}

fn sep() -> gtk::Separator {
    let s = gtk::Separator::new(Orientation::Vertical);
    s.set_margin_start(4);
    s.set_margin_end(4);
    s
}

// ---- Activity bar ----------------------------------------------------------

fn build_activity_bar(app: &Rc<AppState>) -> GtkBox {
    let bar = GtkBox::new(Orientation::Vertical, 2);
    bar.add_css_class("mf-activity-bar");
    bar.set_size_request(60, -1);
    bar.set_margin_top(6);
    for item in ActivityItem::ALL {
        let btn = Button::new();
        btn.set_has_frame(false);
        btn.add_css_class("mf-activity-item");
        let v = GtkBox::new(Orientation::Vertical, 1);
        v.set_halign(gtk::Align::Center);
        let img = Image::from_icon_name(activity_icon(item));
        img.set_pixel_size(20);
        let lbl = Label::new(Some(item.caption()));
        lbl.add_css_class("mf-tool-label");
        v.append(&img);
        v.append(&lbl);
        btn.set_child(Some(&v));
        {
            let app = app.clone();
            btn.connect_clicked(move |_| app.vm.activity_bar.select(item));
        }
        // Selection highlight.
        let btn2 = btn.clone();
        app.vm.activity_bar.selected.bind(move |sel| {
            if *sel == item {
                btn2.add_css_class("selected");
            } else {
                btn2.remove_css_class("selected");
            }
        });
        bar.append(&btn);
    }
    bar
}

fn activity_icon(item: ActivityItem) -> &'static str {
    match item {
        ActivityItem::Explorer => ic::EXPLORER,
        ActivityItem::Search => ic::SEARCH,
        ActivityItem::Run => ic::RUN,
        ActivityItem::Compiler => ic::COMPILE,
        ActivityItem::Hdl => ic::HDL,
        ActivityItem::Debug => ic::DEBUG,
        ActivityItem::Docs => ic::DOCS,
        ActivityItem::Flowchart => ic::FLOWCHART,
    }
}

fn new_untitled(app: &Rc<AppState>) {
    let id = app.vm.editor.open_text("untitled.m", "Matlab", "");
    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        if let Some(nb) = nb.as_ref() {
            let view = editor_view::build_code_view(app, id, "", Language::Matlab);
            let label = tab_label(&view, "untitled.m", app, Some(id));
            let page = nb.append_page(&view, Some(&label));
            nb.set_current_page(Some(page));
        }
    });
}

/// A notebook tab label: file-kind icon + name + close button.
fn tab_label(content: &impl IsA<gtk::Widget>, name: &str, app: &Rc<AppState>, id: Option<u64>) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 5);
    row.add_css_class("mf-tab-label");
    let icon = Image::from_icon_name(tab_icon(name));
    icon.set_pixel_size(13);
    let lbl = Label::new(Some(name));
    let close = Button::from_icon_name(ic::CLOSE);
    close.set_has_frame(false);
    close.add_css_class("mf-tab-close");
    {
        let content = content.clone().upcast::<gtk::Widget>();
        let app = app.clone();
        close.connect_clicked(move |_| {
            EDITOR_NB.with(|nb| {
                if let Some(nb) = nb.borrow().as_ref() {
                    if let Some(p) = nb.page_num(&content) {
                        nb.remove_page(Some(p));
                    }
                }
            });
            if let Some(id) = id {
                app.vm.editor.close(id);
            }
        });
    }
    row.append(&icon);
    row.append(&lbl);
    row.append(&close);
    row
}

fn tab_icon(name: &str) -> &'static str {
    if name.ends_with(".mflow") {
        ic::FLOWCHART
    } else {
        ic::FILE
    }
}

// ---- Sidebar (Explorer ⇄ Debug) -------------------------------------------

fn build_sidebar(app: &Rc<AppState>) -> Stack {
    let stack = Stack::new();
    stack.set_size_request(220, -1);
    stack.add_css_class("mf-panel");
    stack.add_css_class("mf-border-right");
    stack.add_named(&build_explorer(app), Some("explorer"));
    stack.add_named(&build_debug_panel(app), Some("debug"));

    let stack2 = stack.clone();
    app.vm.activity_bar.selected.bind(move |item| {
        let name = if *item == ActivityItem::Debug { "debug" } else { "explorer" };
        stack2.set_visible_child_name(name);
    });
    stack
}

fn build_explorer(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    let refresh = header_action(ic::REFRESH);
    {
        let app = app.clone();
        refresh.connect_clicked(move |_| {
            let _ = app.vm.project.refresh(app.vm.fs());
        });
    }
    panel.append(&panel_header("EXPLORER", &[refresh]));

    let list = ListBox::new();
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    let app_sub = app.clone();
    app.vm.project.root.bind(move |root| {
        clear_list(&list);
        if let Some(node) = root {
            for child in &node.children {
                append_node_rows(&list, child, 0, &app_sub);
            }
        }
    });

    // CURRENT FOLDER picker.
    panel.append(&sub_header("CURRENT FOLDER"));
    let folder_lbl = Label::new(Some("—"));
    folder_lbl.add_css_class("mf-text-secondary");
    folder_lbl.set_halign(gtk::Align::Start);
    folder_lbl.set_margin_start(8);
    folder_lbl.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    let fl = folder_lbl.clone();
    app.vm.project.root_url.bind(move |url| {
        fl.set_text(&url.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "—".into()));
    });
    panel.append(&folder_lbl);

    // DETAILS panel.
    panel.append(&sub_header("DETAILS"));
    let details = Label::new(Some("No file selected"));
    details.add_css_class("mf-text-muted");
    details.set_halign(gtk::Align::Start);
    details.set_margin_start(8);
    details.set_margin_bottom(8);
    details.set_wrap(true);
    let d = details.clone();
    let app2 = app.clone();
    app.vm.project.selected_id.bind(move |_| {
        d.set_text(&describe_selection(&app2));
    });
    panel.append(&details);

    panel
}

fn describe_selection(app: &Rc<AppState>) -> String {
    let Some(node) = app.vm.project.selected_node() else {
        return "No file selected".into();
    };
    let kind = match node.kind {
        NodeFileKind::Matlab => "MATLAB Script",
        NodeFileKind::Header => "C/C++ Header",
        NodeFileKind::Source => "Source",
        NodeFileKind::Build => "Build Artifact",
        NodeFileKind::Flowchart => "Flowchart",
        NodeFileKind::Folder => "Folder",
        NodeFileKind::Generic => "File",
    };
    let mut out = format!("Name:  {}\nType:  {}", node.name, kind);
    if let Some(url) = &node.url {
        if let Ok(meta) = std::fs::metadata(url) {
            out.push_str(&format!("\nSize:  {:.1} KB", meta.len() as f64 / 1024.0));
        }
    }
    out
}

fn append_node_rows(list: &ListBox, node: &ProjectNode, depth: i32, app: &Rc<AppState>) {
    let row = GtkBox::new(Orientation::Horizontal, 5);
    row.set_margin_start(6 + depth * 12);
    let btn = Button::new();
    btn.set_has_frame(false);
    btn.set_halign(gtk::Align::Start);
    btn.add_css_class("mf-row");

    let inner = GtkBox::new(Orientation::Horizontal, 5);
    if node.is_folder() {
        let tri = Image::from_icon_name(if node.is_expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        });
        tri.set_pixel_size(10);
        inner.append(&tri);
    }
    let icon = Image::from_icon_name(file_icon(node.kind));
    icon.set_pixel_size(14);
    icon.add_css_class(file_icon_class(node.kind));
    inner.append(&icon);
    inner.append(&Label::new(Some(&node.name)));
    btn.set_child(Some(&inner));

    {
        let app = app.clone();
        let id = node.id;
        let url = node.url.clone();
        let is_folder = node.is_folder();
        btn.connect_clicked(move |_| {
            app.vm.project.select(id);
            if is_folder {
                app.vm.project.toggle_expand(id);
            } else if let Some(path) = &url {
                open_file_in_editor(&app, path);
            }
        });
    }
    row.append(&btn);
    list.append(&row);

    if node.is_folder() && node.is_expanded {
        for child in &node.children {
            append_node_rows(list, child, depth + 1, app);
        }
    }
}

fn file_icon(kind: NodeFileKind) -> &'static str {
    match kind {
        NodeFileKind::Folder => ic::FOLDER,
        NodeFileKind::Flowchart => ic::FLOWCHART,
        _ => ic::FILE,
    }
}

fn file_icon_class(kind: NodeFileKind) -> &'static str {
    match kind {
        NodeFileKind::Folder => "mf-ic-folder",
        NodeFileKind::Matlab => "mf-ic-matlab",
        NodeFileKind::Header => "mf-ic-header",
        NodeFileKind::Source => "mf-ic-source",
        NodeFileKind::Flowchart => "mf-ic-flow",
        _ => "mf-ic-generic",
    }
}

fn build_debug_panel(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.append(&section_header("DEBUG"));

    // Stepping toolbar.
    let bar = GtkBox::new(Orientation::Horizontal, 2);
    bar.set_margin_start(6);
    for (glyph, cmd) in [
        ("▶", "continue"),
        ("⏸", "pause"),
        ("⤼", "next"),
        ("⤷", "stepIn"),
        ("⤴", "stepOut"),
        ("↺", "stepBack"),
    ] {
        let b = Button::with_label(glyph);
        b.set_has_frame(false);
        b.set_tooltip_text(Some(cmd));
        let app = app.clone();
        b.connect_clicked(move |_| app.debug_command(cmd));
        bar.append(&b);
    }
    let stop = Button::with_label("⏹");
    stop.set_has_frame(false);
    stop.set_tooltip_text(Some("stop"));
    {
        let app = app.clone();
        stop.connect_clicked(move |_| app.stop_debug());
    }
    bar.append(&stop);
    panel.append(&bar);

    let state = Label::new(Some("idle"));
    state.add_css_class("mf-text-secondary");
    state.set_halign(gtk::Align::Start);
    state.set_margin_start(8);
    panel.append(&state);
    let state_lbl = state.clone();
    app.vm.debug.state.bind(move |s| {
        state_lbl.set_text(match s {
            DebugState::Idle => "idle",
            DebugState::Launching => "launching…",
            DebugState::Running => "running…",
            DebugState::Paused => "paused",
            DebugState::Terminated => "terminated",
        });
    });

    // Call stack.
    panel.append(&sub_header("CALL STACK"));
    let stack_list = ListBox::new();
    let stack_scroll = ScrolledWindow::new();
    stack_scroll.set_min_content_height(120);
    stack_scroll.set_child(Some(&stack_list));
    panel.append(&stack_scroll);
    app.vm.debug.stack_frames.bind(move |frames| {
        clear_list(&stack_list);
        for f in frames {
            let line = f.line.map(|l| format!("  (line {l})")).unwrap_or_default();
            stack_list.append(&row_label(&format!("{}{}", f.name, line)));
        }
    });

    // Locals.
    panel.append(&sub_header("LOCALS"));
    let locals_list = ListBox::new();
    let locals_scroll = ScrolledWindow::new();
    locals_scroll.set_vexpand(true);
    locals_scroll.set_child(Some(&locals_list));
    panel.append(&locals_scroll);
    app.vm.debug.locals.bind(move |locals| {
        clear_list(&locals_list);
        for v in locals {
            let ty = v.type_hint.as_deref().map(|t| format!("  [{t}]")).unwrap_or_default();
            locals_list.append(&row_label(&format!("{} = {}{}", v.name, v.value, ty)));
        }
    });

    panel
}

// ---- Center (editor + console) --------------------------------------------

thread_local! {
    static EDITOR_NB: std::cell::RefCell<Option<Notebook>> = const { std::cell::RefCell::new(None) };
}

fn build_center(app: &Rc<AppState>) -> GtkBox {
    let center = GtkBox::new(Orientation::Vertical, 0);
    center.set_hexpand(true);

    let editor_nb = Notebook::new();
    editor_nb.set_vexpand(true);
    editor_nb.add_css_class("mf-editor");
    EDITOR_NB.with(|e| *e.borrow_mut() = Some(editor_nb.clone()));

    center.append(&editor_nb);
    center.append(&build_console(app));
    center
}

/// Public so `main` can open a file at startup (demo / verification).
pub fn open_file_path(app: &Rc<AppState>, path: &Path) {
    open_file_in_editor(app, path);
}

fn open_file_in_editor(app: &Rc<AppState>, path: &Path) {
    // Flowchart documents open in the Cairo canvas, not the text editor.
    if path.extension().and_then(|e| e.to_str()) == Some("mflow") {
        open_flowchart(app, path);
        return;
    }
    let Ok(id) = app.vm.open_file(path) else {
        app.vm.console.log(ConsoleLevel::Error, format!("could not open {}", path.display()));
        return;
    };
    let Some(tab) = app.vm.editor.active_tab() else { return };
    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        let Some(nb) = nb.as_ref() else { return };
        let language = Language::from_label(&tab.language);
        let view = editor_view::build_code_view(app, id, &tab.contents, language);
        let label = tab_label(&view, &tab.name, app, Some(id));
        let page = nb.append_page(&view, Some(&label));
        nb.set_current_page(Some(page));
    });
}

/// Open a fresh demo flowchart (used for demos / verification).
pub fn open_demo_flowchart(app: &Rc<AppState>, signal: bool) {
    use matforge_core::models::flowchart::{NodeKind, SchemaKind};
    let kind = if signal { SchemaKind::SignalFlow } else { SchemaKind::ControlFlow };
    let fc = Rc::new(FlowchartViewModel::empty("Demo", kind));
    if signal {
        let c = fc.add_node(NodeKind::SignalConstant, 80.0, 120.0);
        let g = fc.add_node(NodeKind::SignalGain, 300.0, 120.0);
        let s = fc.add_node(NodeKind::SignalScope, 520.0, 120.0);
        fc.add_edge(&c, "out", &g, "in");
        fc.add_edge(&g, "out", &s, "in");
    } else {
        // Lay the template Start/End out as a clean vertical column.
        fc.set_node_position("main_start", 320.0, 30.0);
        fc.set_node_position("main_end", 300.0, 560.0);
        let a = fc.add_node(NodeKind::Assignment, 280.0, 150.0);
        let cond = fc.add_node(NodeKind::IfBlock, 250.0, 280.0);
        let disp = fc.add_node(NodeKind::Display, 280.0, 430.0);
        fc.add_edge("main_start", "out", &a, "in");
        fc.add_edge(&a, "out", &cond, "in");
        fc.add_edge(&cond, "true", &disp, "in");
        fc.add_edge(&disp, "out", "main_end", "in");
    }
    fc.select(None);
    let view = crate::flowchart_view::build_flowchart_view(fc);
    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        if let Some(nb) = nb.as_ref() {
            let label = tab_label(&view, "Demo.mflow", app, None);
            let page = nb.append_page(&view, Some(&label));
            nb.set_current_page(Some(page));
        }
    });
    app.vm.activity_bar.select(ActivityItem::Flowchart);
}

fn open_flowchart(app: &Rc<AppState>, path: &Path) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            app.vm.console.log(ConsoleLevel::Error, format!("open {}: {e}", path.display()));
            return;
        }
    };
    let doc = match matforge_core::services::flowchart_codec::decode_str(&text) {
        Ok(d) => d,
        Err(e) => {
            app.vm.console.log(ConsoleLevel::Error, format!("invalid .mflow: {e}"));
            return;
        }
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let fc = Rc::new(FlowchartViewModel::from_document(doc));
    let view = crate::flowchart_view::build_flowchart_view(fc);
    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        if let Some(nb) = nb.as_ref() {
            let label = tab_label(&view, &name, app, None);
            let page = nb.append_page(&view, Some(&label));
            nb.set_current_page(Some(page));
        }
    });
    app.vm.status_bar.set_message(format!("Opened {name}"));
}

fn save_active(app: &Rc<AppState>) {
    let Some(tab) = app.vm.editor.active_tab() else { return };
    let Some(url) = tab.url else {
        app.vm.status_bar.set_message("Save As is not wired yet");
        return;
    };
    match std::fs::write(&url, &tab.contents) {
        Ok(()) => {
            app.vm.editor.mark_saved(tab.id);
            app.vm.status_bar.set_message(format!("Saved {}", url.display()));
        }
        Err(e) => app.vm.console.log(ConsoleLevel::Error, format!("save failed: {e}")),
    }
}

// ---- Console + live REPL ---------------------------------------------------

fn build_console(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-top");
    panel.set_size_request(-1, 220);

    let nb = Notebook::new();
    nb.set_vexpand(true);

    let console_view = TextView::new();
    console_view.set_monospace(true);
    console_view.set_editable(false);
    console_view.add_css_class("mf-code");
    let console_scroll = ScrolledWindow::new();
    console_scroll.set_child(Some(&console_view));
    nb.append_page(&console_scroll, Some(&Label::new(Some("CONSOLE"))));

    let render = {
        let app = app.clone();
        let buf = console_view.buffer();
        move || {
            let mut text = String::new();
            for m in app.vm.console.messages.get() {
                text.push_str(&m.text);
                text.push('\n');
            }
            for m in app.vm.repl.transcript.get() {
                text.push_str(&m.text);
                text.push('\n');
            }
            buf.set_text(&text);
        }
    };
    {
        let render = render.clone();
        app.vm.console.messages.subscribe(move |_| render());
    }
    {
        let render = render.clone();
        app.vm.repl.transcript.subscribe(move |_| render());
    }

    {
        let nb_artifacts = nb.clone();
        app.vm.console.artifacts.subscribe(move |artifacts| {
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

    // Live REPL input.
    let input_row = GtkBox::new(Orientation::Horizontal, 4);
    let prompt = Label::new(Some(">>"));
    prompt.add_css_class("mf-text-secondary");
    let entry = Entry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("MATLAB command…"));
    {
        let app = app.clone();
        let entry2 = entry.clone();
        entry.connect_activate(move |_| {
            app.vm.repl.input.set(entry2.text().to_string());
            if let Some(cmd) = app.vm.repl.submit() {
                entry2.set_text("");
                app.repl_send(&cmd);
            }
        });
    }
    // ↑/↓ history recall.
    let key = gtk::EventControllerKey::new();
    {
        let app = app.clone();
        let entry2 = entry.clone();
        key.connect_key_pressed(move |_c, keyval, _code, _state| {
            match keyval {
                gtk::gdk::Key::Up => {
                    app.vm.repl.recall_previous();
                    entry2.set_text(&app.vm.repl.input.get());
                    entry2.set_position(-1);
                    glib_stop()
                }
                gtk::gdk::Key::Down => {
                    app.vm.repl.recall_next();
                    entry2.set_text(&app.vm.repl.input.get());
                    entry2.set_position(-1);
                    glib_stop()
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
    }
    entry.add_controller(key);
    input_row.append(&prompt);
    input_row.append(&entry);
    input_row.set_margin_start(8);
    input_row.set_margin_end(8);
    input_row.set_margin_top(2);
    input_row.set_margin_bottom(2);
    panel.append(&input_row);
    panel
}

fn glib_stop() -> gtk::glib::Propagation {
    gtk::glib::Propagation::Stop
}

// ---- Right column (Workspace ⇄ Plots) -------------------------------------

fn build_right_column(app: &Rc<AppState>) -> Notebook {
    let nb = Notebook::new();
    nb.set_size_request(380, -1);
    nb.add_css_class("mf-panel");
    nb.add_css_class("mf-border-left");
    nb.append_page(&build_workspace(app), Some(&Label::new(Some("WORKSPACE"))));
    nb.append_page(&build_plots(app), Some(&Label::new(Some("PLOTS"))));

    // Surface the LIVE badge: jump to Plots when a figure arrives.
    let nb2 = nb.clone();
    app.vm.plots.figures.subscribe(move |figs| {
        if !figs.is_empty() {
            nb2.set_current_page(Some(1));
        }
    });
    nb
}

fn build_workspace(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);

    // Header with refresh action.
    let refresh = Button::from_icon_name(ic::REFRESH);
    refresh.set_has_frame(false);
    refresh.add_css_class("mf-header-action");
    {
        // A bare comment evaluates to nothing but still triggers the trailing
        // `whos` workspace-sync probe in ReplSession::send.
        let app = app.clone();
        refresh.connect_clicked(move |_| app.repl_send("% refresh"));
    }
    panel.append(&panel_header("WORKSPACE", &[refresh]));

    // Column header.
    panel.append(&ws_columns_header());

    // Table / empty-state stack.
    let table = ListBox::new();
    let tscroll = ScrolledWindow::new();
    tscroll.set_vexpand(true);
    tscroll.set_child(Some(&table));
    let empty = empty_state(
        ic::COMPILE,
        "No variables yet",
        "Start the REPL and assign a value to see it here.",
    );
    let body = Stack::new();
    body.set_vexpand(true);
    body.add_named(&tscroll, Some("table"));
    body.add_named(&empty, Some("empty"));
    panel.append(&body);

    {
        let body = body.clone();
        let app2 = app.clone();
        app.vm.workspace.variables.bind(move |vars| {
            clear_list(&table);
            for v in vars {
                let btn = ws_variable_row(v);
                let app3 = app2.clone();
                let name = v.name.clone();
                btn.connect_clicked(move |_| app3.vm.workspace.select(name.clone()));
                table.append(&btn);
            }
            body.set_visible_child_name(if vars.is_empty() { "empty" } else { "table" });
        });
    }

    // Inspector tabs.
    let insp = Notebook::new();
    insp.set_size_request(-1, 180);
    insp.append_page(&build_variable_inspector(app), Some(&Label::new(Some("VARIABLE INSPECTOR"))));
    insp.append_page(&build_matrix_viewer(app), Some(&Label::new(Some("MATRIX VIEWER"))));
    panel.append(&insp);

    panel
}

fn ws_columns_header() -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 0);
    row.add_css_class("mf-col-header");
    for (text, chars, expand) in [("NAME", 12, false), ("VALUE", 0, true), ("TYPE", 10, false), ("SIZE", 8, false)] {
        let l = Label::new(Some(text));
        l.add_css_class("mf-col-title");
        l.set_xalign(0.0);
        if expand {
            l.set_hexpand(true);
        } else {
            l.set_width_chars(chars);
        }
        row.append(&l);
    }
    row
}

fn ws_variable_row(v: &matforge_core::models::WorkspaceVariable) -> Button {
    let btn = Button::new();
    btn.set_has_frame(false);
    btn.add_css_class("mf-row");
    let row = GtkBox::new(Orientation::Horizontal, 0);
    let cells = [
        (v.name.clone(), 12, false),
        (if v.preview.is_empty() { "—".into() } else { v.preview.clone() }, 0, true),
        (v.dtype.display_name(), 10, false),
        (v.size.clone(), 8, false),
    ];
    for (text, chars, expand) in cells {
        let l = Label::new(Some(&text));
        l.set_xalign(0.0);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        if expand {
            l.set_hexpand(true);
        } else {
            l.set_width_chars(chars);
        }
        row.append(&l);
    }
    btn.set_child(Some(&row));
    btn
}

fn build_variable_inspector(app: &Rc<AppState>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 4);
    let placeholder = empty_state(ic::COMPILE, "", "Select a variable in the table above to inspect its value.");
    let label = Label::new(None);
    label.set_halign(gtk::Align::Start);
    label.set_margin_start(8);
    label.set_margin_top(8);
    label.set_wrap(true);
    v.append(&label);
    v.append(&placeholder);
    let label2 = label.clone();
    let placeholder2 = placeholder.clone();
    app.vm.workspace.selected_name.bind(move |sel| match sel {
        Some(name) => {
            label2.set_text(&format!("{name}\n\nMetadata inspection drills in via DAP when debugging."));
            placeholder2.set_visible(false);
            label2.set_visible(true);
        }
        None => {
            label2.set_visible(false);
            placeholder2.set_visible(true);
        }
    });
    v
}

fn build_matrix_viewer(app: &Rc<AppState>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 0);
    let canvas = gtk::DrawingArea::new();
    canvas.set_vexpand(true);
    canvas.set_hexpand(true);
    {
        let app = app.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            app.vm.workspace.inspected_matrix.with(|m| match m {
                Some(matrix) => crate::plot_render::draw_heatmap(ctx, w as f64, h as f64, matrix),
                None => {}
            });
        });
    }
    {
        let canvas = canvas.clone();
        app.vm.workspace.inspected_matrix.subscribe(move |_| canvas.queue_draw());
    }
    v.append(&canvas);
    v
}

fn build_plots(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 4);

    let add = header_action(ic::ADD);
    let refresh = header_action(ic::REFRESH);
    let trash = header_action(ic::TRASH);
    let clear = header_action(ic::CLEAR);
    {
        let app = app.clone();
        trash.connect_clicked(move |_| {
            if let Some(id) = app.vm.plots.selected_id.get() {
                app.vm.plots.remove(id);
            }
        });
    }
    {
        let app = app.clone();
        clear.connect_clicked(move |_| app.vm.plots.remove_all());
    }
    panel.append(&panel_header("PLOTS", &[add, refresh, trash, clear]));

    let list = ListBox::new();
    let list_scroll = ScrolledWindow::new();
    list_scroll.set_min_content_height(90);
    list_scroll.set_child(Some(&list));
    panel.append(&list_scroll);

    let canvas = gtk::DrawingArea::new();
    canvas.set_vexpand(true);
    canvas.set_hexpand(true);
    {
        let app = app.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            let figs = app.vm.plots.figures.get();
            let sel = app.vm.plots.selected_id.get();
            let figure = figs
                .iter()
                .find(|f| Some(f.id) == sel)
                .or_else(|| figs.last());
            if let Some(figure) = figure {
                crate::plot_render::draw_figure(ctx, w as f64, h as f64, figure);
            }
        });
    }
    panel.append(&canvas);

    // Rebuild the figure list + redraw when figures or selection change.
    let rebuild = {
        let app = app.clone();
        let list = list.clone();
        let canvas = canvas.clone();
        move || {
            clear_list(&list);
            for f in app.vm.plots.figures.get() {
                let btn = Button::with_label(&format!("Figure {} — {}", f.index, f.title));
                btn.set_has_frame(false);
                btn.set_halign(gtk::Align::Start);
                btn.add_css_class("mf-row");
                let app2 = app.clone();
                let id = f.id;
                btn.connect_clicked(move |_| app2.vm.plots.select(id));
                let row = GtkBox::new(Orientation::Horizontal, 0);
                row.append(&btn);
                list.append(&row);
            }
            canvas.queue_draw();
        }
    };
    {
        let rebuild = rebuild.clone();
        app.vm.plots.figures.subscribe(move |_| rebuild());
    }
    {
        let canvas = canvas.clone();
        app.vm.plots.selected_id.subscribe(move |_| canvas.queue_draw());
    }
    panel
}

// ---- Status bar ------------------------------------------------------------

fn build_status_bar(app: &Rc<AppState>) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 12);
    bar.add_css_class("mf-status-bar");
    bar.set_size_request(-1, 22);
    let label = Label::new(Some("Ready"));
    label.set_margin_start(8);
    bar.append(&label);
    app.vm.status_bar.state.bind(move |s| {
        label.set_text(&format!(
            "Ln {}, Col {}   |   {}   |   {}   |   {}",
            s.line, s.column, s.message, s.language, s.encoding
        ));
    });
    bar
}

// ---- Helpers ---------------------------------------------------------------

fn section_header(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.add_css_class("mf-panel-header");
    l.set_halign(gtk::Align::Start);
    l.set_margin_start(8);
    l.set_margin_top(6);
    l
}

/// A panel header with a title and right-aligned action buttons.
fn panel_header(title: &str, actions: &[Button]) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 2);
    row.add_css_class("mf-panel-header-row");
    row.set_margin_start(8);
    row.set_margin_end(4);
    row.set_margin_top(5);
    row.set_margin_bottom(3);
    let l = Label::new(Some(title));
    l.add_css_class("mf-panel-header");
    l.set_halign(gtk::Align::Start);
    l.set_hexpand(true);
    row.append(&l);
    for b in actions {
        row.append(b);
    }
    row
}

/// A small flat icon button for a panel header.
fn header_action(icon: &str) -> Button {
    let b = Button::from_icon_name(icon);
    b.set_has_frame(false);
    b.add_css_class("mf-header-action");
    b
}

/// A centered empty-state placeholder (icon + optional title + subtitle).
fn empty_state(icon: &str, title: &str, subtitle: &str) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 6);
    v.set_valign(gtk::Align::Center);
    v.set_halign(gtk::Align::Center);
    v.set_vexpand(true);
    let img = Image::from_icon_name(icon);
    img.set_pixel_size(36);
    img.add_css_class("mf-empty-icon");
    v.append(&img);
    if !title.is_empty() {
        let t = Label::new(Some(title));
        t.add_css_class("mf-empty-title");
        v.append(&t);
    }
    let s = Label::new(Some(subtitle));
    s.add_css_class("mf-text-muted");
    s.set_wrap(true);
    s.set_justify(gtk::Justification::Center);
    s.set_max_width_chars(28);
    v.append(&s);
    v
}

fn sub_header(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.add_css_class("mf-text-secondary");
    l.set_halign(gtk::Align::Start);
    l.set_margin_start(8);
    l.set_margin_top(4);
    l
}

fn row_label(text: &str) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 0);
    let label = Label::new(Some(text));
    label.set_halign(gtk::Align::Start);
    label.add_css_class("mf-row");
    row.append(&label);
    row
}

fn clear_list(list: &ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn pick_folder(window: &ApplicationWindow, app: &Rc<AppState>) {
    let dialog = gtk::FileDialog::builder().title("Open Folder").build();
    let app = app.clone();
    dialog.select_folder(Some(window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                let _ = app.vm.open_folder(&path);
            }
        }
    });
}
