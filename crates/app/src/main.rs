//! MatForge IDE — GTK4 entry point.
//!
//! Boots a [`gtk::Application`], installs the dark CSS theme, constructs the
//! `MainViewModel` with the real (GTK/process) service implementations, and
//! hands it to the view builder. App lifecycle + theming only; all panels and
//! their wiring live in [`ui`].

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib, Application, ApplicationWindow, CssProvider};

use matforge_core::services::filesystem::RealFileSystem;
use matforge_core::services::settings::Settings;
use matforge_core::viewmodels::MainViewModel;

mod app_state;
mod e2e;
mod editor_view;
mod flow_render;
mod icons;
mod flowchart_view;
mod highlight;
mod mflowlink_window;
mod plot_render;
mod statechart_window;
mod process;
mod runner;
mod services_impl;
mod settings_view;
mod theme_css;
mod ui;

use app_state::AppState;
use services_impl::{GtkClipboard, NoopFilePicker};

const APP_ID: &str = "org.matlab_llvm.MatForge";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        icons::install();
    });
    app.connect_activate(build_main_window);
    app.run()
}

fn build_main_window(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("MatForge IDE")
        .default_width(1600)
        .default_height(980)
        .build();
    window.add_css_class("mf-root");
    window.set_size_request(1440, 820);

    let settings = Settings::from_env();
    let vm = Rc::new(MainViewModel::new(
        Rc::new(RealFileSystem),
        Rc::new(GtkClipboard),
        Rc::new(NoopFilePicker),
        settings.clone(),
    ));
    let app = AppState::new(vm, settings.clone());

    // Apply persisted appearance before the first paint, then keep the CSS +
    // Cairo renderers in sync with the appearance view model at runtime.
    let prefs = matforge_core::services::preferences::Preferences::load();
    app.vm.appearance.apply(
        prefs.appearance.theme_id(),
        prefs.appearance.accent_enum(),
        prefs.appearance.font_scale,
        prefs.appearance.code_font.clone(),
    );
    // Restore panel visibility before the panels are built.
    app.vm.layout.sidebar_visible.set(prefs.layout.sidebar_visible);
    app.vm.layout.workspace_visible.set(prefs.layout.workspace_visible);
    app.vm.layout.plots_visible.set(prefs.layout.plots_visible);

    ui::build(&window, app.clone());

    install_theming(&window, &app);

    // Save the session (layout + open tabs + folder) on a clean window close.
    {
        let app = app.clone();
        window.connect_close_request(move |_| {
            save_prefs(&app);
            gtk::glib::Propagation::Proceed
        });
    }

    // E2E state introspection (test-only; no-op unless the env var is set).
    if let Ok(path) = std::env::var("MATFORGE_E2E_STATE") {
        e2e::install_state_dump(app.clone(), std::path::PathBuf::from(path));
    }

    // Optional startup open (used for demos / verification):
    //   MATFORGE_OPEN=<folder>  MATFORGE_FILE=<file>  MATFORGE_COMPILE=1
    if let Ok(folder) = std::env::var("MATFORGE_OPEN") {
        let _ = app.vm.open_folder(std::path::Path::new(&folder));
    }
    if let Ok(file) = std::env::var("MATFORGE_FILE") {
        ui::open_file_path(&app, std::path::Path::new(&file));
    }
    if std::env::var("MATFORGE_COMPILE").is_ok() {
        runner::compile(&app.vm);
    }
    if std::env::var("MATFORGE_RUN").is_ok() {
        runner::run(app.vm.clone(), &settings);
    }
    if let Ok(cmd) = std::env::var("MATFORGE_REPL") {
        app.repl_send(&cmd);
    }
    if std::env::var("MATFORGE_DEBUG").is_ok() {
        app.start_debug();
    }
    if std::env::var("MATFORGE_PLOT").is_ok() {
        use matforge_core::models::{PlotFigure, PlotKind};
        let xs: Vec<f64> = (0..240).map(|i| i as f64 * 0.05).collect();
        let ys: Vec<f64> = xs.iter().map(|x| (x * 1.5).sin() * (-x * 0.1).exp()).collect();
        app.vm.plots.add(PlotFigure::series(1, "damped sine", PlotKind::Line2D, xs, ys));
    }
    if let Ok(kind) = std::env::var("MATFORGE_NEWFLOW") {
        ui::open_demo_flowchart(&app, kind == "signal");
    }
    if let Ok(p) = std::env::var("MATFORGE_SIMULATE") {
        let path = std::path::PathBuf::from(&p);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(doc) = matforge_core::services::flowchart_codec::decode_str(&text) {
                mflowlink_window::open(&app, doc, Some(path), true);
            }
        }
    }
    if let Ok(p) = std::env::var("MATFORGE_STATECHART") {
        let path = std::path::PathBuf::from(&p);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(doc) = matforge_core::services::flowchart_codec::decode_str(&text) {
                statechart_window::open(&app, doc, Some(path), true);
            }
        }
    }
    if let Ok(line) = std::env::var("MATFORGE_BP") {
        if let (Ok(n), Some(tab)) = (line.parse::<usize>(), app.vm.editor.active_tab()) {
            app.vm.editor.toggle_breakpoint(tab.id, n);
        }
    }
    if std::env::var("MATFORGE_NORIGHT").is_ok() {
        app.vm.layout.workspace_visible.set(false);
    }
    // Demo/verification: force a theme/accent at launch.
    if let Ok(theme) = std::env::var("MATFORGE_THEME") {
        app.vm.appearance.set_theme(matforge_core::theme::ThemeId::from_key(&theme));
    }
    if let Ok(accent) = std::env::var("MATFORGE_ACCENT") {
        app.vm.appearance.set_accent(matforge_core::theme::Accent::from_key(&accent));
    }
    if std::env::var("MATFORGE_ZEN").is_ok() {
        app.vm.layout.zen.set(true);
    }

    // Session restore: reopen the last folder + tabs when nothing was opened via
    // env (so explicit MATFORGE_OPEN/FILE always win).
    if std::env::var("MATFORGE_OPEN").is_err() && std::env::var("MATFORGE_FILE").is_err() {
        if let Some(folder) = &prefs.last_folder {
            let _ = app.vm.open_folder(std::path::Path::new(folder));
        }
        for tab in &prefs.open_tabs {
            ui::open_file_path(&app, std::path::Path::new(tab));
        }
    }

    if !runner::matlabc_available(&settings) {
        app.vm.status_bar.set_message(format!(
            "matlabc not found at {} — set $MATLABC_PATH",
            settings.matlabc_path.display()
        ));
    }

    window.present();

    if std::env::var("MATFORGE_PREFS").is_ok() {
        settings_view::open(&app, Some(&window));
    }
}

