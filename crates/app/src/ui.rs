//! Builds the main IDE window and wires every panel to `AppState`
//! (`MainViewModel` + live REPL/DAP sessions). Widgets subscribe to the view
//! models' `Property`s and call verb methods / `AppState` commands on input.

use std::cell::Cell;
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
    MAIN_WINDOW.with(|w| *w.borrow_mut() = Some(window.clone()));
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    root.append(&build_menu_bar(window, &app));
    root.append(&build_toolbar(window, &app));

    let middle = GtkBox::new(Orientation::Horizontal, 0);
    middle.set_vexpand(true);
    let activity = build_activity_bar(&app);
    middle.append(&activity);

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
    // Let both sides shrink past their natural size when the window is narrow, so
    // the center + right never overflow into each other (the old no-shrink pair
    // overlapped the console onto the workspace on smaller screens).
    inner.set_shrink_start_child(true);
    inner.set_shrink_end_child(true);
    // No fixed position: the editor expands, the right region keeps its size
    // request (workspace + plots).

    // sidebar | (center|right) — draggable divider, sidebar keeps its size.
    let outer = Paned::new(Orientation::Horizontal);
    outer.set_wide_handle(true);
    outer.set_hexpand(true);
    outer.set_start_child(Some(&sidebar));
    outer.set_end_child(Some(&inner));
    outer.set_resize_start_child(false);
    outer.set_resize_end_child(true);
    outer.set_shrink_start_child(false);
    // Restore the persisted sidebar width (the workspace|plots split is restored
    // inside build_right_column); both are saved back on exit.
    let sidebar_width = matforge_core::services::preferences::Preferences::load().layout.sidebar_width;
    outer.set_position(sidebar_width.clamp(160, 600));
    middle.append(&outer);
    LAYOUT_PANES.with(|p| *p.borrow_mut() = Some((outer.clone(), right.clone())));

    // Panel visibility, zen-aware: the activity bar / sidebar / right region all
    // follow their own flags unless Focus (zen) mode suppresses them.
    let update_chrome = {
        let app = app.clone();
        let activity = activity.clone();
        let sidebar = sidebar.clone();
        let right = right.clone();
        move || {
            let l = &app.vm.layout;
            activity.set_visible(l.chrome_visible());
            sidebar.set_visible(l.sidebar_effective());
            right.set_visible(l.right_effective());
        }
    };
    update_chrome();
    for prop in [
        app.vm.layout.sidebar_visible.clone(),
        app.vm.layout.workspace_visible.clone(),
        app.vm.layout.plots_visible.clone(),
    ] {
        let f = update_chrome.clone();
        prop.subscribe(move |_| f());
    }
    {
        let f = update_chrome.clone();
        app.vm.layout.zen.subscribe(move |_| f());
    }

    root.append(&middle);
    root.append(&build_status_bar(&app));

    // Transient toast feedback floats over the content (bottom-center).
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&root));
    let toast = Label::new(None);
    toast.add_css_class("mf-toast");
    toast.set_halign(gtk::Align::Center);
    toast.set_valign(gtk::Align::End);
    toast.set_visible(false);
    overlay.add_overlay(&toast);
    window.set_child(Some(&overlay));
    {
        let app = app.clone();
        let toast = toast.clone();
        let hide: Rc<std::cell::RefCell<Option<gtk::glib::SourceId>>> =
            Rc::new(std::cell::RefCell::new(None));
        let revision = app.vm.toast.revision.clone();
        revision.subscribe(move |_| {
            let Some(msg) = app.vm.toast.message.get() else { return };
            toast.set_text(&msg);
            toast.set_visible(true);
            if let Some(id) = hide.borrow_mut().take() {
                id.remove();
            }
            let toast2 = toast.clone();
            let hide2 = hide.clone();
            let id = gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(2200), move || {
                toast2.set_visible(false);
                *hide2.borrow_mut() = None;
            });
            *hide.borrow_mut() = Some(id);
        });
    }

    // Ctrl + scroll zooms the UI (capture phase so it beats scrollable children).
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let app = app.clone();
        scroll.connect_scroll(move |c, _dx, dy| {
            if c.current_event_state().contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                if dy < 0.0 {
                    app.vm.appearance.zoom_in();
                } else if dy > 0.0 {
                    app.vm.appearance.zoom_out();
                }
                glib_stop()
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(scroll);
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
    register("new", Rc::new(move || new_document_dialog(&a)));
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
        register("toggle-zen", Rc::new(move || a.vm.layout.toggle_zen()));
    }
    {
        let a = app.clone();
        register("compile", Rc::new(move || runner::compile(&a.vm)));
    }
    {
        let a = app.clone();
        register("run", Rc::new(move || run_active(&a)));
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
    {
        let a = app.clone();
        let w2 = w.clone();
        register("preferences", Rc::new(move || crate::settings_view::open(&a, Some(&w2))));
    }
    {
        let a = app.clone();
        register("zoom-in", Rc::new(move || a.vm.appearance.zoom_in()));
    }
    {
        let a = app.clone();
        register("zoom-out", Rc::new(move || a.vm.appearance.zoom_out()));
    }
    {
        let a = app.clone();
        register("zoom-reset", Rc::new(move || a.vm.appearance.zoom_reset()));
    }
    {
        let a = app.clone();
        let w2 = w.clone();
        register("command-palette", Rc::new(move || crate::palette::open_command_palette(&a, &w2)));
    }
    {
        let a = app.clone();
        let w2 = w.clone();
        register("quick-open", Rc::new(move || crate::palette::open_quick_open(&a, &w2)));
    }
    register("find-in-editor", Rc::new(show_find_bar));
    {
        let a = app.clone();
        let w2 = w.clone();
        register("goto-line", Rc::new(move || goto_line_prompt(&a, &w2)));
    }

    // Edit ▸ Undo/Redo/Cut/Copy/Paste/Select All. These target widget-scoped GTK
    // actions (`text.*`, `clipboard.*`, `selection.*`) that only exist on the
    // focused editable — so pointing the menu straight at them leaves the items
    // greyed whenever focus isn't in a text widget (and the menu popover itself
    // steals focus while open). Instead we track the text widget that last held
    // focus and forward through always-enabled `win.*` actions, so the items stay
    // live and act on the editor / console input / search box as expected.
    let last_text_focus: Rc<std::cell::RefCell<Option<gtk::Widget>>> =
        Rc::new(std::cell::RefCell::new(None));
    {
        let last = last_text_focus.clone();
        window.connect_focus_widget_notify(move |win| {
            if let Some(f) = gtk::prelude::GtkWindowExt::focus(win) {
                if f.is::<gtk::Text>() || f.is::<gtk::TextView>() {
                    *last.borrow_mut() = Some(f);
                }
            }
        });
    }
    for (name, target) in [
        ("undo", "text.undo"),
        ("redo", "text.redo"),
        ("cut", "clipboard.cut"),
        ("copy", "clipboard.copy"),
        ("paste", "clipboard.paste"),
        ("select-all", "selection.select-all"),
    ] {
        let last = last_text_focus.clone();
        let target = target.to_string();
        register(
            name,
            Rc::new(move || {
                if let Some(widget) = last.borrow().clone() {
                    let _ = widget.activate_action(&target, None);
                }
            }),
        );
    }

    // Keyboard accelerators (shown automatically in the menu by GTK).
    if let Some(gapp) = window.application().and_then(|a| a.downcast::<gtk::Application>().ok()) {
        for (action, accels) in [
            ("win.new", &["<Ctrl>n"][..]),
            ("win.open", &["<Ctrl>o"]),
            ("win.save", &["<Ctrl>s"]),
            ("win.close-tab", &["<Ctrl>w"]),
            ("win.quit", &["<Ctrl>q"]),
            ("win.find", &["<Ctrl><Shift>f"]),
            ("win.find-in-editor", &["<Ctrl>f"]),
            ("win.goto-line", &["<Ctrl>g"]),
            ("win.toggle-sidebar", &["<Ctrl>b"]),
            ("win.toggle-workspace", &["<Ctrl><Shift>w"]),
            ("win.command-palette", &["<Ctrl><Shift>p"]),
            ("win.quick-open", &["<Ctrl>p"]),
            ("win.toggle-zen", &["F11"]),
            ("win.compile", &["<Ctrl><Shift>b"]),
            ("win.run", &["<Ctrl>r"]),
            ("win.stop", &["<Shift>F5"]),
            ("win.debug", &["F5"]),
            ("win.dbg-continue", &["F8"]),
            ("win.dbg-next", &["F10"]),
            ("win.dbg-step-in", &["F11"]),
            ("win.dbg-step-out", &["<Shift>F11"]),
            ("win.preferences", &["<Ctrl>comma"]),
            ("win.zoom-in", &["<Ctrl>equal", "<Ctrl>plus", "<Ctrl>KP_Add"]),
            ("win.zoom-out", &["<Ctrl>minus", "<Ctrl>KP_Subtract"]),
            ("win.zoom-reset", &["<Ctrl>0"]),
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
    edit.append(Some("Undo"), Some("win.undo"));
    edit.append(Some("Redo"), Some("win.redo"));
    let clip = Menu::new();
    clip.append(Some("Cut"), Some("win.cut"));
    clip.append(Some("Copy"), Some("win.copy"));
    clip.append(Some("Paste"), Some("win.paste"));
    clip.append(Some("Select All"), Some("win.select-all"));
    edit.append_section(None, &clip);
    let find_section = Menu::new();
    find_section.append(Some("Find in File"), Some("win.find-in-editor"));
    find_section.append(Some("Go to Line…"), Some("win.goto-line"));
    find_section.append(Some("Search in Files"), Some("win.find"));
    edit.append_section(None, &find_section);
    let go_section = Menu::new();
    go_section.append(Some("Command Palette…"), Some("win.command-palette"));
    go_section.append(Some("Quick Open File…"), Some("win.quick-open"));
    edit.append_section(None, &go_section);
    let prefs_section = Menu::new();
    prefs_section.append(Some("Preferences…"), Some("win.preferences"));
    edit.append_section(None, &prefs_section);
    menubar.append_submenu(Some("Edit"), &edit);

    let view = Menu::new();
    let zoom = Menu::new();
    zoom.append(Some("Zoom In"), Some("win.zoom-in"));
    zoom.append(Some("Zoom Out"), Some("win.zoom-out"));
    zoom.append(Some("Reset Zoom"), Some("win.zoom-reset"));
    view.append_section(None, &zoom);
    let panels = Menu::new();
    panels.append(Some("Toggle Sidebar"), Some("win.toggle-sidebar"));
    panels.append(Some("Toggle Workspace"), Some("win.toggle-workspace"));
    panels.append(Some("Toggle Plots"), Some("win.toggle-plots"));
    panels.append(Some("Focus Mode"), Some("win.toggle-zen"));
    view.append_section(None, &panels);
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
        new_btn.connect_clicked(move |_| new_document_dialog(&app));
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
        run_btn.connect_clicked(move |_| run_active(&app));
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
    stack.add_named(&build_run_panel(app), Some("run"));
    stack.add_named(&build_compiler_panel(app), Some("compiler"));
    stack.add_named(&build_hdl_panel(app), Some("hdl"));
    stack.add_named(&build_docs_panel(app), Some("docs"));
    stack.add_named(&build_debug_panel(app), Some("debug"));

    let stack2 = stack.clone();
    app.vm.activity_bar.selected.bind(move |item| {
        let name = match item {
            ActivityItem::Debug => "debug",
            ActivityItem::Search => "search",
            ActivityItem::Run => "run",
            ActivityItem::Compiler => "compiler",
            ActivityItem::Hdl => "hdl",
            ActivityItem::Docs => "docs",
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

// ---- Run panel -------------------------------------------------------------

fn build_run_panel(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);

    // Header with an idle / RUN / DEBUG badge.
    let header = GtkBox::new(Orientation::Horizontal, 2);
    header.add_css_class("mf-panel-header-row");
    header.set_margin_start(8);
    header.set_margin_end(8);
    header.set_margin_top(5);
    header.set_margin_bottom(3);
    let title = Label::new(Some("RUN"));
    title.add_css_class("mf-panel-header");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    header.append(&title);
    let badge = Label::new(Some("IDLE"));
    badge.add_css_class("mf-build-badge");
    header.append(&badge);
    panel.append(&header);
    {
        let appc = app.clone();
        let badge = badge.clone();
        let update = move || {
            let (text, class) = if appc.vm.toolbar.is_debugging.get() {
                ("DEBUG", "mf-badge-fail")
            } else if appc.vm.toolbar.is_running.get() {
                ("RUN", "mf-badge-ok")
            } else {
                ("IDLE", "mf-badge-idle")
            };
            badge.set_text(text);
            for c in ["mf-badge-ok", "mf-badge-fail", "mf-badge-idle"] {
                badge.remove_css_class(c);
            }
            badge.add_css_class(class);
        };
        update();
        let u1 = update.clone();
        app.vm.toolbar.is_running.subscribe(move |_| u1());
        app.vm.toolbar.is_debugging.subscribe(move |_| update());
    }

    let body = GtkBox::new(Orientation::Vertical, 0);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&body));
    panel.append(&scroll);

    // PROGRAM — active file + its folder.
    body.append(&sub_header("PROGRAM"));
    let program = Label::new(Some("No active file"));
    program.add_css_class("mf-text-secondary");
    program.set_halign(gtk::Align::Start);
    program.set_margin_start(8);
    program.set_wrap(true);
    body.append(&program);
    {
        let appc = app.clone();
        let program = program.clone();
        let update = move |_: &Option<u64>| match appc.vm.editor.active_tab() {
            Some(t) => match t.url {
                Some(url) => {
                    let dir = url.parent().map(|p| p.display().to_string()).unwrap_or_default();
                    program.set_text(&format!("{}\n{}", t.name, dir));
                }
                None => program.set_text(&format!("{} — unsaved (Save to run / debug)", t.name)),
            },
            None => program.set_text("No active file"),
        };
        update(&None);
        app.vm.editor.active_id.subscribe(update);
    }

    // ACTIONS — Run / Debug / Stop.
    body.append(&sub_header("ACTIONS"));
    let run = Button::with_label("▶  Run");
    run.add_css_class("mf-compile-cta");
    run.set_margin_start(8);
    run.set_margin_end(8);
    run.set_margin_top(2);
    {
        let app = app.clone();
        run.connect_clicked(move |_| {
            let settings = app.settings.clone();
            runner::run(app.vm.clone(), &settings);
        });
    }
    body.append(&run);
    let debug = Button::with_label("🐞  Debug");
    debug.add_css_class("mf-compile-cta");
    debug.set_margin_start(8);
    debug.set_margin_end(8);
    debug.set_margin_top(4);
    {
        let app = app.clone();
        debug.connect_clicked(move |_| app.start_debug());
    }
    body.append(&debug);
    let stop = Button::with_label("■  Stop");
    stop.add_css_class("mf-tool");
    stop.add_css_class("mf-stop");
    stop.set_margin_start(8);
    stop.set_margin_end(8);
    stop.set_margin_top(4);
    {
        let app = app.clone();
        stop.connect_clicked(move |_| {
            app.stop_debug();
            app.vm.toolbar.is_running.set(false);
        });
    }
    body.append(&stop);

    // BINARIES — the resolved toolchain paths with existence checks.
    body.append(&sub_header("BINARIES"));
    body.append(&binary_row("matlabc", &app.settings.matlabc_path));
    body.append(&binary_row("libMatlabRuntime.a", &app.settings.runtime_archive));
    let note = Label::new(Some("Set $MATLABC_PATH or ~/.config/matforge/config.toml to override."));
    note.add_css_class("mf-text-muted");
    note.set_halign(gtk::Align::Start);
    note.set_wrap(true);
    note.set_margin_start(8);
    note.set_margin_top(4);
    note.set_margin_bottom(8);
    body.append(&note);

    panel
}

/// A BINARIES row: name, a ✓/✗ existence mark, and the path.
fn binary_row(name: &str, path: &Path) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 0);
    row.set_margin_start(8);
    row.set_margin_top(2);
    let exists = path.exists();
    let head = Label::new(Some(&format!("{} {name}", if exists { "✓" } else { "✗" })));
    head.set_halign(gtk::Align::Start);
    head.add_css_class(if exists { "mf-badge-ok" } else { "mf-badge-fail" });
    let p = Label::new(Some(&path.display().to_string()));
    p.add_css_class("mf-text-muted");
    p.add_css_class("mf-mono");
    p.set_halign(gtk::Align::Start);
    p.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    p.set_max_width_chars(28);
    row.append(&head);
    row.append(&p);
    row
}

// ---- HDL panel -------------------------------------------------------------

fn build_hdl_panel(app: &Rc<AppState>) -> GtkBox {
    use matforge_core::models::{CompilerTarget, ConsoleTab};

    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.append(&panel_header("HDL", &[]));
    let body = GtkBox::new(Orientation::Vertical, 0);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&body));
    panel.append(&scroll);

    // SOURCE.
    body.append(&sub_header("SOURCE"));
    let source = Label::new(Some("No active file"));
    source.add_css_class("mf-text-secondary");
    source.set_halign(gtk::Align::Start);
    source.set_margin_start(8);
    source.set_wrap(true);
    body.append(&source);
    {
        let appc = app.clone();
        let source = source.clone();
        let update = move |_: &Option<u64>| match appc.vm.editor.active_tab() {
            Some(t) if t.url.is_some() => source.set_text(&t.name),
            Some(t) => source.set_text(&format!("{} — unsaved (Save to compile)", t.name)),
            None => source.set_text("No active file"),
        };
        update(&None);
        app.vm.editor.active_id.subscribe(update);
    }

    // ACTIONS — the two HDL lanes.
    body.append(&sub_header("ACTIONS"));
    let sv = Button::with_label("Compile to SystemVerilog");
    sv.add_css_class("mf-compile-cta");
    sv.set_margin_start(8);
    sv.set_margin_end(8);
    sv.set_margin_top(2);
    {
        let app = app.clone();
        sv.connect_clicked(move |_| {
            app.vm.toolbar.set_target(CompilerTarget::Sv);
            runner::compile(&app.vm);
        });
    }
    body.append(&sv);
    let sv_note = Label::new(Some("Emits to the SYSTEMVERILOG artifact tab."));
    sv_note.add_css_class("mf-text-muted");
    sv_note.set_halign(gtk::Align::Start);
    sv_note.set_wrap(true);
    sv_note.set_margin_start(8);
    body.append(&sv_note);

    let va = Button::with_label("Compile to Verilog-A");
    va.add_css_class("mf-compile-cta");
    va.set_margin_start(8);
    va.set_margin_end(8);
    va.set_margin_top(6);
    {
        let app = app.clone();
        va.connect_clicked(move |_| {
            app.vm.toolbar.set_target(CompilerTarget::Va);
            let settings = app.settings.clone();
            runner::run(app.vm.clone(), &settings);
        });
    }
    body.append(&va);
    let va_note = Label::new(Some("Runs the script; writeVerilogA(...) calls emit .va files."));
    va_note.add_css_class("mf-text-muted");
    va_note.set_halign(gtk::Align::Start);
    va_note.set_wrap(true);
    va_note.set_margin_start(8);
    body.append(&va_note);

    // LATEST OUTPUT — whether each lane has produced an artifact.
    body.append(&sub_header("LATEST OUTPUT"));
    let sv_state = Label::new(None);
    sv_state.set_halign(gtk::Align::Start);
    sv_state.set_margin_start(8);
    let va_state = Label::new(None);
    va_state.set_halign(gtk::Align::Start);
    va_state.set_margin_start(8);
    body.append(&sv_state);
    body.append(&va_state);
    {
        let app = app.clone();
        let sv_state = sv_state.clone();
        let va_state = va_state.clone();
        app.vm.console.artifacts.bind(move |arts| {
            let mark = |present: bool, name: &str| {
                if present {
                    format!("✓ {name} emitted")
                } else {
                    format!("— no {name} yet")
                }
            };
            sv_state.set_text(&mark(arts.contains_key(&ConsoleTab::SystemVerilog), "SystemVerilog"));
            va_state.set_text(&mark(arts.contains_key(&ConsoleTab::VerilogA), "Verilog-A"));
        });
    }

    panel
}

