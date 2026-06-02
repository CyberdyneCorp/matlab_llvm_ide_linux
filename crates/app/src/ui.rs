//! Builds the main IDE window and wires every panel to `AppState`
//! (`MainViewModel` + live REPL/DAP sessions). Widgets subscribe to the view
//! models' `Property`s and call verb methods / `AppState` commands on input.

use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gio, ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Image, Label, ListBox,
    Notebook, Orientation, Paned, ScrolledWindow, Stack, TextView,
};

use matforge_core::models::{
    CompilerTarget, ConsoleLevel, NodeFileKind, NumericMode, OptimizationProfile, ProjectNode,
};
use matforge_core::services::compiler::DiagnosticLevel;
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

    root.append(&build_menu_bar(window, &app));
    root.append(&build_toolbar(window, &app));

    let middle = GtkBox::new(Orientation::Horizontal, 0);
    middle.set_vexpand(true);
    middle.append(&build_activity_bar(&app));

    let sidebar = build_sidebar(&app);
    let center = build_center(&app);
    let right = build_right_column(&app);

    // center | right-column — draggable divider, right keeps its size.
    let inner = Paned::new(Orientation::Horizontal);
    inner.set_wide_handle(true);
    inner.set_start_child(Some(&center));
    inner.set_end_child(Some(&right));
    inner.set_resize_start_child(true);
    inner.set_resize_end_child(false);
    inner.set_shrink_start_child(false);
    inner.set_shrink_end_child(false);
    // No fixed position: the editor expands, the right region keeps its size
    // request (workspace + plots), so nothing overflows the window.

    // sidebar | (center|right) — draggable divider, sidebar keeps its size.
    let outer = Paned::new(Orientation::Horizontal);
    outer.set_wide_handle(true);
    outer.set_hexpand(true);
    outer.set_start_child(Some(&sidebar));
    outer.set_end_child(Some(&inner));
    outer.set_resize_start_child(false);
    outer.set_resize_end_child(true);
    outer.set_shrink_start_child(false);
    outer.set_position(220);
    middle.append(&outer);

    // The whole right region is visible if either panel is; closing both hands
    // the space to the editor.
    {
        let right = right.clone();
        let plots_vis = app.vm.layout.plots_visible.clone();
        app.vm.layout.workspace_visible.bind(move |v| right.set_visible(*v || plots_vis.get()));
    }
    {
        let right = right.clone();
        let ws_vis = app.vm.layout.workspace_visible.clone();
        app.vm.layout.plots_visible.bind(move |v| right.set_visible(*v || ws_vis.get()));
    }
    {
        let sidebar = sidebar.clone();
        app.vm.layout.sidebar_visible.bind(move |v| sidebar.set_visible(*v));
    }

    root.append(&middle);
    root.append(&build_status_bar(&app));
    window.set_child(Some(&root));
}

// ---- Menu bar + actions ----------------------------------------------------

