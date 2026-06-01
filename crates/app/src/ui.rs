//! Builds the main IDE window and wires every panel to `AppState`
//! (`MainViewModel` + live REPL/DAP sessions). Widgets subscribe to the view
//! models' `Property`s and call verb methods / `AppState` commands on input.

use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Label, ListBox, Notebook,
    Orientation, ScrolledWindow, Stack, TextView,
};

use matforge_core::models::{
    CompilerTarget, ConsoleLevel, NodeFileKind, OptimizationProfile, ProjectNode,
};
use matforge_core::services::highlighter::Language;
use matforge_core::viewmodels::{ActivityItem, DebugState, FlowchartViewModel};

use crate::app_state::AppState;
use crate::editor_view;
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
    let toolbar = GtkBox::new(Orientation::Vertical, 2);
    toolbar.add_css_class("mf-toolbar");
    toolbar.set_margin_top(4);
    toolbar.set_margin_bottom(4);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);

    let brand = Label::new(Some("⬣ MatForge IDE"));
    brand.add_css_class("mf-brand");
    brand.set_halign(gtk::Align::Start);
    toolbar.append(&brand);

    let row = GtkBox::new(Orientation::Horizontal, 6);

    let open_btn = Button::with_label("Open Folder");
    open_btn.add_css_class("mf-toolbar-button");
    {
        let app = app.clone();
        let window = window.clone();
        open_btn.connect_clicked(move |_| pick_folder(&window, &app));
    }
    row.append(&open_btn);

    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("mf-toolbar-button");
    {
        let app = app.clone();
        save_btn.connect_clicked(move |_| save_active(&app));
    }
    row.append(&save_btn);
    row.append(&sep());

    let target_labels: Vec<&str> = CompilerTarget::ALL.iter().map(|t| t.label()).collect();
    let target_dd = DropDown::from_strings(&target_labels);
    {
        let app = app.clone();
        target_dd.connect_selected_notify(move |dd| {
            app.vm.toolbar.set_target(CompilerTarget::ALL[dd.selected() as usize]);
        });
    }
    row.append(&Label::new(Some("Target:")));
    row.append(&target_dd);

    let opt_labels: Vec<&str> = OptimizationProfile::ALL.iter().map(|o| o.label()).collect();
    let opt_dd = DropDown::from_strings(&opt_labels);
    {
        let app = app.clone();
        opt_dd.connect_selected_notify(move |dd| {
            app.vm.toolbar.set_optimization(OptimizationProfile::ALL[dd.selected() as usize]);
        });
    }
    row.append(&opt_dd);
    row.append(&sep());

    let compile_btn = Button::with_label("Compile");
    compile_btn.add_css_class("mf-toolbar-button");
    {
        let app = app.clone();
        compile_btn.connect_clicked(move |_| runner::compile(&app.vm));
    }
    row.append(&compile_btn);

    let run_btn = Button::with_label("▶ Run");
    run_btn.add_css_class("mf-run");
    run_btn.add_css_class("mf-toolbar-button");
    {
        let app = app.clone();
        run_btn.connect_clicked(move |_| {
            let settings = app.settings.clone();
            runner::run(&app.vm, &settings);
        });
    }
    row.append(&run_btn);

    let debug_btn = Button::with_label("🐞 Debug");
    debug_btn.add_css_class("mf-debug");
    debug_btn.add_css_class("mf-toolbar-button");
    {
        let app = app.clone();
        debug_btn.connect_clicked(move |_| {
            app.vm.activity_bar.select(ActivityItem::Debug);
            app.start_debug();
        });
    }
    row.append(&debug_btn);

    let stop_btn = Button::with_label("⏹ Stop");
    stop_btn.add_css_class("mf-stop");
    stop_btn.add_css_class("mf-toolbar-button");
    {
        let app = app.clone();
        stop_btn.connect_clicked(move |_| app.stop_debug());
    }
    row.append(&stop_btn);

    toolbar.append(&row);
    toolbar
}

fn sep() -> gtk::Separator {
    gtk::Separator::new(Orientation::Vertical)
}

// ---- Activity bar ----------------------------------------------------------

fn build_activity_bar(app: &Rc<AppState>) -> GtkBox {
    let bar = GtkBox::new(Orientation::Vertical, 4);
    bar.add_css_class("mf-activity-bar");
    bar.set_size_request(56, -1);
    bar.set_margin_top(6);
    for item in ActivityItem::ALL {
        let btn = Button::with_label(short_caption(item));
        btn.add_css_class("mf-activity-item");
        btn.set_has_frame(false);
        let app = app.clone();
        btn.connect_clicked(move |_| app.vm.activity_bar.select(item));
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
    let panel = GtkBox::new(Orientation::Vertical, 4);
    let header = section_header("EXPLORER");
    panel.append(&header);

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
    panel
}

fn append_node_rows(list: &ListBox, node: &ProjectNode, depth: i32, app: &Rc<AppState>) {
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
        let app = app.clone();
        let id = node.id;
        let url = node.url.clone();
        let is_folder = node.is_folder();
        btn.connect_clicked(move |_| {
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
        let page = nb.append_page(&view, Some(&Label::new(Some(&tab.name))));
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
            let page = nb.append_page(&view, Some(&Label::new(Some("Demo.mflow"))));
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
            let page = nb.append_page(&view, Some(&Label::new(Some(&name))));
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
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.append(&section_header("WORKSPACE"));
    let list = ListBox::new();
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);
    app.vm.workspace.variables.bind(move |vars| {
        clear_list(&list);
        for v in vars {
            list.append(&row_label(&format!(
                "{:<12} {:<8} {}",
                v.name,
                v.size,
                v.dtype.display_name()
            )));
        }
    });
    panel
}

fn build_plots(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.append(&section_header("PLOTS"));

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