// ---- Docs panel ------------------------------------------------------------

fn build_docs_panel(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.append(&panel_header("DOCS", &[]));

    let list = ListBox::new();
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    let app_sub = app.clone();
    app.vm.project.root_url.bind(move |root| {
        clear_list(&list);
        let Some(root) = root else {
            let l = row_label("Open a folder to browse its docs.");
            l.set_margin_top(8);
            list.append(&l);
            return;
        };
        let mut md = Vec::new();
        collect_markdown(root, 6, &mut md);
        // Prefer a top-level docs/ folder, then alphabetical.
        md.sort_by(|a, b| {
            let da = a.components().any(|c| c.as_os_str() == "docs");
            let db = b.components().any(|c| c.as_os_str() == "docs");
            db.cmp(&da).then(a.cmp(b))
        });
        if md.is_empty() {
            let l = row_label("No .md files under this folder.");
            l.set_margin_top(8);
            list.append(&l);
            return;
        }
        for path in md.iter().take(300) {
            let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned();
            let btn = Button::with_label(&rel);
            btn.set_has_frame(false);
            btn.set_halign(gtk::Align::Start);
            btn.add_css_class("mf-row");
            let app = app_sub.clone();
            let path = path.clone();
            btn.connect_clicked(move |_| open_file_in_editor(&app, &path));
            list.append(&btn);
        }
    });

    panel
}

/// Recursively collect `.md` files under `dir` (depth-limited, skips dot-dirs).
fn collect_markdown(dir: &Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if depth > 0 {
                collect_markdown(&path, depth - 1, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
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

// ---- Compiler panel --------------------------------------------------------

fn build_compiler_panel(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);

    // Header with a build-state badge.
    let header = GtkBox::new(Orientation::Horizontal, 2);
    header.add_css_class("mf-panel-header-row");
    header.set_margin_start(8);
    header.set_margin_end(8);
    header.set_margin_top(5);
    header.set_margin_bottom(3);
    let title = Label::new(Some("COMPILER"));
    title.add_css_class("mf-panel-header");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    header.append(&title);
    let badge = Label::new(Some("IDLE"));
    badge.add_css_class("mf-build-badge");
    header.append(&badge);
    panel.append(&header);
    {
        let appc = app.clone();
        let badge = badge.clone();
        let update = move || {
            let (text, class) = if appc.vm.toolbar.is_compiling.get() {
                ("BUILDING", "mf-badge-busy")
            } else {
                match appc.vm.toolbar.last_build.get() {
                    Some(true) => ("READY", "mf-badge-ok"),
                    Some(false) => ("FAILED", "mf-badge-fail"),
                    None => ("IDLE", "mf-badge-idle"),
                }
            };
            badge.set_text(text);
            for c in ["mf-badge-busy", "mf-badge-ok", "mf-badge-fail", "mf-badge-idle"] {
                badge.remove_css_class(c);
            }
            badge.add_css_class(class);
        };
        update();
        let u1 = update.clone();
        app.vm.toolbar.is_compiling.subscribe(move |_| u1());
        app.vm.toolbar.last_build.subscribe(move |_| update());
    }

    let body = GtkBox::new(Orientation::Vertical, 0);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&body));
    panel.append(&scroll);

    // SOURCE — the active file (and whether it can be compiled).
    body.append(&sub_header("SOURCE"));
    let source = Label::new(Some("No active file"));
    source.add_css_class("mf-text-secondary");
    source.set_halign(gtk::Align::Start);
    source.set_margin_start(8);
    source.set_wrap(true);
    body.append(&source);
    {
        let appc = app.clone();
        let source = source.clone();
        let update = move |_: &Option<u64>| {
            source.remove_css_class("mf-text-warn");
            match appc.vm.editor.active_tab() {
                Some(tab) if tab.url.is_some() => source.set_text(&tab.name),
                Some(tab) => {
                    source.set_text(&format!("{} — unsaved (Save to compile)", tab.name));
                    source.add_css_class("mf-text-warn");
                }
                None => source.set_text("No active file"),
            }
        };
        update(&None);
        app.vm.editor.active_id.subscribe(update);
    }

    // TARGET — language picker + the matlabc emit flag it maps to.
    body.append(&sub_header("TARGET"));
    let target_dd = DropDown::from_strings(&CompilerTarget::ALL.iter().map(|t| t.label()).collect::<Vec<_>>());
    target_dd.set_margin_start(8);
    target_dd.set_margin_end(8);
    body.append(&target_dd);
    let flag = Label::new(None);
    flag.add_css_class("mf-text-muted");
    flag.add_css_class("mf-mono");
    flag.set_halign(gtk::Align::Start);
    flag.set_margin_start(8);
    flag.set_margin_top(2);
    body.append(&flag);
    {
        let app = app.clone();
        target_dd.connect_selected_notify(move |dd| {
            app.vm.toolbar.set_target(CompilerTarget::ALL[dd.selected() as usize]);
        });
    }
    {
        let dd = target_dd.clone();
        let flag = flag.clone();
        app.vm.toolbar.target.bind(move |t| {
            if let Some(i) = CompilerTarget::ALL.iter().position(|x| x == t) {
                dd.set_selected(i as u32);
            }
            flag.set_text(t.matlabc_flag().unwrap_or("(runs program, captures .va)"));
        });
    }

    // OPTIONS — optimization + numeric mode (same state as the toolbar).
    body.append(&sub_header("OPTIONS"));
    let opt_dd = DropDown::from_strings(&OptimizationProfile::ALL.iter().map(|o| o.label()).collect::<Vec<_>>());
    opt_dd.set_margin_start(8);
    opt_dd.set_margin_end(8);
    opt_dd.set_margin_top(2);
    body.append(&opt_dd);
    {
        let app = app.clone();
        opt_dd.connect_selected_notify(move |dd| {
            app.vm.toolbar.set_optimization(OptimizationProfile::ALL[dd.selected() as usize]);
        });
    }
    {
        let dd = opt_dd.clone();
        app.vm.toolbar.optimization.bind(move |o| {
            if let Some(i) = OptimizationProfile::ALL.iter().position(|x| x == o) {
                dd.set_selected(i as u32);
            }
        });
    }
    let num_dd = DropDown::from_strings(&NumericMode::ALL.iter().map(|n| n.label()).collect::<Vec<_>>());
    num_dd.set_margin_start(8);
    num_dd.set_margin_end(8);
    num_dd.set_margin_top(4);
    body.append(&num_dd);
    {
        let app = app.clone();
        num_dd.connect_selected_notify(move |dd| {
            app.vm.toolbar.set_numeric_mode(NumericMode::ALL[dd.selected() as usize]);
        });
    }
    {
        let dd = num_dd.clone();
        app.vm.toolbar.numeric_mode.bind(move |n| {
            if let Some(i) = NumericMode::ALL.iter().position(|x| x == n) {
                dd.set_selected(i as u32);
            }
        });
    }

    // ACTIONS — Compile (enabled only for a saved file).
    body.append(&sub_header("ACTIONS"));
    let compile = Button::with_label("Compile");
    compile.add_css_class("mf-compile-cta");
    compile.set_margin_start(8);
    compile.set_margin_end(8);
    compile.set_margin_top(2);
    compile.set_margin_bottom(8);
    {
        let app = app.clone();
        compile.connect_clicked(move |_| runner::compile(&app.vm));
    }
    {
        let appc = app.clone();
        let compile = compile.clone();
        let update = move || {
            let can = appc.vm.editor.active_tab().and_then(|t| t.url).is_some()
                && !appc.vm.toolbar.is_compiling.get();
            compile.set_sensitive(can);
        };
        update();
        let u1 = update.clone();
        app.vm.editor.active_id.subscribe(move |_| u1());
        app.vm.toolbar.is_compiling.subscribe(move |_| update());
    }
    body.append(&compile);

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
    static MAIN_WINDOW: std::cell::RefCell<Option<ApplicationWindow>> = const { std::cell::RefCell::new(None) };
    static FIND_BAR: std::cell::RefCell<Option<(gtk::Revealer, Entry)>> = const { std::cell::RefCell::new(None) };
    /// The right-panel BLOCK INSPECTOR host: `(notebook, page index, body host)`.
    static FLOW_INSPECTOR: std::cell::RefCell<Option<(Notebook, u32, GtkBox)>> = const { std::cell::RefCell::new(None) };
    /// The flowchart whose tab is currently visible — the Run/Debug target.
    static ACTIVE_FLOWCHART: std::cell::RefCell<Option<(Rc<FlowchartViewModel>, Option<std::path::PathBuf>)>> = const { std::cell::RefCell::new(None) };
    /// The persisted-position dividers: `(sidebar|content, workspace|plots)`.
    static LAYOUT_PANES: std::cell::RefCell<Option<(Paned, Paned)>> = const { std::cell::RefCell::new(None) };
}