/// Register the `win.*` actions, bind their keyboard accelerators on the
/// application, and return the rendered menu bar. Mirrors the macOS reference's
/// File / Edit / View / Run / Debug / Help menus.
fn build_menu_bar(window: &ApplicationWindow, app: &Rc<AppState>) -> gtk::PopoverMenuBar {
    use gtk::gio::{Menu, SimpleAction};

    // Register a parameterless `win.<name>` action running `f`.
    let register = |name: &str, f: Rc<dyn Fn()>| {
        let act = SimpleAction::new(name, None);
        act.connect_activate(move |_, _| f());
        window.add_action(&act);
    };

    let a = app.clone();
    let w = window.clone();
    register("new", Rc::new(move || new_untitled(&a)));
    {
        let a = app.clone();
        let w2 = w.clone();
        register("open", Rc::new(move || pick_folder(&w2, &a)));
    }
    {
        let a = app.clone();
        register("save", Rc::new(move || save_active(&a)));
    }
    {
        let a = app.clone();
        register("close-tab", Rc::new(move || close_active_tab(&a)));
    }
    {
        let w2 = w.clone();
        register("quit", Rc::new(move || w2.close()));
    }
    {
        let a = app.clone();
        register(
            "find",
            Rc::new(move || {
                a.vm.activity_bar.select(ActivityItem::Search);
                a.vm.layout.sidebar_visible.set(true);
                focus_search_entry();
            }),
        );
    }
    {
        let a = app.clone();
        register("toggle-sidebar", Rc::new(move || a.vm.layout.toggle_sidebar()));
    }
    {
        let a = app.clone();
        register("toggle-workspace", Rc::new(move || a.vm.layout.toggle_workspace()));
    }
    {
        let a = app.clone();
        register("toggle-plots", Rc::new(move || a.vm.layout.toggle_plots()));
    }
    {
        let a = app.clone();
        register("compile", Rc::new(move || runner::compile(&a.vm)));
    }
    {
        let a = app.clone();
        register(
            "run",
            Rc::new(move || {
                let settings = a.settings.clone();
                runner::run(&a.vm, &settings);
            }),
        );
    }
    {
        let a = app.clone();
        register(
            "stop",
            Rc::new(move || {
                a.stop_debug();
                a.vm.toolbar.is_running.set(false);
            }),
        );
    }
    {
        let a = app.clone();
        register("debug", Rc::new(move || a.start_debug()));
    }
    for (name, cmd) in [
        ("dbg-continue", "continue"),
        ("dbg-next", "next"),
        ("dbg-step-in", "stepIn"),
        ("dbg-step-out", "stepOut"),
    ] {
        let a = app.clone();
        register(name, Rc::new(move || a.debug_command(cmd)));
    }
    {
        let a = app.clone();
        register("dbg-stop", Rc::new(move || a.stop_debug()));
    }
    {
        let w2 = w.clone();
        register("about", Rc::new(move || show_about(&w2)));
    }

    // Keyboard accelerators (shown automatically in the menu by GTK).
    if let Some(gapp) = window.application().and_then(|a| a.downcast::<gtk::Application>().ok()) {
        for (action, accels) in [
            ("win.new", &["<Ctrl>n"][..]),
            ("win.open", &["<Ctrl>o"]),
            ("win.save", &["<Ctrl>s"]),
            ("win.close-tab", &["<Ctrl>w"]),
            ("win.quit", &["<Ctrl>q"]),
            ("win.find", &["<Ctrl>f"]),
            ("win.toggle-sidebar", &["<Ctrl>b"]),
            ("win.toggle-workspace", &["<Ctrl><Shift>w"]),
            ("win.toggle-plots", &["<Ctrl><Shift>p"]),
            ("win.compile", &["<Ctrl><Shift>b"]),
            ("win.run", &["<Ctrl>r"]),
            ("win.stop", &["<Shift>F5"]),
            ("win.debug", &["F5"]),
            ("win.dbg-continue", &["F8"]),
            ("win.dbg-next", &["F10"]),
            ("win.dbg-step-in", &["F11"]),
            ("win.dbg-step-out", &["<Shift>F11"]),
        ] {
            gapp.set_accels_for_action(action, accels);
        }
    }

    let menubar = Menu::new();

    let file = Menu::new();
    file.append(Some("New File"), Some("win.new"));
    file.append(Some("Open Folder…"), Some("win.open"));
    file.append(Some("Save"), Some("win.save"));
    file.append(Some("Close Tab"), Some("win.close-tab"));
    file.append(Some("Quit"), Some("win.quit"));
    menubar.append_submenu(Some("File"), &file);

    let edit = Menu::new();
    edit.append(Some("Undo"), Some("text.undo"));
    edit.append(Some("Redo"), Some("text.redo"));
    let clip = Menu::new();
    clip.append(Some("Cut"), Some("clipboard.cut"));
    clip.append(Some("Copy"), Some("clipboard.copy"));
    clip.append(Some("Paste"), Some("clipboard.paste"));
    clip.append(Some("Select All"), Some("selection.select-all"));
    edit.append_section(None, &clip);
    let find_section = Menu::new();
    find_section.append(Some("Search in Files"), Some("win.find"));
    edit.append_section(None, &find_section);
    menubar.append_submenu(Some("Edit"), &edit);

    let view = Menu::new();
    view.append(Some("Toggle Sidebar"), Some("win.toggle-sidebar"));
    view.append(Some("Toggle Workspace"), Some("win.toggle-workspace"));
    view.append(Some("Toggle Plots"), Some("win.toggle-plots"));
    menubar.append_submenu(Some("View"), &view);

    let run_menu = Menu::new();
    run_menu.append(Some("Compile"), Some("win.compile"));
    run_menu.append(Some("Run"), Some("win.run"));
    run_menu.append(Some("Stop"), Some("win.stop"));
    menubar.append_submenu(Some("Run"), &run_menu);

    let debug = Menu::new();
    debug.append(Some("Start Debugging"), Some("win.debug"));
    debug.append(Some("Continue"), Some("win.dbg-continue"));
    debug.append(Some("Step Over"), Some("win.dbg-next"));
    debug.append(Some("Step Into"), Some("win.dbg-step-in"));
    debug.append(Some("Step Out"), Some("win.dbg-step-out"));
    debug.append(Some("Stop Debugging"), Some("win.dbg-stop"));
    menubar.append_submenu(Some("Debug"), &debug);

    let help = Menu::new();
    help.append(Some("About MatForge IDE"), Some("win.about"));
    menubar.append_submenu(Some("Help"), &help);

    let bar = gtk::PopoverMenuBar::from_model(Some(&menubar));
    bar.add_css_class("mf-menubar");
    bar
}