/// Install a swappable `CssProvider` driven by the appearance view model: render
/// the stylesheet from the active theme tokens + font scale, and re-render on any
/// appearance change (theme / accent / zoom). Also caches the tokens for the
/// Cairo renderers and persists the choice.
fn install_theming(window: &ApplicationWindow, app: &Rc<AppState>) {
    let provider = CssProvider::new();
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let render = {
        let app = app.clone();
        let provider = provider.clone();
        let window = window.clone();
        move || {
            let tokens = app.vm.appearance.tokens();
            let scale = app.vm.appearance.font_scale.get();
            provider.load_from_string(&theme_css::render(&tokens, scale));
            theme_css::set_current(tokens);
            // Re-tint the Cairo widgets (plots/flowchart/gutter) immediately.
            window.queue_draw();
            save_prefs(&app);
        }
    };
    render(); // initial paint
    app.vm.appearance.revision.subscribe(move |_| render());

    // Toast on theme switches (not on every font-zoom tick).
    {
        let app = app.clone();
        let theme_id = app.vm.appearance.theme_id.clone();
        theme_id.subscribe(move |id| {
            app.vm.toast.show(format!("Theme: {}", id.label()));
        });
    }
}

/// Persist appearance + layout + session (open tabs / last folder) to
/// `config.toml`, preserving fields this build does not own.
fn save_prefs(app: &Rc<AppState>) {
    use matforge_core::services::preferences::Preferences;
    let mut prefs = Preferences::load();

    let a = &app.vm.appearance;
    prefs.appearance.theme = a.theme_id.get().key().to_string();
    prefs.appearance.accent = a.accent.get().key().to_string();
    prefs.appearance.font_scale = a.font_scale.get();
    prefs.appearance.code_font = a.code_font_family.get();

    let l = &app.vm.layout;
    prefs.layout.sidebar_visible = l.sidebar_visible.get();
    prefs.layout.workspace_visible = l.workspace_visible.get();
    prefs.layout.plots_visible = l.plots_visible.get();

    prefs.open_tabs = app.vm.editor.tabs.with(|ts| {
        ts.iter().filter_map(|t| t.url.as_ref().map(|u| u.display().to_string())).collect()
    });
    prefs.last_folder = app.vm.project.root_url.get().map(|u| u.display().to_string());
    if let Some(folder) = prefs.last_folder.clone() {
        prefs.push_recent(folder);
    }
    let _ = prefs.save();
}