/// Current divider positions `(sidebar_width, workspace_split)` for persistence,
/// or `None` before the window is built.
pub fn layout_pane_positions() -> Option<(i32, i32)> {
    LAYOUT_PANES.with(|p| p.borrow().as_ref().map(|(outer, right)| (outer.position(), right.position())))
}

/// Mark `fc` as the visible flowchart (its tab just mapped). Used by Run.
pub fn set_active_flowchart(fc: &Rc<FlowchartViewModel>, path: Option<std::path::PathBuf>) {
    ACTIVE_FLOWCHART.with(|a| *a.borrow_mut() = Some((fc.clone(), path)));
}

/// Clear the active flowchart if it is still `fc` (its tab just unmapped).
pub fn clear_active_flowchart(fc: &Rc<FlowchartViewModel>) {
    ACTIVE_FLOWCHART.with(|a| {
        let mut a = a.borrow_mut();
        if a.as_ref().is_some_and(|(cur, _)| Rc::ptr_eq(cur, fc)) {
            *a = None;
        }
    });
}

/// The visible flowchart and its `.mflow` path, if a flowchart tab is showing.
fn active_flowchart() -> Option<(Rc<FlowchartViewModel>, Option<std::path::PathBuf>)> {
    ACTIVE_FLOWCHART.with(|a| a.borrow().clone())
}

/// Run the active editor target: a visible flowchart compiles to MATLAB and runs
/// the generated `.m`; otherwise the active `.m` tab runs directly.
fn run_active(app: &Rc<AppState>) {
    if let Some((fc, path)) = active_flowchart() {
        if crate::flowchart_view::emit_matlab(app, &fc, path.as_deref()).is_some() {
            runner::run(app.vm.clone(), &app.settings);
        }
    } else {
        runner::run(app.vm.clone(), &app.settings);
    }
}

/// Reveal the in-editor find bar and focus its entry.
fn show_find_bar() {
    FIND_BAR.with(|f| {
        if let Some((rev, entry)) = f.borrow().as_ref() {
            rev.set_reveal_child(true);
            entry.grab_focus();
            entry.select_region(0, -1);
        }
    });
}