/// Close the current center notebook page and, if it is a text tab, its model.
fn close_active_tab(app: &Rc<AppState>) {
    let active_id = app.vm.editor.active_tab().map(|t| t.id);
    EDITOR_NB.with(|nb| {
        if let Some(nb) = nb.borrow().as_ref() {
            if let Some(p) = nb.current_page() {
                nb.remove_page(Some(p));
            }
        }
    });
    if let Some(id) = active_id {
        app.vm.editor.close(id);
    }
}

/// Modal About dialog.
fn show_about(window: &ApplicationWindow) {
    let about = gtk::AboutDialog::new();
    about.set_program_name(Some("MatForge IDE"));
    about.set_version(Some(env!("CARGO_PKG_VERSION")));
    about.set_comments(Some(
        "A Linux (Rust + GTK4) port of the MatForge IDE for the matlab_llvm compiler.",
    ));
    about.set_license_type(gtk::License::MitX11);
    about.set_transient_for(Some(window));
    about.set_modal(true);
    about.present();
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

    // Debug transport controls — shown while a debug session is active.
    let debug_controls = GtkBox::new(Orientation::Horizontal, 4);
    for (icon, label, cmd) in [
        (ic::CONTINUE, "Continue", "continue"),
        (ic::PAUSE, "Pause", "pause"),
        (ic::STEP_OVER, "Step Over", "next"),
        (ic::STEP_IN, "Step In", "stepIn"),
        (ic::STEP_OUT, "Step Out", "stepOut"),
        (ic::STEP_BACK, "Step Back", "stepBack"),
    ] {
        let b = tool_button(icon, label, None);
        let app = app.clone();
        b.connect_clicked(move |_| app.debug_command(cmd));
        debug_controls.append(&b);
    }
    debug_controls.append(&sep());
    row.append(&debug_controls);

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
        layouts.connect_clicked(move |_| app.vm.layout.toggle_workspace());
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
            btn.connect_clicked(move |_| {
                // Clicking an activity item reopens the sidebar if it was closed,
                // or toggles it shut when re-clicking the active item.
                if app.vm.activity_bar.is_selected(item) {
                    app.vm.layout.toggle_sidebar();
                } else {
                    app.vm.layout.sidebar_visible.set(true);
                    app.vm.activity_bar.select(item);
                }
            });
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
    stack.add_named(&build_search(app), Some("search"));
    stack.add_named(&build_debug_panel(app), Some("debug"));

    let stack2 = stack.clone();
    app.vm.activity_bar.selected.bind(move |item| {
        let name = match item {
            ActivityItem::Debug => "debug",
            ActivityItem::Search => "search",
            _ => "explorer",
        };
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
    let close = header_action(ic::CLOSE);
    {
        let app = app.clone();
        close.connect_clicked(move |_| app.vm.layout.sidebar_visible.set(false));
    }
    panel.append(&panel_header("EXPLORER", &[refresh, close]));

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

// ---- Search (find in files) ------------------------------------------------

fn build_search(app: &Rc<AppState>) -> GtkBox {
    use matforge_core::models::SearchMode;

    let panel = GtkBox::new(Orientation::Vertical, 0);
    let close = header_action(ic::CLOSE);
    {
        let app = app.clone();
        close.connect_clicked(move |_| app.vm.layout.sidebar_visible.set(false));
    }
    panel.append(&panel_header("SEARCH", &[close]));

    // Query entry.
    let entry = Entry::new();
    entry.set_placeholder_text(Some("Search files…"));
    entry.set_margin_start(8);
    entry.set_margin_end(8);
    entry.set_margin_top(2);
    entry.set_hexpand(true);
    SEARCH_ENTRY.with(|e| *e.borrow_mut() = Some(entry.clone()));
    crate::e2e::set_search_entry(&entry);
    panel.append(&entry);

    // Match-mode selector.
    let mode_dd = DropDown::from_strings(&SearchMode::ALL.iter().map(|m| m.label()).collect::<Vec<_>>());
    mode_dd.set_selected(SearchMode::ALL.iter().position(|m| *m == SearchMode::Both).unwrap_or(0) as u32);
    mode_dd.set_margin_start(8);
    mode_dd.set_margin_end(8);
    mode_dd.set_margin_top(4);
    panel.append(&mode_dd);

    // Run-search closure shared by the entry + mode change.
    let run_search: Rc<dyn Fn()> = {
        let app = app.clone();
        let entry = entry.clone();
        Rc::new(move || {
            let query = entry.text().to_string();
            app.vm.search.set_query(query.clone());
            if query.trim().is_empty() {
                app.vm.search.results.set(Vec::new());
                return;
            }
            let Some(root) = app.vm.project.root_url.get() else {
                app.vm.status_bar.set_message("Open a folder to search");
                return;
            };
            app.vm.search.run(app.vm.fs(), &root);
        })
    };
    {
        let run = run_search.clone();
        entry.connect_activate(move |_| run());
    }
    {
        let app = app.clone();
        let run = run_search.clone();
        mode_dd.connect_selected_notify(move |dd| {
            app.vm.search.set_mode(SearchMode::ALL[dd.selected() as usize]);
            run();
        });
    }

    // Result count.
    let count = sub_header("No results");
    panel.append(&count);

    // Results list.
    let list = ListBox::new();
    list.add_css_class("mf-search-results");
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    let app_sub = app.clone();
    app.vm.search.results.bind(move |results| {
        clear_list(&list);
        count.set_text(&match results.len() {
            0 => "No results".to_string(),
            1 => "1 result".to_string(),
            n => format!("{n} results"),
        });
        for result in results {
            list.append(&search_result_row(&app_sub, result));
        }
    });

    panel
}

/// One clickable find-in-files result: `file:line` over a trimmed preview.
fn search_result_row(app: &Rc<AppState>, result: &matforge_core::viewmodels::search::SearchResult) -> Button {
    let name = result.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let location = match result.line {
        Some(line) => format!("{name}:{line}"),
        None => name,
    };

    let btn = Button::new();
    btn.set_has_frame(false);
    btn.add_css_class("mf-row");
    let col = GtkBox::new(Orientation::Vertical, 0);
    col.set_halign(gtk::Align::Start);

    let loc = Label::new(Some(&location));
    loc.set_halign(gtk::Align::Start);
    loc.add_css_class("mf-search-loc");
    col.append(&loc);

    if result.line.is_some() {
        let preview = Label::new(Some(&result.preview));
        preview.set_halign(gtk::Align::Start);
        preview.set_ellipsize(gtk::pango::EllipsizeMode::End);
        preview.set_max_width_chars(30);
        preview.add_css_class("mf-text-muted");
        col.append(&preview);
    }
    btn.set_child(Some(&col));

    let app = app.clone();
    let file = result.path.to_string_lossy().into_owned();
    let line = result.line.unwrap_or(1);
    btn.connect_clicked(move |_| goto_problem(&app, &file, line));
    btn
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

fn build_debug_panel(app: &Rc<AppState>) -> ScrolledWindow {
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

    // Exception filters (built once — the adapter ships a static set).
    panel.append(&sub_header("FILTERS"));
    for f in app.vm.breakpoints.exception_filters.get() {
        let cb = gtk::CheckButton::with_label(&f.label);
        cb.set_margin_start(8);
        cb.set_active(f.enabled);
        let app = app.clone();
        let filter = f.filter.clone();
        cb.connect_toggled(move |c| {
            app.vm.breakpoints.set_exception_enabled(&filter, c.is_active());
            app.send_exception_breakpoints();
        });
        panel.append(&cb);
    }

    // Function breakpoints.
    panel.append(&sub_header("FUNCTION BREAKPOINTS"));
    let fn_entry = panel_entry("function name + Enter");
    {
        let app = app.clone();
        let e = fn_entry.clone();
        fn_entry.connect_activate(move |_| {
            let name = e.text().to_string();
            if !name.trim().is_empty() {
                app.vm.breakpoints.add_function(name.trim());
                app.send_function_breakpoints();
                e.set_text("");
            }
        });
    }
    panel.append(&fn_entry);
    let fn_list = ListBox::new();
    panel.append(&fn_list);
    {
        let app = app.clone();
        app.clone().vm.breakpoints.function_bps.bind(move |bps| {
            clear_list(&fn_list);
            for bp in bps {
                let row = GtkBox::new(Orientation::Horizontal, 4);
                row.set_margin_start(8);
                let lbl = Label::new(Some(&bp.name));
                lbl.set_hexpand(true);
                lbl.set_halign(gtk::Align::Start);
                let rm = Button::with_label("✕");
                rm.set_has_frame(false);
                rm.add_css_class("mf-header-action");
                let app = app.clone();
                let id = bp.id;
                rm.connect_clicked(move |_| {
                    app.vm.breakpoints.remove_function(id);
                    app.send_function_breakpoints();
                });
                row.append(&lbl);
                row.append(&rm);
                fn_list.append(&row);
            }
        });
    }

    // Line breakpoints across all tabs (click to jump).
    panel.append(&sub_header("BREAKPOINTS"));
    let bp_list = ListBox::new();
    panel.append(&bp_list);
    {
        let app = app.clone();
        app.clone().vm.editor.tabs.bind(move |tabs| {
            clear_list(&bp_list);
            for t in tabs {
                for line in t.breakpoints.keys() {
                    let btn = Button::with_label(&format!("● {}:{}", t.name, line));
                    btn.set_has_frame(false);
                    btn.set_halign(gtk::Align::Start);
                    btn.add_css_class("mf-row");
                    let app = app.clone();
                    let id = t.id;
                    let line = *line;
                    btn.connect_clicked(move |_| app.vm.editor.request_goto(id, line));
                    bp_list.append(&btn);
                }
            }
        });
    }

    // Call stack.
    panel.append(&sub_header("CALL STACK"));
    let stack_list = ListBox::new();
    panel.append(&stack_list);
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
    panel.append(&locals_list);
    app.vm.debug.locals.bind(move |locals| {
        clear_list(&locals_list);
        for v in locals {
            let ty = v.type_hint.as_deref().map(|t| format!("  [{t}]")).unwrap_or_default();
            locals_list.append(&row_label(&format!("{} = {}{}", v.name, v.value, ty)));
        }
    });

    // Watch.
    panel.append(&sub_header("WATCH"));
    let watch_entry = panel_entry("expression + Enter");
    {
        let app = app.clone();
        let e = watch_entry.clone();
        watch_entry.connect_activate(move |_| {
            let expr = e.text().to_string();
            if !expr.trim().is_empty() {
                app.evaluate_watch(expr.trim());
                e.set_text("");
            }
        });
    }
    panel.append(&watch_entry);
    let watch_list = ListBox::new();
    panel.append(&watch_list);
    app.vm.debug.evaluations.bind(move |evals| {
        clear_list(&watch_list);
        for ev in evals {
            watch_list.append(&row_label(&format!("{} = {}", ev.expression, ev.result)));
        }
    });

    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&panel));
    scroll
}

fn panel_entry(placeholder: &str) -> Entry {
    let e = Entry::new();
    e.set_placeholder_text(Some(placeholder));
    e.set_margin_start(8);
    e.set_margin_end(8);
    e
}

// ---- Center (editor + console) --------------------------------------------

thread_local! {
    static EDITOR_NB: std::cell::RefCell<Option<Notebook>> = const { std::cell::RefCell::new(None) };
    static SEARCH_ENTRY: std::cell::RefCell<Option<Entry>> = const { std::cell::RefCell::new(None) };
}

/// Focus the find-in-files entry (used by the `Ctrl+F` action).
fn focus_search_entry() {
    SEARCH_ENTRY.with(|e| {
        if let Some(entry) = e.borrow().as_ref() {
            entry.grab_focus();
        }
    });
}

fn build_center(app: &Rc<AppState>) -> GtkBox {
    let center = GtkBox::new(Orientation::Vertical, 0);
    center.set_hexpand(true);

    let editor_nb = Notebook::new();
    editor_nb.set_vexpand(true);
    editor_nb.add_css_class("mf-editor");
    EDITOR_NB.with(|e| *e.borrow_mut() = Some(editor_nb.clone()));

    let console = build_console(app);
    center.append(&editor_nb);
    center.append(&console);

    // Command-window mode: with nothing open in the center notebook (no source
    // tab and no flowchart), hide it and let the console (the MATLAB command
    // window / REPL workspace) fill the center. Driven by the notebook's page
    // count so it accounts for flowchart pages, which are not editor.tabs.
    let update_center = {
        let editor_nb = editor_nb.clone();
        let console = console.clone();
        move || {
            if editor_nb.n_pages() == 0 {
                editor_nb.set_visible(false);
                console.set_vexpand(true);
                console.set_size_request(-1, -1);
            } else {
                editor_nb.set_visible(true);
                console.set_vexpand(false);
                console.set_size_request(-1, 220);
            }
        }
    };
    update_center();
    {
        let f = update_center.clone();
        editor_nb.connect_page_added(move |_, _, _| f());
    }
    {
        let f = update_center.clone();
        editor_nb.connect_page_removed(move |_, _, _| f());
    }
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
    let view = crate::flowchart_view::build_flowchart_view(app, fc, None);
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
    let view = crate::flowchart_view::build_flowchart_view(app, fc, Some(path.to_path_buf()));
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

    // CONSOLE — color-coded transcript, matrix-retro green-on-black.
    let console_view = TextView::new();
    console_view.set_monospace(true);
    console_view.set_editable(false);
    console_view.add_css_class("mf-terminal");
    console_view.set_left_margin(6);
    console_view.set_top_margin(4);
    let cbuf = console_view.buffer();
    for (name, color) in console_tag_colors() {
        if cbuf.tag_table().lookup(name).is_none() {
            cbuf.create_tag(Some(name), &[("foreground", &color.to_css())]);
        }
    }
    let console_scroll = ScrolledWindow::new();
    console_scroll.set_child(Some(&console_view));
    nb.append_page(&console_scroll, Some(&Label::new(Some("CONSOLE"))));

    let render = {
        let app = app.clone();
        let buf = console_view.buffer();
        move || {
            buf.set_text("");
            let all = app.vm.console.messages.get().into_iter().chain(app.vm.repl.transcript.get());
            for m in all {
                let mut end = buf.end_iter();
                buf.insert_with_tags_by_name(&mut end, &format!("{}\n", m.text), &[level_tag(m.level)]);
            }
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

    // PROBLEMS — clickable diagnostics that jump to the source line.
    let problems = ListBox::new();
    let problems_scroll = ScrolledWindow::new();
    problems_scroll.set_child(Some(&problems));
    nb.append_page(&problems_scroll, Some(&Label::new(Some("PROBLEMS"))));
    {
        let app = app.clone();
        app.clone().vm.console.problems.bind(move |diags| {
            clear_list(&problems);
            for d in diags {
                let (icon, cls) = match d.level {
                    DiagnosticLevel::Error => ("✕", "mf-log-error"),
                    DiagnosticLevel::Warning => ("▲", "mf-log-warning"),
                    DiagnosticLevel::Note => ("ℹ", "mf-text-secondary"),
                };
                let file = std::path::Path::new(&d.file)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| d.file.clone());
                let btn = Button::with_label(&format!("{icon}  {file}:{}:{}   {}", d.line, d.column, d.message));
                btn.set_has_frame(false);
                btn.set_halign(gtk::Align::Start);
                btn.add_css_class("mf-row");
                btn.add_css_class(cls);
                let app = app.clone();
                let path = d.file.clone();
                let line = d.line;
                btn.connect_clicked(move |_| goto_problem(&app, &path, line));
                problems.append(&btn);
            }
        });
    }

    // Artifact panes (LLVM IR / C++ / …) appear after CONSOLE + PROBLEMS.
    {
        let nb_artifacts = nb.clone();
        app.vm.console.artifacts.subscribe(move |artifacts| {
            while nb_artifacts.n_pages() > 2 {
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
    input_row.add_css_class("mf-term-panel");
    let prompt = Label::new(Some(">>"));
    prompt.add_css_class("mf-prompt");
    let entry = Entry::new();
    entry.set_hexpand(true);
    entry.add_css_class("mf-terminal-entry");
    entry.set_placeholder_text(Some("MATLAB command…"));
    crate::e2e::set_repl_entry(&entry);
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

/// GtkTextTag name for a console message level.
fn level_tag(level: ConsoleLevel) -> &'static str {
    match level {
        ConsoleLevel::Error => "lvl-error",
        ConsoleLevel::Warning => "lvl-warning",
        ConsoleLevel::Success => "lvl-success",
        ConsoleLevel::Command => "lvl-command",
        ConsoleLevel::Debug => "lvl-debug",
        ConsoleLevel::Info => "lvl-info",
        ConsoleLevel::Plain => "lvl-plain",
    }
}

fn console_tag_colors() -> [(&'static str, matforge_core::theme::Rgb); 7] {
    use matforge_core::theme::Rgb;
    // Matrix-retro terminal palette: greens for normal output, red/yellow kept
    // for errors/warnings so they still pop on the near-black background.
    [
        ("lvl-error", Rgb::hex(0xFF5C57)),
        ("lvl-warning", Rgb::hex(0xF3F99D)),
        ("lvl-success", Rgb::hex(0x5AF78E)),
        ("lvl-command", Rgb::hex(0x7CFC8A)),
        ("lvl-debug", Rgb::hex(0x2F8F3F)),
        ("lvl-info", Rgb::hex(0x57C7B8)),
        ("lvl-plain", Rgb::hex(0x43D459)),
    ]
}

/// Open the diagnostic's file (if needed) and scroll to its line.
fn goto_problem(app: &Rc<AppState>, file: &str, line: usize) {
    let path = std::path::Path::new(file);
    open_file_in_editor(app, path);
    if let Some(id) = app
        .vm
        .editor
        .tabs
        .with(|ts| ts.iter().find(|t| t.url.as_deref() == Some(path)).map(|t| t.id))
    {
        app.vm.editor.request_goto(id, line);
    }
}

// ---- Right column (Workspace ⇄ Plots) -------------------------------------

fn build_right_column(app: &Rc<AppState>) -> Paned {
    // Workspace and Plots are separate, both-visible panels (like the macOS
    // reference) — a draggable divider between them, each independently
    // closable via its header ✕.
    let workspace = build_workspace(app);
    let plots = build_plots(app);

    let paned = Paned::new(Orientation::Horizontal);
    paned.set_wide_handle(true);
    paned.set_size_request(620, -1);
    paned.add_css_class("mf-border-left");
    paned.set_start_child(Some(&workspace));
    paned.set_end_child(Some(&plots));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);
    paned.set_position(320);

    {
        let workspace = workspace.clone();
        app.vm.layout.workspace_visible.bind(move |v| workspace.set_visible(*v));
    }
    {
        let plots = plots.clone();
        app.vm.layout.plots_visible.bind(move |v| plots.set_visible(*v));
    }
    // A new figure re-opens the Plots panel if it was closed.
    {
        let app = app.clone();
        app.clone().vm.plots.figures.subscribe(move |figs| {
            if !figs.is_empty() {
                app.vm.layout.plots_visible.set(true);
            }
        });
    }
    paned
}

fn build_workspace(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);

    // Header with refresh + close actions.
    let refresh = header_action(ic::REFRESH);
    {
        // A bare comment evaluates to nothing but still triggers the trailing
        // `whos` workspace-sync probe in ReplSession::send.
        let app = app.clone();
        refresh.connect_clicked(move |_| app.repl_send("% refresh"));
    }
    let close = header_action(ic::CLOSE);
    {
        let app = app.clone();
        close.connect_clicked(move |_| app.vm.layout.workspace_visible.set(false));
    }
    panel.append(&panel_header("WORKSPACE", &[refresh, close]));

    // Column header.
    panel.append(&ws_columns_header());

    // Table / empty-state stack.
    let table = ListBox::new();
    crate::e2e::set_workspace_table(&table);
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
                {
                    let app3 = app2.clone();
                    let name = v.name.clone();
                    // Left-click: select + capture the value into the Matrix Viewer.
                    btn.connect_clicked(move |_| app3.inspect_variable(&name));
                }
                attach_var_menu(&btn, &app2, &v.name); // right-click: Plot As…
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

    // When a value is captured, jump to the Matrix Viewer so it's visible.
    {
        let insp = insp.clone();
        app.vm.workspace.inspected_matrix.subscribe(move |m| {
            if m.is_some() {
                insp.set_current_page(Some(1));
            }
        });
    }

    panel
}

/// Attach a right-click "Plot As…" / Inspect menu to a workspace variable row.
fn attach_var_menu(btn: &Button, app: &Rc<AppState>, name: &str) {
    use matforge_core::models::PlotKind;
    let pop = gtk::Popover::new();
    pop.set_parent(btn);
    let menu = GtkBox::new(Orientation::Vertical, 1);

    let inspect = Button::with_label("Inspect value");
    inspect.set_has_frame(false);
    inspect.set_halign(gtk::Align::Start);
    {
        let app = app.clone();
        let name = name.to_string();
        let pop = pop.clone();
        inspect.connect_clicked(move |_| {
            app.inspect_variable(&name);
            pop.popdown();
        });
    }
    menu.append(&inspect);

    for (label, kind) in [
        ("Plot (Line)", PlotKind::Line2D),
        ("Plot Scatter", PlotKind::Scatter),
        ("Plot Bar", PlotKind::Bar),
        ("Plot Area", PlotKind::Spectrum),
    ] {
        let b = Button::with_label(label);
        b.set_has_frame(false);
        b.set_halign(gtk::Align::Start);
        let app = app.clone();
        let name = name.to_string();
        let pop = pop.clone();
        b.connect_clicked(move |_| {
            app.plot_variable_as(&name, kind);
            pop.popdown();
        });
        menu.append(&b);
    }
    pop.set_child(Some(&menu));

    let gesture = gtk::GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    {
        let pop = pop.clone();
        gesture.connect_pressed(move |_g, _n, x, y| {
            pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            pop.popup();
        });
    }
    btn.add_controller(gesture);
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
    crate::e2e::set_plots_add(&add);
    {
        let app = app.clone();
        add.connect_clicked(move |_| app.plot_inspected());
    }
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
    let close = header_action(ic::CLOSE);
    {
        let app = app.clone();
        close.connect_clicked(move |_| app.vm.layout.plots_visible.set(false));
    }
    panel.append(&panel_header("PLOTS", &[add, refresh, trash, clear, close]));

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
            match figure {
                Some(figure) => crate::plot_render::draw_figure(ctx, w as f64, h as f64, figure),
                None => crate::plot_render::draw_empty(ctx, w as f64, h as f64),
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