/// A small "Go to line" prompt; jumps the active editor tab on Enter.
fn goto_line_prompt(app: &Rc<AppState>, parent: &ApplicationWindow) {
    let Some(tab) = app.vm.editor.active_tab() else { return };
    let win = gtk::Window::builder()
        .transient_for(parent)
        .modal(true)
        .decorated(false)
        .default_width(240)
        .build();
    win.add_css_class("mf-root");
    let bar = GtkBox::new(Orientation::Vertical, 0);
    bar.add_css_class("mf-window");
    bar.add_css_class("mf-palette");
    let entry = Entry::new();
    entry.set_placeholder_text(Some("Go to line…"));
    entry.set_margin_top(8);
    entry.set_margin_bottom(8);
    entry.set_margin_start(8);
    entry.set_margin_end(8);
    bar.append(&entry);
    win.set_child(Some(&bar));
    {
        let app = app.clone();
        let win = win.clone();
        let tab_id = tab.id;
        entry.connect_activate(move |e| {
            if let Ok(line) = e.text().trim().parse::<usize>() {
                app.vm.editor.request_goto(tab_id, line.max(1));
            }
            win.close();
        });
    }
    let keys = gtk::EventControllerKey::new();
    {
        let win = win.clone();
        keys.connect_key_pressed(move |_c, key, _code, _state| {
            if key == gtk::gdk::Key::Escape {
                win.close();
                glib_stop()
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    entry.add_controller(keys);
    win.present();
    entry.grab_focus();
}

/// The in-editor find bar: searches the focused editor's buffer with next / prev
/// + a match count, slid in above the editor by Ctrl+F.
fn build_find_bar() -> gtk::Revealer {
    let rev = gtk::Revealer::new();
    rev.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    let bar = GtkBox::new(Orientation::Horizontal, 6);
    bar.add_css_class("mf-chrome");
    bar.add_css_class("mf-border-bottom");
    bar.set_margin_start(8);
    bar.set_margin_end(8);
    bar.set_margin_top(4);
    bar.set_margin_bottom(4);

    let entry = Entry::new();
    entry.set_placeholder_text(Some("Find in file"));
    entry.set_hexpand(true);
    let count = Label::new(None);
    count.add_css_class("mf-text-muted");
    count.set_width_chars(11);
    let prev = Button::from_icon_name("go-up-symbolic");
    prev.add_css_class("mf-header-action");
    let next = Button::from_icon_name("go-down-symbolic");
    next.add_css_class("mf-header-action");
    let close = Button::from_icon_name(ic::CLOSE);
    close.add_css_class("mf-header-action");
    bar.append(&entry);
    bar.append(&count);
    bar.append(&prev);
    bar.append(&next);
    bar.append(&close);
    rev.set_child(Some(&bar));

    let search = {
        let count = count.clone();
        move |query: &str, forward: bool, reset: bool| {
            let Some(view) = editor_view::active_view() else { return };
            let buffer = view.buffer();
            if query.is_empty() {
                count.set_text("");
                return;
            }
            let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY;
            let (sel_start, sel_end) = buffer
                .selection_bounds()
                .unwrap_or_else(|| (buffer.start_iter(), buffer.start_iter()));
            let found = if reset {
                buffer.start_iter().forward_search(query, flags, None)
            } else if forward {
                sel_end
                    .forward_search(query, flags, None)
                    .or_else(|| buffer.start_iter().forward_search(query, flags, None))
            } else {
                sel_start
                    .backward_search(query, flags, None)
                    .or_else(|| buffer.end_iter().backward_search(query, flags, None))
            };
            if let Some((mut s, e)) = found {
                buffer.select_range(&s, &e);
                view.scroll_to_iter(&mut s, 0.15, false, 0.0, 0.0);
            }
            let mut n = 0;
            let mut it = buffer.start_iter();
            while let Some((_, e)) = it.forward_search(query, flags, None) {
                n += 1;
                it = e;
            }
            count.set_text(&match n {
                0 => "no results".to_string(),
                1 => "1 match".to_string(),
                _ => format!("{n} matches"),
            });
        }
    };

    {
        let search = search.clone();
        entry.connect_changed(move |e| search(&e.text(), true, true));
    }
    {
        let search = search.clone();
        entry.connect_activate(move |e| search(&e.text(), true, false));
    }
    {
        let search = search.clone();
        let entry = entry.clone();
        next.connect_clicked(move |_| search(&entry.text(), true, false));
    }
    {
        let search = search.clone();
        let entry = entry.clone();
        prev.connect_clicked(move |_| search(&entry.text(), false, false));
    }
    let hide = {
        let rev = rev.clone();
        move || {
            rev.set_reveal_child(false);
            if let Some(v) = editor_view::active_view() {
                v.grab_focus();
            }
        }
    };
    {
        let hide = hide.clone();
        close.connect_clicked(move |_| hide());
    }
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_c, key, _code, _state| {
        if key == gtk::gdk::Key::Escape {
            hide();
            glib_stop()
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    entry.add_controller(keys);

    FIND_BAR.with(|f| *f.borrow_mut() = Some((rev.clone(), entry)));
    rev
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

    center.append(&build_find_bar());

    let editor_nb = Notebook::new();
    editor_nb.set_vexpand(true);
    editor_nb.add_css_class("mf-editor");
    EDITOR_NB.with(|e| *e.borrow_mut() = Some(editor_nb.clone()));

    let welcome = build_welcome(app);
    let console = build_console(app);
    center.append(&editor_nb);
    center.append(&welcome);
    center.append(&console);

    // Three center states, driven by what's open:
    //   • a source/flowchart tab open → editor + a docked console;
    //   • a folder open but no tab → the console (MATLAB command window) fills;
    //   • nothing open → the Welcome start screen.
    let update_center = {
        let app = app.clone();
        let editor_nb = editor_nb.clone();
        let welcome = welcome.clone();
        let console = console.clone();
        move || {
            let pages = editor_nb.n_pages();
            let has_folder = app.vm.project.root_url.get().is_some();
            if pages > 0 {
                editor_nb.set_visible(true);
                welcome.set_visible(false);
                console.set_vexpand(false);
                console.set_size_request(-1, 220);
            } else if has_folder {
                editor_nb.set_visible(false);
                welcome.set_visible(false);
                console.set_vexpand(true);
                console.set_size_request(-1, -1);
            } else {
                editor_nb.set_visible(false);
                welcome.set_visible(true);
                console.set_vexpand(false);
                console.set_size_request(-1, 180);
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
    {
        let f = update_center.clone();
        app.vm.project.root_url.subscribe(move |_| f());
    }
    center
}

/// The start screen shown when nothing is open: quick actions, recent folders,
/// and a few examples to get a researcher productive immediately.
fn build_welcome(app: &Rc<AppState>) -> GtkBox {
    use matforge_core::services::preferences::Preferences;

    let outer = GtkBox::new(Orientation::Vertical, 0);
    outer.set_vexpand(true);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    let card = GtkBox::new(Orientation::Vertical, 10);
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    card.set_vexpand(true);
    card.set_margin_top(40);
    card.set_margin_bottom(40);
    card.set_size_request(440, -1);
    scroll.set_child(Some(&card));
    outer.append(&scroll);

    let logo = Label::new(Some("M"));
    logo.add_css_class("mf-logo");
    logo.set_halign(gtk::Align::Center);
    card.append(&logo);
    let title = Label::new(Some("MatForge IDE"));
    title.add_css_class("mf-empty-title");
    card.append(&title);
    let tag = Label::new(Some("A home for MATLAB-LLVM data science and engineering."));
    tag.add_css_class("mf-text-muted");
    card.append(&tag);

    // Quick actions.
    let actions = GtkBox::new(Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::Center);
    actions.set_margin_top(8);
    let new_btn = Button::with_label("New File");
    new_btn.add_css_class("mf-compile-cta");
    {
        let app = app.clone();
        new_btn.connect_clicked(move |_| new_document_dialog(&app));
    }
    let open_btn = Button::with_label("Open Folder…");
    open_btn.add_css_class("mf-compile-cta");
    {
        let app = app.clone();
        open_btn.connect_clicked(move |_| {
            if let Some(win) = MAIN_WINDOW.with(|w| w.borrow().clone()) {
                pick_folder(&win, &app);
            }
        });
    }
    actions.append(&new_btn);
    actions.append(&open_btn);
    card.append(&actions);

    // Recent folders.
    let recent = Preferences::load().recent;
    if !recent.is_empty() {
        card.append(&welcome_header("RECENT"));
        for entry in recent.iter().take(6) {
            let name = std::path::Path::new(entry)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.clone());
            let btn = welcome_link(&format!("📁  {name}"), entry);
            let app = app.clone();
            let path = entry.clone();
            btn.connect_clicked(move |_| {
                let _ = app.vm.open_folder(std::path::Path::new(&path));
            });
            card.append(&btn);
        }
    }

    // Examples.
    card.append(&welcome_header("EXAMPLES"));
    let ex_script = welcome_link("📄  New MATLAB script", "untitled.m");
    {
        let app = app.clone();
        ex_script.connect_clicked(move |_| new_untitled(&app));
    }
    card.append(&ex_script);
    let ex_signal = welcome_link("🔀  Signal-flow model", "mflowLink demo");
    {
        let app = app.clone();
        ex_signal.connect_clicked(move |_| open_demo_flowchart(&app, true));
    }
    card.append(&ex_signal);
    let ex_control = welcome_link("◇  Control-flow model", "flowchart demo");
    {
        let app = app.clone();
        ex_control.connect_clicked(move |_| open_demo_flowchart(&app, false));
    }
    card.append(&ex_control);

    outer
}

fn welcome_header(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.add_css_class("mf-panel-header");
    l.set_halign(gtk::Align::Start);
    l.set_margin_top(12);
    l
}

fn welcome_link(label: &str, tooltip: &str) -> Button {
    let b = Button::with_label(label);
    b.set_has_frame(false);
    b.add_css_class("mf-row");
    b.set_halign(gtk::Align::Start);
    b.set_tooltip_text(Some(tooltip));
    b
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

/// Open a blank flowchart of `kind` (mFlow / mFlowLink) in a new tab.
fn new_flowchart(app: &Rc<AppState>, kind: matforge_core::models::flowchart::SchemaKind) {
    let fc = Rc::new(FlowchartViewModel::empty("untitled", kind));
    fc.select(None);
    let view = crate::flowchart_view::build_flowchart_view(app, fc, None);
    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        if let Some(nb) = nb.as_ref() {
            let label = tab_label(&view, "untitled.mflow", app, None);
            let page = nb.append_page(&view, Some(&label));
            nb.set_current_page(Some(page));
        }
    });
    app.vm.activity_bar.select(ActivityItem::Flowchart);
}

/// A document-type chooser shown by the New action: MATLAB script, mFlow
/// (control-flow), or mFlowLink (signal-flow) — each creates a blank document.
fn new_document_dialog(app: &Rc<AppState>) {
    use matforge_core::models::flowchart::SchemaKind;

    let parent = MAIN_WINDOW.with(|w| w.borrow().clone());
    let mut builder = gtk::Window::builder()
        .modal(true)
        .decorated(false)
        .default_width(340);
    if let Some(p) = &parent {
        builder = builder.transient_for(p);
    }
    let win = builder.build();
    win.add_css_class("mf-root");

    let bar = GtkBox::new(Orientation::Vertical, 2);
    bar.add_css_class("mf-window");
    bar.add_css_class("mf-palette");
    bar.set_margin_top(6);
    bar.set_margin_bottom(8);

    let header = Label::new(Some("New File"));
    header.add_css_class("mf-panel-header");
    header.set_xalign(0.0);
    header.set_margin_start(12);
    header.set_margin_top(4);
    header.set_margin_bottom(4);
    bar.append(&header);

    let script = new_doc_option(ic::FILE, "MATLAB Script", "A blank .m script");
    let mflow = new_doc_option(ic::FLOWCHART, "mFlow Diagram", "Control-flow flowchart");
    let mflink = new_doc_option(ic::FLOWCHART, "mFlowLink Model", "Signal-flow model you can simulate");
    bar.append(&script);
    bar.append(&mflow);
    bar.append(&mflink);
    win.set_child(Some(&bar));

    {
        let app = app.clone();
        let win = win.clone();
        script.connect_clicked(move |_| {
            win.close();
            new_untitled(&app);
        });
    }
    {
        let app = app.clone();
        let win = win.clone();
        mflow.connect_clicked(move |_| {
            win.close();
            new_flowchart(&app, SchemaKind::ControlFlow);
        });
    }
    {
        let app = app.clone();
        let win = win.clone();
        mflink.connect_clicked(move |_| {
            win.close();
            new_flowchart(&app, SchemaKind::SignalFlow);
        });
    }

    let keys = gtk::EventControllerKey::new();
    {
        let win = win.clone();
        keys.connect_key_pressed(move |_c, key, _code, _state| {
            if key == gtk::gdk::Key::Escape {
                win.close();
                glib_stop()
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    win.add_controller(keys);
    win.present();
    script.grab_focus();
}

/// One option row (icon + title + subtitle) for the New File chooser.
fn new_doc_option(icon: &str, title: &str, subtitle: &str) -> Button {
    let btn = Button::new();
    btn.set_has_frame(false);
    btn.add_css_class("mf-row");
    btn.add_css_class("mf-newdoc-row");

    let row = GtkBox::new(Orientation::Horizontal, 10);
    let img = Image::from_icon_name(icon);
    img.set_pixel_size(22);
    img.add_css_class("mf-text-secondary");
    row.append(&img);

    let texts = GtkBox::new(Orientation::Vertical, 0);
    let t = Label::new(Some(title));
    t.set_xalign(0.0);
    t.add_css_class("mf-newdoc-title");
    let s = Label::new(Some(subtitle));
    s.set_xalign(0.0);
    s.add_css_class("mf-text-muted");
    texts.append(&t);
    texts.append(&s);
    row.append(&texts);

    btn.set_child(Some(&row));
    btn
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
    match tab.url {
        Some(url) => write_tab(app, tab.id, &url, &tab.contents),
        None => save_as_dialog(app, tab.id, &tab.name, &tab.contents),
    }
}

/// Prompt for a destination, then write + repoint the tab (Save As).
fn save_as_dialog(app: &Rc<AppState>, id: u64, suggested: &str, contents: &str) {
    let dialog = gtk::FileDialog::builder()
        .title("Save As")
        .initial_name(suggested)
        .build();
    let parent = MAIN_WINDOW.with(|w| w.borrow().clone());
    let app = app.clone();
    let contents = contents.to_string();
    dialog.save(parent.as_ref(), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                if std::fs::write(&path, &contents).is_ok() {
                    app.vm.editor.save_as(id, &path);
                    rename_tab_label(id, &path);
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    app.vm.status_bar.set_message(format!("Saved {}", path.display()));
                    app.vm.toast.show(format!("Saved {name}"));
                } else {
                    app.vm.console.log(ConsoleLevel::Error, format!("save failed: {}", path.display()));
                }
            }
        }
    });
}

fn write_tab(app: &Rc<AppState>, id: u64, url: &Path, contents: &str) {
    match std::fs::write(url, contents) {
        Ok(()) => {
            app.vm.editor.mark_saved(id);
            let name = url.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            app.vm.status_bar.set_message(format!("Saved {}", url.display()));
            app.vm.toast.show(format!("Saved {name}"));
        }
        Err(e) => app.vm.console.log(ConsoleLevel::Error, format!("save failed: {e}")),
    }
}

/// Update the visible notebook tab label after a Save As (icon + name).
fn rename_tab_label(id: u64, path: &Path) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    EDITOR_NB.with(|nb| {
        let nb = nb.borrow();
        let Some(nb) = nb.as_ref() else { return };
        let Some(p) = nb.current_page() else { return };
        let Some(page) = nb.nth_page(Some(p)) else { return };
        let Some(label_box) = nb.tab_label(&page).and_then(|w| w.downcast::<GtkBox>().ok()) else {
            return;
        };
        let mut child = label_box.first_child();
        while let Some(w) = child {
            if let Some(img) = w.downcast_ref::<Image>() {
                img.set_icon_name(Some(tab_icon(&name)));
            } else if let Some(lbl) = w.downcast_ref::<Label>() {
                lbl.set_text(&name);
            }
            child = w.next_sibling();
        }
    });
    let _ = id;
}

// ---- Console + live REPL ---------------------------------------------------

fn build_console(app: &Rc<AppState>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-top");
    panel.set_size_request(-1, 220);

    let nb = Notebook::new();
    nb.set_vexpand(true);

    // CONSOLE — an inline MATLAB Command Window: one editable TextView that is
    // both the scrollback transcript and the live `>>` command line. Output
    // streams in *above* a prompt pinned to the bottom; everything before the
    // current input is frozen (read-only) scrollback. Matrix-retro green-on-black.
    let console_view = TextView::new();
    console_view.set_monospace(true);
    console_view.set_editable(true);
    console_view.set_cursor_visible(true);
    console_view.set_wrap_mode(gtk::WrapMode::WordChar);
    console_view.add_css_class("mf-terminal");
    console_view.set_left_margin(6);
    console_view.set_top_margin(4);
    console_view.set_bottom_margin(4);
    let cbuf = console_view.buffer();
    refresh_console_tags(&cbuf, &app.vm.appearance.tokens());
    {
        // Recolor the level tags when the theme changes (e.g. dark → light).
        let cbuf = cbuf.clone();
        let app = app.clone();
        app.clone().vm.appearance.revision.subscribe(move |_| {
            refresh_console_tags(&cbuf, &app.vm.appearance.tokens());
        });
    }
    let console_scroll = ScrolledWindow::new();
    console_scroll.set_child(Some(&console_view));
    nb.append_page(&console_scroll, Some(&Label::new(Some("CONSOLE"))));

    // Terminal state. `programmatic` marks our own edits so the read-only guard
    // ignores them. `prompt_start` sits at the start of the final prompt line
    // (output is inserted here, pushing the prompt down); `input_start` sits just
    // after `>> ` — text from there to the buffer end is the editable command.
    let programmatic = Rc::new(Cell::new(true));
    let mut e = cbuf.end_iter();
    cbuf.insert_with_tags_by_name(&mut e, ">> ", &["lvl-command"]);
    let prompt_start = cbuf.create_mark(Some("mf_prompt_start"), &cbuf.iter_at_offset(0), false);
    let input_start = cbuf.create_mark(Some("mf_input_start"), &cbuf.end_iter(), true);
    programmatic.set(false);

    // Read-only guard: veto user edits that fall before the input boundary.
    {
        let prog = programmatic.clone();
        let input_start = input_start.clone();
        cbuf.connect_insert_text(move |buf, iter, _text| {
            if !prog.get() && iter.offset() < buf.iter_at_mark(&input_start).offset() {
                buf.stop_signal_emission_by_name("insert-text");
            }
        });
    }
    {
        let prog = programmatic.clone();
        let input_start = input_start.clone();
        cbuf.connect_delete_range(move |buf, start, _end| {
            if !prog.get() && start.offset() < buf.iter_at_mark(&input_start).offset() {
                buf.stop_signal_emission_by_name("delete-range");
            }
        });
    }

    // Append-only renderer: draw just the new console/REPL items above the prompt.
    // A shrink in either stream (a `clear()`) re-seeds the terminal to one prompt.
    let msg_count = Rc::new(Cell::new(0usize));
    let tr_count = Rc::new(Cell::new(0usize));
    let append = {
        let app = app.clone();
        let cbuf = cbuf.clone();
        let view = console_view.clone();
        let prog = programmatic.clone();
        let prompt_start = prompt_start.clone();
        let input_start = input_start.clone();
        let msg_count = msg_count.clone();
        let tr_count = tr_count.clone();
        Rc::new(move || {
            let msgs = app.vm.console.messages.get();
            let tr = app.vm.repl.transcript.get();
            prog.set(true);
            if msgs.len() < msg_count.get() || tr.len() < tr_count.get() {
                cbuf.set_text("");
                let mut e = cbuf.end_iter();
                cbuf.insert_with_tags_by_name(&mut e, ">> ", &["lvl-command"]);
                cbuf.move_mark(&prompt_start, &cbuf.iter_at_offset(0));
                cbuf.move_mark(&input_start, &cbuf.end_iter());
                msg_count.set(0);
                tr_count.set(0);
            }
            for m in msgs.iter().skip(msg_count.get()) {
                console_insert_line(&cbuf, &prompt_start, m.level, &m.text);
            }
            msg_count.set(msgs.len());
            for m in tr.iter().skip(tr_count.get()) {
                // The `>> cmd` echo is already typed into the buffer; don't redraw it.
                if m.level == ConsoleLevel::Command {
                    continue;
                }
                console_insert_line(&cbuf, &prompt_start, m.level, &m.text);
            }
            tr_count.set(tr.len());
            prog.set(false);
            let mut end = cbuf.end_iter();
            view.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
        })
    };
    append();
    {
        let append = append.clone();
        app.vm.console.messages.subscribe(move |_| append());
    }
    {
        let append = append.clone();
        app.vm.repl.transcript.subscribe(move |_| append());
    }

    // Enter runs the command; ↑/↓ recall history; typing in frozen scrollback
    // jumps the caret back to the prompt.
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let app = app.clone();
        let cbuf = cbuf.clone();
        let view = console_view.clone();
        let prog = programmatic.clone();
        let prompt_start = prompt_start.clone();
        let input_start = input_start.clone();
        key.connect_key_pressed(move |_c, keyval, _code, state| {
            use gtk::gdk::Key;
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let alt = state.contains(gtk::gdk::ModifierType::ALT_MASK);
            let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if !ctrl && !alt && keyval.to_unicode().is_some() {
                let guard = cbuf.iter_at_mark(&input_start).offset();
                if cbuf.cursor_position() < guard {
                    cbuf.place_cursor(&cbuf.end_iter());
                }
            }
            match keyval {
                // Ctrl+L clears the console (MATLAB's clc); Ctrl+C interrupts a
                // running command — but only with no selection, so Ctrl+C still
                // copies selected text (terminal convention).
                Key::l | Key::L if ctrl => {
                    clear_console(&app);
                    glib_stop()
                }
                Key::c | Key::C if ctrl && cbuf.selection_bounds().is_none() => {
                    app.repl_interrupt();
                    glib_stop()
                }
                Key::Return | Key::KP_Enter if !shift => {
                    let start = cbuf.iter_at_mark(&input_start);
                    let cmd = cbuf.text(&start, &cbuf.end_iter(), false).to_string();
                    // `clc` clears the command window instead of running.
                    if cmd.trim() == "clc" {
                        app.vm.repl.input.set(String::new());
                        clear_console(&app);
                        return glib_stop();
                    }
                    // Freeze the typed line and open a fresh prompt beneath it.
                    prog.set(true);
                    let mut e = cbuf.end_iter();
                    cbuf.insert(&mut e, "\n");
                    let line_start = cbuf.end_iter().offset();
                    let mut e = cbuf.end_iter();
                    cbuf.insert_with_tags_by_name(&mut e, ">> ", &["lvl-command"]);
                    cbuf.move_mark(&prompt_start, &cbuf.iter_at_offset(line_start));
                    cbuf.move_mark(&input_start, &cbuf.end_iter());
                    cbuf.place_cursor(&cbuf.end_iter());
                    prog.set(false);
                    let mut end = cbuf.end_iter();
                    view.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
                    app.vm.repl.input.set(cmd);
                    if let Some(command) = app.vm.repl.submit() {
                        app.repl_send(&command);
                    }
                    glib_stop()
                }
                Key::Up => {
                    app.vm.repl.recall_previous();
                    replace_input(&cbuf, &prog, &input_start, &app.vm.repl.input.get());
                    glib_stop()
                }
                Key::Down => {
                    app.vm.repl.recall_next();
                    replace_input(&cbuf, &prog, &input_start, &app.vm.repl.input.get());
                    glib_stop()
                }
                Key::Tab => {
                    tab_complete(&app, &cbuf, &input_start);
                    glib_stop()
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
    }
    console_view.add_controller(key);

    // Click a underlined file:line reference to jump to that source line.
    {
        let click = gtk::GestureClick::new();
        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        let view = console_view.clone();
        let app = app.clone();
        click.connect_released(move |_g, _n, x, y| {
            let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            let Some(iter) = view.iter_at_location(bx, by) else { return };
            let buf = view.buffer();
            let Some(tag) = buf.tag_table().lookup("link") else { return };
            if !iter.has_tag(&tag) {
                return;
            }
            // Re-parse the clicked line for the reference and jump to it.
            let mut ls = buf.iter_at_line(iter.line()).unwrap_or_else(|| iter.clone());
            let mut le = ls.clone();
            le.forward_to_line_end();
            let line_text = buf.text(&ls, &le, false).to_string();
            ls.set_line_offset(0);
            if let Some((_, _, file, line)) = find_source_link(&line_text) {
                goto_problem(&app, &file, line);
            }
        });
        console_view.add_controller(click);
    }
    crate::e2e::set_repl_entry(&console_view);

    // PROBLEMS — clickable diagnostics that jump to the source line.
    let problems = ListBox::new();
    let problems_scroll = ScrolledWindow::new();
    problems_scroll.set_child(Some(&problems));
    // A friendly empty state, centered over the (empty) list.
    let problems_empty = empty_state(
        "emblem-ok-symbolic",
        "No problems detected",
        "Errors and warnings from compile and run show up here.",
    );
    problems_empty.set_can_target(false);
    let problems_overlay = gtk::Overlay::new();
    problems_overlay.set_child(Some(&problems_scroll));
    problems_overlay.add_overlay(&problems_empty);
    nb.append_page(&problems_overlay, Some(&Label::new(Some("PROBLEMS"))));
    {
        let app = app.clone();
        let problems_empty = problems_empty.clone();
        app.clone().vm.console.problems.bind(move |diags| {
            clear_list(&problems);
            problems_empty.set_visible(diags.is_empty());
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
                let buf = view.buffer();
                buf.set_text(text);
                // Syntax-highlight generated code (C++/Python/TS/LLVM/MLIR/Verilog).
                if let Some(lang) = artifact_language(*tab) {
                    crate::highlight::apply(&buf, lang);
                }
                let scroll = ScrolledWindow::new();
                scroll.set_child(Some(&view));
                nb_artifacts.append_page(&scroll, Some(&Label::new(Some(tab.label()))));
            }
        });
    }

    // Copy / Clear actions in the tab bar's end corner; they act on the tab in
    // view (console transcript, problems list, or a generated-code pane).
    {
        let actions = GtkBox::new(Orientation::Horizontal, 2);
        actions.add_css_class("mf-tab-actions");
        let copy_btn = header_action(ic::COPY);
        copy_btn.set_tooltip_text(Some("Copy contents"));
        let clear_btn = header_action(ic::CLEAR);
        clear_btn.set_tooltip_text(Some("Clear"));
        actions.append(&copy_btn);
        actions.append(&clear_btn);
        nb.set_action_widget(&actions, gtk::PackType::End);

        {
            let app = app.clone();
            let nb = nb.clone();
            let cbuf = console_view.buffer();
            copy_btn.connect_clicked(move |btn| {
                let text = current_tab_text(&nb, &app, &cbuf);
                if !text.is_empty() {
                    btn.clipboard().set_text(&text);
                    app.vm.toast.show("Copied to clipboard");
                }
            });
        }
        {
            let app = app.clone();
            let nb = nb.clone();
            clear_btn.connect_clicked(move |_| clear_current_tab(&nb, &app));
        }
    }

    // Search-in-output box in the tab bar's start corner. It searches the tab in
    // view (console transcript or a generated-code pane), selecting matches;
    // Enter / ↓ go to the next match, ↑ to the previous, with a live count.
    {
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search output…"));
        search.set_width_chars(14);
        let count = Label::new(None);
        count.add_css_class("mf-text-muted");
        count.set_width_chars(11);
        let sbox = GtkBox::new(Orientation::Horizontal, 4);
        sbox.add_css_class("mf-tab-search");
        sbox.append(&search);
        sbox.append(&count);
        nb.set_action_widget(&sbox, gtk::PackType::Start);

        let run = {
            let nb = nb.clone();
            let count = count.clone();
            move |query: &str, forward: bool, reset: bool| match current_tab_view(&nb) {
                Some(view) => count.set_text(&search_count_text(query, output_search(&view, query, forward, reset))),
                None => count.set_text(""),
            }
        };
        {
            let run = run.clone();
            search.connect_search_changed(move |e| run(&e.text(), true, true));
        }
        {
            let run = run.clone();
            search.connect_activate(move |e| run(&e.text(), true, false));
        }
        {
            let run = run.clone();
            search.connect_next_match(move |e| run(&e.text(), true, false));
        }
        {
            let run = run.clone();
            search.connect_previous_match(move |e| run(&e.text(), false, false));
        }
        // Follow the active tab: re-run once the switched-to page is current.
        {
            let run = run.clone();
            let search = search.clone();
            nb.connect_switch_page(move |_nb, _page, _idx| {
                let q = search.text().to_string();
                let run = run.clone();
                gtk::glib::idle_add_local_once(move || run(&q, true, true));
            });
        }
    }
    panel.append(&nb);
    panel
}

/// Replace the editable command (from `input_start` to the buffer end) with
/// `text`, used by ↑/↓ history recall. Marked programmatic so the read-only
/// guard does not veto the edit.
fn replace_input(cbuf: &gtk::TextBuffer, prog: &Rc<Cell<bool>>, input_start: &gtk::TextMark, text: &str) {
    prog.set(true);
    let mut start = cbuf.iter_at_mark(input_start);
    let mut end = cbuf.end_iter();
    cbuf.delete(&mut start, &mut end);
    let mut at = cbuf.iter_at_mark(input_start);
    cbuf.insert(&mut at, text);
    cbuf.place_cursor(&cbuf.end_iter());
    prog.set(false);
}

fn glib_stop() -> gtk::glib::Propagation {
    gtk::glib::Propagation::Stop
}

/// The highlighter language for a generated-code console tab, if it is code.
fn artifact_language(tab: matforge_core::models::ConsoleTab) -> Option<Language> {
    use matforge_core::models::ConsoleTab as T;
    Some(match tab {
        T::Cpp => Language::Cpp,
        T::Python => Language::Python,
        T::TypeScript => Language::TypeScript,
        T::LlvmIr => Language::LlvmIr,
        T::Mlir => Language::Mlir,
        T::SystemVerilog | T::VerilogA => Language::Verilog,
        T::Console | T::Problems => return None,
    })
}

/// Text content of the bottom panel's currently-visible tab, for Copy.
fn current_tab_text(nb: &Notebook, app: &Rc<AppState>, console_buf: &gtk::TextBuffer) -> String {
    match nb.current_page() {
        Some(0) => {
            let (s, e) = console_buf.bounds();
            console_buf.text(&s, &e, false).to_string()
        }
        Some(1) => app
            .vm
            .console
            .problems
            .get()
            .iter()
            .map(|d| format!("{}:{}:{}: {}", d.file, d.line, d.column, d.message))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(idx) => nb
            .nth_page(Some(idx))
            .and_then(|p| p.downcast::<ScrolledWindow>().ok())
            .and_then(|s| s.child())
            .and_then(|c| c.downcast::<TextView>().ok())
            .map(|tv| {
                let b = tv.buffer();
                let (s, e) = b.bounds();
                b.text(&s, &e, false).to_string()
            })
            .unwrap_or_default(),
        None => String::new(),
    }
}

/// The `TextView` of the bottom panel's current tab (console or a code pane),
/// or `None` for the non-text PROBLEMS tab.
fn current_tab_view(nb: &Notebook) -> Option<TextView> {
    let idx = nb.current_page()?;
    nb.nth_page(Some(idx))
        .and_then(|p| p.downcast::<ScrolledWindow>().ok())
        .and_then(|s| s.child())
        .and_then(|c| c.downcast::<TextView>().ok())
}

/// Select the next/previous (or first, when `reset`) case-insensitive match of
/// `query` in `view` and scroll to it; returns the total number of matches.
fn output_search(view: &TextView, query: &str, forward: bool, reset: bool) -> usize {
    let buffer = view.buffer();
    if query.is_empty() {
        return 0;
    }
    let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY;
    let (sel_start, sel_end) = buffer
        .selection_bounds()
        .unwrap_or_else(|| (buffer.start_iter(), buffer.start_iter()));
    let found = if reset {
        buffer.start_iter().forward_search(query, flags, None)
    } else if forward {
        sel_end
            .forward_search(query, flags, None)
            .or_else(|| buffer.start_iter().forward_search(query, flags, None))
    } else {
        sel_start
            .backward_search(query, flags, None)
            .or_else(|| buffer.end_iter().backward_search(query, flags, None))
    };
    if let Some((mut s, e)) = found {
        buffer.select_range(&s, &e);
        view.scroll_to_iter(&mut s, 0.15, false, 0.0, 0.0);
    }
    let mut n = 0;
    let mut it = buffer.start_iter();
    while let Some((_, e)) = it.forward_search(query, flags, None) {
        n += 1;
        it = e;
    }
    n
}

/// Match-count label text for the output search box.
fn search_count_text(query: &str, n: usize) -> String {
    if query.is_empty() {
        String::new()
    } else {
        match n {
            0 => "no results".to_string(),
            1 => "1 match".to_string(),
            _ => format!("{n} matches"),
        }
    }
}

/// Clear the bottom panel's currently-visible tab.
/// Clear the console transcript (general log + REPL transcript). The inline
/// terminal's renderer re-seeds itself to a single `>>` prompt on this shrink.
fn clear_console(app: &Rc<AppState>) {
    app.vm.console.clear();
    app.vm.repl.transcript.update(|t| t.clear());
}

fn clear_current_tab(nb: &Notebook, app: &Rc<AppState>) {
    match nb.current_page() {
        Some(0) => clear_console(app),
        Some(1) => app.vm.console.problems.update(|p| p.clear()),
        Some(idx) => {
            // Drop the generated artifact backing this page (idx 2.. ↔ map keys).
            let keys: Vec<_> = app.vm.console.artifacts.get().keys().copied().collect();
            if let Some(&tab) = keys.get((idx as usize).saturating_sub(2)) {
                app.vm.console.artifacts.update(|a| {
                    a.remove(&tab);
                });
            }
        }
        None => {}
    }
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

/// Per-level console text colors for the active theme. Dark themes keep the
/// signature matrix-retro greens (bright inks that pop on near-black); light
/// themes derive readable dark inks from the theme so the console reads like a
/// classic white MATLAB Command Window.
fn console_tag_colors(t: &matforge_core::theme::ThemeTokens) -> [(&'static str, matforge_core::theme::Rgb); 7] {
    use matforge_core::theme::Rgb;
    if t.dark {
        [
            ("lvl-error", Rgb::hex(0xFF5C57)),
            ("lvl-warning", Rgb::hex(0xF3F99D)),
            ("lvl-success", Rgb::hex(0x5AF78E)),
            ("lvl-command", Rgb::hex(0x7CFC8A)),
            ("lvl-debug", Rgb::hex(0x2F8F3F)),
            ("lvl-info", Rgb::hex(0x57C7B8)),
            ("lvl-plain", Rgb::hex(0x43D459)),
        ]
    } else {
        [
            ("lvl-error", t.red),
            ("lvl-warning", t.yellow),
            ("lvl-success", t.green),
            ("lvl-command", t.term_fg),
            ("lvl-debug", t.text_secondary),
            ("lvl-info", t.cyan),
            ("lvl-plain", t.term_fg),
        ]
    }
}

/// Create or recolor the console's per-level text tags for `tokens`. Called once
/// at build and again whenever the appearance (theme) changes. Reads the tokens
/// straight from the appearance VM rather than the cached render so it doesn't
/// race the CSS re-render on a theme switch.
fn refresh_console_tags(buf: &gtk::TextBuffer, tokens: &matforge_core::theme::ThemeTokens) {
    let table = buf.tag_table();
    for (name, color) in console_tag_colors(tokens) {
        match table.lookup(name) {
            Some(tag) => tag.set_property("foreground", color.to_css()),
            None => {
                buf.create_tag(Some(name), &[("foreground", &color.to_css())]);
            }
        }
    }
    // The clickable file:line link tag — underlined, in the theme's link blue.
    let link = tokens.blue.to_css();
    match table.lookup("link") {
        Some(tag) => tag.set_property("foreground", link),
        None => {
            buf.create_tag(
                Some("link"),
                &[("foreground", &link), ("underline", &gtk::pango::Underline::Single)],
            );
        }
    }
}

/// Insert one console line above the prompt, tagged by level, and underline any
/// `path:line[:col]` source reference (compiler/run errors) as a clickable link.
fn console_insert_line(
    buf: &gtk::TextBuffer,
    prompt_start: &gtk::TextMark,
    level: ConsoleLevel,
    text: &str,
) {
    let start = buf.iter_at_mark(prompt_start).offset();
    let mut it = buf.iter_at_mark(prompt_start);
    buf.insert_with_tags_by_name(&mut it, &format!("{text}\n"), &[level_tag(level)]);
    if let Some((bs, be, _, _)) = find_source_link(text) {
        let cs = start + text[..bs].chars().count() as i32;
        let ce = start + text[..be].chars().count() as i32;
        buf.apply_tag_by_name("link", &buf.iter_at_offset(cs), &buf.iter_at_offset(ce));
    }
}

/// Common MATLAB functions/keywords offered by Tab-completion, alongside live
/// workspace variables and command history.
const MATLAB_BUILTINS: &[&str] = &[
    "abs", "acos", "all", "angle", "any", "asin", "atan", "atan2", "axis", "bar", "cat", "ceil",
    "cell", "clc", "clear", "close", "cos", "cumsum", "det", "diag", "diff", "disp", "dot", "eig",
    "else", "elseif", "end", "error", "exp", "eye", "fft", "figure", "find", "fix", "fliplr",
    "flipud", "floor", "for", "fprintf", "function", "grid", "hold", "if", "imag", "inv",
    "isempty", "isnan", "kron", "legend", "length", "linspace", "load", "log", "log10", "magic",
    "max", "mean", "median", "min", "mod", "norm", "numel", "ones", "plot", "plot3", "prod",
    "rand", "randn", "real", "repmat", "reshape", "return", "round", "save", "scatter", "sign",
    "sin", "size", "sort", "sprintf", "sqrt", "subplot", "sum", "surf", "tan", "title",
    "transpose", "trace", "while", "who", "whos", "xlabel", "ylabel", "zeros", "zlabel",
];

/// Tab-complete the identifier left of the caret in the console input. Completes
/// to the single match, or to the longest common prefix of several (listing them
/// in the console when the prefix can't grow further).
fn tab_complete(app: &Rc<AppState>, cbuf: &gtk::TextBuffer, input_start: &gtk::TextMark) {
    let is_off = cbuf.iter_at_mark(input_start).offset();
    let caret = cbuf.cursor_position();
    if caret < is_off {
        return;
    }
    let before = cbuf.text(&cbuf.iter_at_offset(is_off), &cbuf.iter_at_offset(caret), false).to_string();
    let word_start = before.rfind(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(0, |i| i + 1);
    let word = &before[word_start..];
    if word.is_empty() {
        return;
    }
    let cands = completion_candidates(app, word);
    let Some(common) = longest_common_prefix(&cands) else { return };
    if common.len() > word.len() {
        cbuf.insert_at_cursor(&common[word.len()..]);
    } else if cands.len() > 1 {
        // Can't extend further — list the options like a shell does.
        app.vm.console.log(ConsoleLevel::Info, cands.join("    "));
    }
}

/// Completion candidates for `word` (case-insensitive prefix): live workspace
/// variable names, identifiers seen in command history, then MATLAB built-ins.
fn completion_candidates(app: &Rc<AppState>, word: &str) -> Vec<String> {
    let lw = word.to_lowercase();
    let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    app.vm.workspace.variables.with(|vs| {
        for v in vs {
            if v.name.to_lowercase().starts_with(&lw) {
                set.insert(v.name.clone());
            }
        }
    });
    app.vm.repl.history.with(|h| {
        for cmd in h {
            for tok in cmd.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if tok.len() > 1 && tok.to_lowercase().starts_with(&lw) {
                    set.insert(tok.to_string());
                }
            }
        }
    });
    for b in MATLAB_BUILTINS {
        if b.starts_with(&lw) {
            set.insert((*b).to_string());
        }
    }
    set.into_iter().collect()
}

/// The longest common prefix of `items`, or `None` if empty.
fn longest_common_prefix(items: &[String]) -> Option<String> {
    let first = items.first()?;
    let mut len = first.len();
    for s in &items[1..] {
        len = first
            .char_indices()
            .zip(s.char_indices())
            .take_while(|((_, a), (_, b))| a == b)
            .map(|((i, c), _)| i + c.len_utf8())
            .last()
            .unwrap_or(0)
            .min(len);
    }
    Some(first[..len].to_string())
}

/// Find the first `path:line[:col]` source reference in a console line (the form
/// clang/matlabc print for compile errors). Returns the byte range of the token
/// plus the file and 1-based line. The path must look file-like (contain `.` or
/// `/`) so plain ranges like `3:5` aren't mistaken for links.
fn find_source_link(line: &str) -> Option<(usize, usize, String, usize)> {
    let mut idx = 0;
    for tok in line.split_whitespace() {
        let start = idx + line[idx..].find(tok)?;
        idx = start + tok.len();
        let trimmed = tok.trim_end_matches([':', ',', ')', '(']);
        let mut parts = trimmed.split(':');
        let path = parts.next().unwrap_or("");
        if (path.contains('.') || path.contains('/')) && !path.is_empty() {
            if let Some(Ok(ln)) = parts.next().map(str::parse::<usize>) {
                return Some((start, start + trimmed.len(), path.to_string(), ln));
            }
        }
    }
    None
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

/// Export the selected figure to a PNG: a runtime figure writes its captured
/// bytes verbatim; a series figure is re-rendered to an offscreen surface.
fn export_selected_figure(app: &Rc<AppState>) {
    use matforge_core::models::PlotFigure;
    let Some(id) = app.vm.plots.selected_id.get() else {
        app.vm.status_bar.set_message("No figure selected to export");
        return;
    };
    let Some(figure) = app.vm.plots.figures.with(|f| f.iter().find(|fig| fig.id == id).cloned())
    else {
        return;
    };

    let render_png = |fig: &PlotFigure| -> Option<Vec<u8>> {
        if let Some(png) = &fig.png_data {
            return Some(png.clone());
        }
        // Re-render the series chart to an ARGB32 surface, then PNG-encode it via
        // GDK (cairo's own `png` feature isn't enabled in this gtk4 build).
        let (w, h) = (900, 600);
        let mut surface =
            gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h).ok()?;
        {
            let ctx = gtk::cairo::Context::new(&surface).ok()?;
            crate::plot_render::draw_figure(&ctx, w as f64, h as f64, fig, None, None, None);
        }
        let stride = surface.stride() as usize;
        surface.flush();
        let data = surface.data().ok()?;
        let bytes = gtk::glib::Bytes::from(&data[..]);
        let texture = gtk::gdk::MemoryTexture::new(
            w,
            h,
            gtk::gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            stride,
        );
        Some(texture.save_to_png_bytes().to_vec())
    };
    let Some(png) = render_png(&figure) else {
        app.vm.console.log(ConsoleLevel::Error, "could not render figure to PNG");
        return;
    };

    let suggested = format!(
        "{}.png",
        figure.title.split(['·', ' ']).next().unwrap_or("figure").trim().replace(' ', "_")
    );
    let dialog = gtk::FileDialog::builder().title("Export Figure").initial_name(suggested).build();
    let parent = MAIN_WINDOW.with(|w| w.borrow().clone());
    let app = app.clone();
    dialog.save(parent.as_ref(), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                match std::fs::write(&path, &png) {
                    Ok(()) => {
                        app.vm.status_bar.set_message(format!("Exported {}", path.display()));
                        app.vm.toast.show("Figure exported");
                    }
                    Err(e) => app.vm.console.log(ConsoleLevel::Error, format!("export failed: {e}")),
                }
            }
        }
    });
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
    // A more compact right column so it doesn't dominate smaller screens. The
    // workspace needs ~250px to show its NAME/VALUE/TYPE/SIZE columns without
    // clipping, leaving the rest for the plots preview.
    paned.set_size_request(470, -1);
    paned.add_css_class("mf-border-left");
    paned.set_start_child(Some(&workspace));
    paned.set_end_child(Some(&plots));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);
    // Restore the persisted workspace|plots split (kept within the fixed 470px
    // right column so it can't starve the workspace and re-clip it).
    let split = matforge_core::services::preferences::Preferences::load().layout.workspace_split;
    paned.set_position(split.clamp(200, 360));

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

    // Filter box — narrow the table to variables whose name matches.
    let filter = gtk::SearchEntry::new();
    filter.set_placeholder_text(Some("Filter variables…"));
    filter.add_css_class("mf-ws-filter");
    filter.set_margin_start(6);
    filter.set_margin_end(6);
    filter.set_margin_top(4);
    filter.set_margin_bottom(2);
    panel.append(&filter);

    // Sortable column header: click a title to sort by it; click again to reverse.
    // `sort` is (column index, ascending).
    let sort = Rc::new(Cell::new((0usize, true)));
    let header = GtkBox::new(Orientation::Horizontal, 4);
    header.add_css_class("mf-col-header");
    let header_lbls: Rc<Vec<Label>> = Rc::new(
        WS_COLS
            .iter()
            .map(|c| {
                let l = Label::new(Some(c.title));
                l.add_css_class("mf-col-title");
                l.set_xalign(c.xalign);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                l.set_width_chars(c.min_chars);
                l.set_hexpand(c.expand);
                l.set_cursor_from_name(Some("pointer"));
                l
            })
            .collect(),
    );
    let update_arrows = {
        let header_lbls = header_lbls.clone();
        let sort = sort.clone();
        Rc::new(move || {
            let (col, asc) = sort.get();
            for (i, l) in header_lbls.iter().enumerate() {
                if i == col {
                    l.set_text(&format!("{} {}", WS_COLS[i].title, if asc { "▲" } else { "▼" }));
                } else {
                    l.set_text(WS_COLS[i].title);
                }
            }
        })
    };

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

    // Shared render: filter + sort the variables and rebuild the rows. Driven by
    // variable changes, the filter box, and header clicks.
    let render = {
        let app = app.clone();
        let table = table.clone();
        let body = body.clone();
        let filter = filter.clone();
        let sort = sort.clone();
        Rc::new(move || {
            let q = filter.text().to_lowercase();
            let (col, asc) = sort.get();
            let mut vars = app.vm.workspace.variables.get();
            if !q.is_empty() {
                vars.retain(|v| v.name.to_lowercase().contains(&q));
            }
            vars.sort_by(|a, b| ws_cmp(a, b, col));
            if !asc {
                vars.reverse();
            }
            clear_list(&table);
            for v in &vars {
                let btn = ws_variable_row(v);
                {
                    let app = app.clone();
                    let name = v.name.clone();
                    // Left-click: select + capture the value into the Matrix Viewer.
                    btn.connect_clicked(move |_| app.inspect_variable(&name));
                }
                attach_var_menu(&btn, &app, &v.name); // right-click: Plot/Save/Clear…
                table.append(&btn);
            }
            // Only the truly-empty workspace shows the hint; a no-match filter just
            // leaves the table empty.
            let none = app.vm.workspace.variables.with(|vs| vs.is_empty());
            body.set_visible_child_name(if none { "empty" } else { "table" });
        })
    };

    // Wire header clicks (sort), filter changes, and variable updates to render.
    for (i, l) in header_lbls.iter().enumerate() {
        let gesture = gtk::GestureClick::new();
        let sort = sort.clone();
        let render = render.clone();
        let update_arrows = update_arrows.clone();
        gesture.connect_released(move |_, _, _, _| {
            let (col, asc) = sort.get();
            sort.set(if col == i { (i, !asc) } else { (i, true) });
            update_arrows();
            render();
        });
        l.add_controller(gesture);
        header.append(l);
    }
    {
        let render = render.clone();
        filter.connect_search_changed(move |_| render());
    }
    {
        let render = render.clone();
        app.vm.workspace.variables.subscribe(move |_| render());
    }
    update_arrows();
    render();

    panel.append(&header);
    panel.append(&body);

    // Inspector tabs.
    let insp = Notebook::new();
    insp.set_size_request(-1, 180);
    // Without this, the four tab labels (VARIABLE/MATRIX/TABLE/BLOCK) force the
    // notebook's minimum width to their combined size (~260px), which exceeds the
    // workspace's column slot and clips the whole panel. Scrolling tabs lets it
    // shrink to the available width instead.
    insp.set_scrollable(true);
    insp.append_page(&build_variable_inspector(app), Some(&Label::new(Some("VARIABLE"))));
    insp.append_page(&build_matrix_viewer(app), Some(&Label::new(Some("MATRIX"))));
    insp.append_page(&build_table_viewer(app), Some(&Label::new(Some("TABLE"))));

    // BLOCK INSPECTOR — host for the active flowchart's property editor. The
    // flowchart view installs its inspector here when shown (so the diagram tab
    // keeps the full width) and removes it when hidden.
    let flow_host = GtkBox::new(Orientation::Vertical, 0);
    flow_host.set_vexpand(true);
    flow_host.append(&flow_inspector_placeholder());
    let flow_scroll = ScrolledWindow::new();
    flow_scroll.set_vexpand(true);
    flow_scroll.set_child(Some(&flow_host));
    let flow_page = insp.append_page(&flow_scroll, Some(&Label::new(Some("BLOCK"))));
    FLOW_INSPECTOR.with(|f| *f.borrow_mut() = Some((insp.clone(), flow_page, flow_host)));

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

/// The placeholder shown in the BLOCK INSPECTOR tab when no flowchart is active.
fn flow_inspector_placeholder() -> GtkBox {
    empty_state(
        ic::FLOWCHART,
        "No block selected",
        "Open a flowchart and select a block to edit its properties.",
    )
}

/// Install `content` as the BLOCK INSPECTOR tab's body (called when a flowchart
/// tab becomes visible).
pub fn flow_inspector_show(content: &gtk::Widget) {
    FLOW_INSPECTOR.with(|f| {
        if let Some((_, _, host)) = &*f.borrow() {
            while let Some(c) = host.first_child() {
                host.remove(&c);
            }
            host.append(content);
        }
    });
}

/// Remove `content` from the BLOCK INSPECTOR tab if it is the current body,
/// restoring the placeholder (called when a flowchart tab is hidden). Guarded so
/// a tab switch that maps the next flowchart first doesn't wipe its inspector.
pub fn flow_inspector_hide(content: &gtk::Widget) {
    FLOW_INSPECTOR.with(|f| {
        if let Some((_, _, host)) = &*f.borrow() {
            if host.first_child().as_ref() == Some(content) {
                host.remove(content);
                host.append(&flow_inspector_placeholder());
            }
        }
    });
}

/// Switch the right-side panel to the BLOCK INSPECTOR tab.
pub fn flow_inspector_focus() {
    FLOW_INSPECTOR.with(|f| {
        if let Some((nb, page, _)) = &*f.borrow() {
            nb.set_current_page(Some(*page));
        }
    });
}

/// The main application window, for parenting transient dialogs/players.
pub fn main_window() -> Option<ApplicationWindow> {
    MAIN_WINDOW.with(|w| w.borrow().clone())
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
        ("Plot Surface (3D)", PlotKind::Surface3D),
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

    // Data actions — Copy name / Rename / Save / Clear, each dispatched to the
    // REPL (or clipboard). MATLAB has no in-place rename, so it copies then
    // clears the old binding.
    menu.append(&gtk::Separator::new(Orientation::Horizontal));
    let item = |label: &str| {
        let b = Button::with_label(label);
        b.set_has_frame(false);
        b.set_halign(gtk::Align::Start);
        b
    };

    let copy = item("Copy name");
    {
        let name = name.to_string();
        let pop = pop.clone();
        copy.connect_clicked(move |b| {
            b.clipboard().set_text(&name);
            pop.popdown();
        });
    }
    menu.append(&copy);

    let rename = item("Rename…");
    {
        let app = app.clone();
        let old = name.to_string();
        let pop = pop.clone();
        rename.connect_clicked(move |_| {
            pop.popdown();
            let app = app.clone();
            let old = old.clone();
            let from = old.clone();
            text_prompt(&format!("Rename “{old}” to…"), &old, move |new| {
                let new = new.trim();
                if !new.is_empty() && new != from {
                    app.repl_send(&format!("{new} = {from}; clear {from}"));
                }
            });
        });
    }
    menu.append(&rename);

    let save = item("Save to .mat");
    {
        let app = app.clone();
        let name = name.to_string();
        let pop = pop.clone();
        save.connect_clicked(move |_| {
            app.repl_send(&format!("save('{name}.mat', '{name}')"));
            app.vm.toast.show(format!("Saved {name}.mat to the working folder"));
            pop.popdown();
        });
    }
    menu.append(&save);

    let clear = item("Clear variable");
    clear.add_css_class("mf-log-error");
    {
        let app = app.clone();
        let name = name.to_string();
        let pop = pop.clone();
        clear.connect_clicked(move |_| {
            app.repl_send(&format!("clear {name}"));
            pop.popdown();
        });
    }
    menu.append(&clear);

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

    // Drag the variable onto the Plots panel to chart it.
    let drag = gtk::DragSource::new();
    drag.set_actions(gtk::gdk::DragAction::COPY);
    {
        let name = name.to_string();
        drag.connect_prepare(move |_src, _x, _y| {
            Some(gtk::gdk::ContentProvider::for_value(&name.to_value()))
        });
    }
    btn.add_controller(drag);
}

/// A small modal text-entry dialog (Enter confirms, Esc cancels), parented to the
/// main window. Used for one-off inputs like renaming a workspace variable.
fn text_prompt(title: &str, initial: &str, on_ok: impl Fn(String) + 'static) {
    let win = gtk::Window::builder().modal(true).decorated(false).default_width(280).build();
    if let Some(p) = main_window() {
        win.set_transient_for(Some(&p));
    }
    win.add_css_class("mf-root");
    let bar = GtkBox::new(Orientation::Vertical, 0);
    bar.add_css_class("mf-window");
    bar.add_css_class("mf-palette");
    let lbl = Label::new(Some(title));
    lbl.set_halign(gtk::Align::Start);
    lbl.set_margin_top(8);
    lbl.set_margin_start(8);
    lbl.set_margin_end(8);
    let entry = Entry::new();
    entry.set_text(initial);
    entry.set_margin_top(6);
    entry.set_margin_bottom(8);
    entry.set_margin_start(8);
    entry.set_margin_end(8);
    bar.append(&lbl);
    bar.append(&entry);
    win.set_child(Some(&bar));
    {
        let win = win.clone();
        entry.connect_activate(move |e| {
            on_ok(e.text().to_string());
            win.close();
        });
    }
    let keys = gtk::EventControllerKey::new();
    {
        let win = win.clone();
        keys.connect_key_pressed(move |_c, key, _code, _state| {
            if key == gtk::gdk::Key::Escape {
                win.close();
                glib_stop()
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    entry.add_controller(keys);
    win.present();
    entry.grab_focus();
}

/// A workspace table column. `min_chars` is a floor (not a fixed width); only
/// NAME `expand`s, so the identifier — not the usually-empty value — absorbs the
/// panel's slack and the table stays reactive as the pane resizes. `dim` renders
/// the secondary columns in the muted style. Shared by the header and each row so
/// they stay aligned.
struct WsCol {
    title: &'static str,
    min_chars: i32,
    expand: bool,
    xalign: f32,
    dim: bool,
}

const WS_COLS: [WsCol; 4] = [
    WsCol { title: "NAME", min_chars: 4, expand: true, xalign: 0.0, dim: false },
    WsCol { title: "VALUE", min_chars: 6, expand: false, xalign: 0.0, dim: true },
    WsCol { title: "TYPE", min_chars: 7, expand: false, xalign: 0.0, dim: true },
    WsCol { title: "SIZE", min_chars: 7, expand: false, xalign: 1.0, dim: true },
];

/// Order two workspace variables by column: 0=name, 1=value preview, 2=type,
/// 3=size (by element count). Names/types compare case-insensitively.
fn ws_cmp(
    a: &matforge_core::models::WorkspaceVariable,
    b: &matforge_core::models::WorkspaceVariable,
    col: usize,
) -> std::cmp::Ordering {
    match col {
        1 => a.preview.cmp(&b.preview),
        2 => a.dtype.display_name().cmp(&b.dtype.display_name()),
        3 => ws_size_magnitude(&a.size).cmp(&ws_size_magnitude(&b.size)),
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    }
}

/// The element count implied by a size string like "10x10" or "1×5"; 0 if it has
/// no parseable dimensions, so non-array entries sort first.
fn ws_size_magnitude(size: &str) -> u64 {
    let dims: Vec<u64> = size.split(['x', '×', 'X']).filter_map(|p| p.trim().parse().ok()).collect();
    if dims.is_empty() {
        0
    } else {
        dims.iter().product()
    }
}

fn ws_variable_row(v: &matforge_core::models::WorkspaceVariable) -> Button {
    let btn = Button::new();
    btn.set_has_frame(false);
    btn.add_css_class("mf-row");
    let row = GtkBox::new(Orientation::Horizontal, 4);
    let preview = if v.preview.is_empty() { "—".into() } else { v.preview.clone() };
    let texts = [v.name.clone(), preview.clone(), v.dtype.display_name().to_string(), v.size.clone()];
    for (c, text) in WS_COLS.iter().zip(texts) {
        let l = Label::new(Some(&text));
        l.set_xalign(c.xalign);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_width_chars(c.min_chars);
        l.set_hexpand(c.expand);
        if c.dim {
            l.add_css_class("mf-cell-dim");
        }
        row.append(&l);
    }
    btn.set_child(Some(&row));
    // Full details on hover, so nothing is lost when a cell ellipsizes.
    btn.set_tooltip_text(Some(&format!(
        "{} = {}\n{} · {}",
        v.name,
        preview,
        v.dtype.display_name(),
        v.size
    )));
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

/// Spreadsheet view of the inspected matrix: row/column headers + numeric cells
/// in a scrollable grid. Rebuilds when a new value is captured.
fn build_table_viewer(app: &Rc<AppState>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 0);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_hexpand(true);
    let grid = gtk::Grid::new();
    grid.add_css_class("mf-table");
    grid.set_row_spacing(0);
    grid.set_column_spacing(0);
    scroll.set_child(Some(&grid));
    v.append(&scroll);

    let rebuild = {
        let app = app.clone();
        let grid = grid.clone();
        move |_: &Option<matforge_core::models::MatrixView>| {
            while let Some(child) = grid.first_child() {
                grid.remove(&child);
            }
            let Some(m) = app.vm.workspace.inspected_matrix.get() else {
                let empty = Label::new(Some("Select a variable to view its cells"));
                empty.add_css_class("mf-text-muted");
                empty.set_margin_top(12);
                empty.set_margin_start(10);
                grid.attach(&empty, 0, 0, 1, 1);
                return;
            };
            // Cap the rendered grid so huge matrices stay responsive.
            let max_r = m.rows.min(200);
            let max_c = m.cols.min(60);
            let corner = table_cell("", true);
            grid.attach(&corner, 0, 0, 1, 1);
            for c in 0..max_c {
                grid.attach(&table_cell(&format!("{}", c + 1), true), c as i32 + 1, 0, 1, 1);
            }
            for r in 0..max_r {
                grid.attach(&table_cell(&format!("{}", r + 1), true), 0, r as i32 + 1, 1, 1);
                for c in 0..max_c {
                    let val = m.cells.get(r).and_then(|row| row.get(c)).copied().unwrap_or(0.0);
                    grid.attach(&table_cell(&fmt_cell(val), false), c as i32 + 1, r as i32 + 1, 1, 1);
                }
            }
            if m.rows > max_r || m.cols > max_c {
                let note = Label::new(Some(&format!(
                    "showing {max_r}×{max_c} of {}×{}",
                    m.rows, m.cols
                )));
                note.add_css_class("mf-text-muted");
                grid.attach(&note, 0, max_r as i32 + 1, (max_c + 1) as i32, 1);
            }
        }
    };
    rebuild(&None);
    app.vm.workspace.inspected_matrix.subscribe(rebuild);
    v
}

fn table_cell(text: &str, header: bool) -> Label {
    let l = Label::new(Some(text));
    l.add_css_class(if header { "mf-table-head" } else { "mf-table-cell" });
    l.set_xalign(if header { 0.5 } else { 1.0 });
    l.set_width_chars(if header { 4 } else { 9 });
    l
}

/// Compact numeric formatting: integers without a decimal, else 4 sig digits.
fn fmt_cell(v: f64) -> String {
    if !v.is_finite() {
        return "NaN".into();
    }
    if v == v.trunc() && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{v:.4}")
    }
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
    let export = header_action(ic::SAVE);
    export.set_tooltip_text(Some("Export selected figure as PNG"));
    {
        let app = app.clone();
        export.connect_clicked(move |_| export_selected_figure(&app));
    }
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
    panel.append(&panel_header("PLOTS", &[add, refresh, export, trash, clear, close]));

    let list = ListBox::new();
    let list_scroll = ScrolledWindow::new();
    list_scroll.set_min_content_height(90);
    list_scroll.set_child(Some(&list));
    panel.append(&list_scroll);

    let canvas = gtk::DrawingArea::new();
    canvas.set_vexpand(true);
    canvas.set_hexpand(true);

    // Interactive view state. `view` is the zoom/pan window tagged with the
    // figure id it belongs to (so switching figures auto-resets); `hover` is the
    // cursor pixel for the crosshair readout.
    let view: Rc<Cell<Option<(u64, matforge_core::models::PlotView)>>> = Rc::new(Cell::new(None));
    let hover: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
    // Orbit camera for 3-D surface figures, tagged with its figure id.
    let cam: Rc<Cell<Option<(u64, matforge_core::models::SurfaceCamera)>>> = Rc::new(Cell::new(None));

    // Playback state. `play_idx` is the current animation step (runtime frame, or
    // revealed-point count for a 2-D series); `playing` drives the tick loop;
    // `play_active` engages series trace-reveal; `follow` keeps a live runtime
    // figure pinned to its newest frame until the user scrubs.
    let play_idx: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let playing: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let play_active: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let follow: Rc<Cell<bool>> = Rc::new(Cell::new(true));

    // The figure currently drawn (selected, else most recent), if any.
    let current_fig = {
        let app = app.clone();
        move || {
            let figs = app.vm.plots.figures.get();
            let sel = app.vm.plots.selected_id.get();
            figs.iter().find(|f| Some(f.id) == sel).or_else(|| figs.last()).cloned()
        }
    };
    // The effective view for `fig`: the stored window if it's for this figure,
    // else the auto-fit window.
    let eff_view = {
        let view = view.clone();
        move |fig: &matforge_core::models::PlotFigure| {
            match view.get() {
                Some((id, v)) if id == fig.id => Some(v),
                _ => fig.auto_view(),
            }
        }
    };
    // The effective camera for `fig`: the stored one if it's for this figure,
    // else the default 3/4 view.
    let eff_cam = {
        let cam = cam.clone();
        move |fig: &matforge_core::models::PlotFigure| match cam.get() {
            Some((id, c)) if id == fig.id => c,
            _ => matforge_core::models::SurfaceCamera::default(),
        }
    };

    {
        let current_fig = current_fig.clone();
        let eff_view = eff_view.clone();
        let eff_cam = eff_cam.clone();
        let hover = hover.clone();
        let play_idx = play_idx.clone();
        let play_active = play_active.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| match current_fig() {
            Some(figure) if figure.is_surface() => {
                crate::plot_render::draw_surface(ctx, w as f64, h as f64, &figure, eff_cam(&figure));
            }
            // Runtime animation: show the scrubbed frame.
            Some(figure) if figure.is_animated() => {
                let i = play_idx.get().min(figure.frames.len() - 1);
                crate::plot_render::draw_png_frame(ctx, w as f64, h as f64, &figure.frames[i]);
            }
            Some(figure) => {
                let v = eff_view(&figure);
                let hov = if figure.is_interactive() { hover.get() } else { None };
                // Trace reveal only while the series animation is engaged.
                let reveal = if play_active.get() && figure.animation_len() > 1 {
                    Some(play_idx.get() + 1)
                } else {
                    None
                };
                crate::plot_render::draw_figure(ctx, w as f64, h as f64, &figure, v, hov, reveal);
            }
            None => crate::plot_render::draw_empty(ctx, w as f64, h as f64),
        });
    }
    panel.append(&canvas);

    // ---- Playback bar: play/pause, scrubber, frame counter ----
    let playbar = GtkBox::new(Orientation::Horizontal, 6);
    playbar.add_css_class("mf-playbar");
    let play_btn = Button::from_icon_name(ic::RUN);
    play_btn.add_css_class("mf-header-action");
    play_btn.set_tooltip_text(Some("Play / pause animation"));
    let scrubber = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 1.0);
    scrubber.set_hexpand(true);
    scrubber.set_draw_value(false);
    let counter = Label::new(None);
    counter.add_css_class("mf-cell-dim");
    counter.set_width_chars(9);
    playbar.append(&play_btn);
    playbar.append(&scrubber);
    playbar.append(&counter);
    playbar.set_visible(false);
    panel.append(&playbar);

    // Guards the scrubber's change handler against the tick loop's own updates.
    let suppress: Rc<Cell<bool>> = Rc::new(Cell::new(false));

    // Sync the bar's visibility, range, and counter to `fig`.
    let update_bar = {
        let playbar = playbar.clone();
        let scrubber = scrubber.clone();
        let counter = counter.clone();
        let play_idx = play_idx.clone();
        let suppress = suppress.clone();
        move |fig: &Option<matforge_core::models::PlotFigure>| match fig {
            Some(f) if f.is_surface() => {
                playbar.set_visible(true);
                scrubber.set_visible(false);
                counter.set_visible(false);
            }
            Some(f) if f.animation_len() > 1 => {
                let len = f.animation_len();
                let idx = play_idx.get().min(len - 1);
                playbar.set_visible(true);
                scrubber.set_visible(true);
                counter.set_visible(true);
                suppress.set(true);
                scrubber.set_range(0.0, (len - 1) as f64);
                scrubber.set_value(idx as f64);
                suppress.set(false);
                counter.set_text(&format!("{} / {}", idx + 1, len));
            }
            _ => playbar.set_visible(false),
        }
    };
    update_bar(&current_fig());

    // Play / pause toggles the tick loop. Starting at the end replays from 0.
    {
        let playing = playing.clone();
        let play_active = play_active.clone();
        let follow = follow.clone();
        let play_idx = play_idx.clone();
        let current_fig = current_fig.clone();
        let play_btn = play_btn.clone();
        play_btn.connect_clicked(move |b| {
            let now = !playing.get();
            playing.set(now);
            if now {
                if let Some(f) = current_fig() {
                    let len = f.animation_len();
                    if len > 1 && play_idx.get() >= len - 1 {
                        play_idx.set(0); // restart a finished animation
                    }
                }
                play_active.set(true);
                follow.set(false);
                b.set_icon_name(ic::PAUSE);
            } else {
                b.set_icon_name(ic::RUN);
            }
        });
    }

    // Scrubbing pauses playback and jumps to the chosen step.
    {
        let suppress = suppress.clone();
        let playing = playing.clone();
        let play_active = play_active.clone();
        let follow = follow.clone();
        let play_idx = play_idx.clone();
        let play_btn = play_btn.clone();
        let counter = counter.clone();
        let current_fig = current_fig.clone();
        let canvas2 = canvas.clone();
        scrubber.connect_value_changed(move |s| {
            if suppress.get() {
                return;
            }
            playing.set(false);
            play_active.set(true);
            follow.set(false);
            play_btn.set_icon_name(ic::RUN);
            let idx = s.value().round() as usize;
            play_idx.set(idx);
            if let Some(f) = current_fig() {
                let len = f.animation_len().max(1);
                counter.set_text(&format!("{} / {}", idx.min(len - 1) + 1, len));
            }
            canvas2.queue_draw();
        });
    }

    // The animation tick: advance frames / auto-orbit while playing.
    {
        let playing = playing.clone();
        let play_idx = play_idx.clone();
        let play_active = play_active.clone();
        let suppress = suppress.clone();
        let current_fig = current_fig.clone();
        let eff_cam = eff_cam.clone();
        let cam = cam.clone();
        let scrubber = scrubber.clone();
        let counter = counter.clone();
        let canvas2 = canvas.clone();
        let last_us: Rc<Cell<i64>> = Rc::new(Cell::new(0));
        const FPS: i64 = 20;
        canvas.add_tick_callback(move |_w, clock| {
            if playing.get() {
                if let Some(f) = current_fig() {
                    if f.is_surface() {
                        // Continuous auto-orbit.
                        cam.set(Some((f.id, eff_cam(&f).orbit_by(0.02, 0.0))));
                        canvas2.queue_draw();
                    } else {
                        let len = f.animation_len();
                        let now = clock.frame_time();
                        if len > 1 && now - last_us.get() >= 1_000_000 / FPS {
                            last_us.set(now);
                            let next = (play_idx.get() + 1) % len;
                            play_idx.set(next);
                            play_active.set(true);
                            suppress.set(true);
                            scrubber.set_value(next as f64);
                            suppress.set(false);
                            counter.set_text(&format!("{} / {}", next + 1, len));
                            canvas2.queue_draw();
                        }
                    }
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    // Switching figures resets the zoom/pan window, the orbit camera, and the
    // playback state (re-pinning a live runtime figure to its newest frame).
    {
        let view = view.clone();
        let cam = cam.clone();
        let canvas = canvas.clone();
        let playing = playing.clone();
        let play_active = play_active.clone();
        let follow = follow.clone();
        let play_idx = play_idx.clone();
        let play_btn = play_btn.clone();
        let update_bar = update_bar.clone();
        let current_fig = current_fig.clone();
        app.vm.plots.selected_id.subscribe(move |_| {
            view.set(None);
            cam.set(None);
            playing.set(false);
            play_active.set(false);
            follow.set(true);
            play_btn.set_icon_name(ic::RUN);
            // Pin to the newest frame of the now-current figure.
            let fig = current_fig();
            play_idx.set(fig.as_ref().map(|f| f.animation_len().saturating_sub(1)).unwrap_or(0));
            update_bar(&fig);
            canvas.queue_draw();
        });
    }

    // Hover → crosshair readout.
    {
        let motion = gtk::EventControllerMotion::new();
        let hover2 = hover.clone();
        let hover1 = hover.clone();
        let canvas2 = canvas.clone();
        motion.connect_motion(move |_, x, y| {
            hover1.set(Some((x, y)));
            canvas2.queue_draw();
        });
        let canvas3 = canvas.clone();
        motion.connect_leave(move |_| {
            hover2.set(None);
            canvas3.queue_draw();
        });
        canvas.add_controller(motion);
    }

    // Scroll → zoom around the cursor.
    {
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let current_fig = current_fig.clone();
        let eff_view = eff_view.clone();
        let eff_cam = eff_cam.clone();
        let view = view.clone();
        let cam = cam.clone();
        let hover = hover.clone();
        let canvas2 = canvas.clone();
        scroll.connect_scroll(move |_c, _dx, dy| {
            let Some(fig) = current_fig() else { return gtk::glib::Propagation::Proceed };
            // 3-D surface: scroll scales the orbit camera.
            if fig.is_surface() {
                let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
                cam.set(Some((fig.id, eff_cam(&fig).zoom_by(factor))));
                canvas2.queue_draw();
                return gtk::glib::Propagation::Stop;
            }
            let Some(v) = eff_view(&fig) else { return gtk::glib::Propagation::Proceed };
            let (w, h) = (canvas2.width() as f64, canvas2.height() as f64);
            let (cx, cy) = hover.get().unwrap_or((w / 2.0, h / 2.0));
            let (zx, zy) = crate::plot_render::data_at_pixel(v, w, h, cx, cy);
            let factor = if dy < 0.0 { 0.85 } else { 1.0 / 0.85 };
            view.set(Some((fig.id, v.zoom_at(zx, zy, factor))));
            canvas2.queue_draw();
            gtk::glib::Propagation::Stop
        });
        canvas.add_controller(scroll);
    }

    // Left-drag → pan a 2-D plot, or orbit a 3-D surface.
    {
        let drag = gtk::GestureDrag::new();
        drag.set_button(gtk::gdk::BUTTON_PRIMARY);
        let start_view: Rc<Cell<Option<(u64, matforge_core::models::PlotView)>>> = Rc::new(Cell::new(None));
        let start_cam: Rc<Cell<Option<(u64, matforge_core::models::SurfaceCamera)>>> = Rc::new(Cell::new(None));
        {
            let current_fig = current_fig.clone();
            let eff_view = eff_view.clone();
            let eff_cam = eff_cam.clone();
            let start_view = start_view.clone();
            let start_cam = start_cam.clone();
            drag.connect_drag_begin(move |_g, _x, _y| {
                let fig = current_fig();
                match &fig {
                    Some(f) if f.is_surface() => {
                        start_cam.set(Some((f.id, eff_cam(f))));
                        start_view.set(None);
                    }
                    _ => {
                        start_view.set(fig.and_then(|f| eff_view(&f).map(|v| (f.id, v))));
                        start_cam.set(None);
                    }
                }
            });
        }
        {
            let start_view = start_view.clone();
            let start_cam = start_cam.clone();
            let view = view.clone();
            let cam = cam.clone();
            let canvas2 = canvas.clone();
            drag.connect_drag_update(move |g, dx, dy| {
                // 3-D orbit: drag rotates azimuth (x) and elevation (y).
                if let Some((id, sc)) = start_cam.get() {
                    cam.set(Some((id, sc.orbit_by(dx * 0.01, dy * 0.01))));
                    canvas2.queue_draw();
                    return;
                }
                let Some((id, sv)) = start_view.get() else { return };
                let Some((sx, sy)) = g.start_point() else { return };
                let (w, h) = (canvas2.width() as f64, canvas2.height() as f64);
                let a = crate::plot_render::data_at_pixel(sv, w, h, sx, sy);
                let b = crate::plot_render::data_at_pixel(sv, w, h, sx + dx, sy + dy);
                view.set(Some((id, sv.pan_by(b.0 - a.0, b.1 - a.1))));
                canvas2.queue_draw();
            });
        }
        canvas.add_controller(drag);
    }

    // Double-click → reset zoom/pan and orbit to their defaults.
    {
        let click = gtk::GestureClick::new();
        let view = view.clone();
        let cam = cam.clone();
        let canvas2 = canvas.clone();
        click.connect_pressed(move |_g, n, _x, _y| {
            if n >= 2 {
                view.set(None);
                cam.set(None);
                canvas2.queue_draw();
            }
        });
        canvas.add_controller(click);
    }

    // Rebuild the figure list + redraw when figures or selection change.
    let rebuild = {
        let app = app.clone();
        let list = list.clone();
        let canvas = canvas.clone();
        move || {
            clear_list(&list);
            let selected = app.vm.plots.selected_id.get();
            for f in app.vm.plots.figures.get() {
                let row = GtkBox::new(Orientation::Horizontal, 6);
                row.add_css_class("mf-row");
                if Some(f.id) == selected {
                    row.add_css_class("selected");
                }

                // Live thumbnail (re-renders on every figure upsert).
                let thumb = gtk::DrawingArea::new();
                thumb.set_size_request(58, 38);
                thumb.add_css_class("mf-thumb");
                let fig = f.clone();
                thumb.set_draw_func(move |_a, ctx, w, h| {
                    crate::plot_render::draw_thumbnail(ctx, w as f64, h as f64, &fig);
                });
                row.append(&thumb);

                let btn = Button::with_label(&format!("Figure {} — {}", f.index, f.title));
                btn.set_has_frame(false);
                btn.set_halign(gtk::Align::Start);
                btn.set_hexpand(true);
                let app2 = app.clone();
                let id = f.id;
                btn.connect_clicked(move |_| app2.vm.plots.select(id));
                row.append(&btn);

                // Figures fed by the runtime emit protocol are "live" (they
                // upsert in place, e.g. during a drawnow animation).
                if f.runtime_id.is_some() {
                    let live = Label::new(Some("LIVE"));
                    live.add_css_class("mf-pill");
                    live.add_css_class("mf-pill-live");
                    live.set_valign(gtk::Align::Center);
                    live.set_margin_end(6);
                    row.append(&live);
                }
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
        let rebuild = rebuild.clone();
        app.vm.plots.selected_id.subscribe(move |_| rebuild());
    }

    // As runtime frames stream in, keep a followed figure pinned to its newest
    // frame and refresh the playback bar's range/counter.
    {
        let follow = follow.clone();
        let playing = playing.clone();
        let play_idx = play_idx.clone();
        let update_bar = update_bar.clone();
        let current_fig = current_fig.clone();
        let canvas = canvas.clone();
        app.vm.plots.figures.subscribe(move |_| {
            let fig = current_fig();
            if let Some(f) = &fig {
                let len = f.animation_len();
                if follow.get() && !playing.get() && len > 0 {
                    play_idx.set(len - 1);
                }
            }
            update_bar(&fig);
            canvas.queue_draw();
        });
    }

    // Drop a workspace variable anywhere on the panel to chart it (a line plot).
    let drop = gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::COPY);
    {
        let app = app.clone();
        drop.connect_drop(move |_t, value, _x, _y| {
            if let Ok(name) = value.get::<String>() {
                app.plot_variable_as(&name, matforge_core::models::PlotKind::Line2D);
                true
            } else {
                false
            }
        });
    }
    panel.add_controller(drop);
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

#[cfg(test)]
mod completion_tests {
    use super::longest_common_prefix;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn single_candidate_completes_fully() {
        assert_eq!(longest_common_prefix(&v(&["zeros"])).as_deref(), Some("zeros"));
    }

    #[test]
    fn many_candidates_yield_common_prefix() {
        assert_eq!(longest_common_prefix(&v(&["plot", "plot3", "plotyy"])).as_deref(), Some("plot"));
        assert_eq!(longest_common_prefix(&v(&["sin", "size", "sign"])).as_deref(), Some("si"));
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(longest_common_prefix(&[]), None);
    }
}

#[cfg(test)]
mod link_tests {
    use super::find_source_link;

    #[test]
    fn parses_clang_style_reference() {
        let line = "examples/bad.m:12:9: error: expected expression";
        let (s, e, file, ln) = find_source_link(line).expect("link");
        assert_eq!(&line[s..e], "examples/bad.m:12:9");
        assert_eq!(file, "examples/bad.m");
        assert_eq!(ln, 12);
    }

    #[test]
    fn parses_absolute_path_with_line_only() {
        let (_, _, file, ln) = find_source_link("/home/u/foo.m:7: warning: x").expect("link");
        assert_eq!(file, "/home/u/foo.m");
        assert_eq!(ln, 7);
    }

    #[test]
    fn ignores_non_file_ranges_and_tool_prefixes() {
        assert!(find_source_link("ans = 3:5").is_none());
        assert!(find_source_link("matlabc: No such file or directory").is_none());
        assert!(find_source_link("plain output with no reference").is_none());
    }
}
