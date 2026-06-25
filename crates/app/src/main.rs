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
mod block_library;
mod e2e;
mod editor_view;
mod flow_render;
mod flowchart_view;
mod highlight;
mod icons;
mod mermaid_render;
mod mflowlink_window;
mod palette;
mod plot_render;
mod process;
mod runner;
mod scene3d_window;
mod scope_render;
mod services_impl;
mod settings_view;
mod statechart_window;
mod theme_css;
mod transition_table;
mod ui;
mod video_view;

use app_state::AppState;
use services_impl::{GtkClipboard, NoopFilePicker};

const APP_ID: &str = "org.matlab_llvm.MatForge";

fn main() -> glib::ExitCode {
    // Headless scope export: simulate a model and render the overlay scope to a
    // PNG (`<model>.scope-preview.png`), then exit — verifies the scope visuals
    // without a display, and useful for docs / regression snapshots.
    if let Ok(m) = std::env::var("MATFORGE_SCOPE_PNG") {
        return render_scope_preview(std::path::Path::new(&m));
    }

    let app_id = std::env::var("MATFORGE_APP_ID").unwrap_or_else(|_| APP_ID.to_string());
    let app = Application::builder().application_id(app_id).build();
    app.connect_startup(|_| {
        icons::install();
    });
    app.connect_activate(build_main_window);
    app.run()
}

/// One previewed signal: display name, stable color, and its `(time, value)` points.
type PreviewSignal = (String, (f64, f64, f64), Vec<(f64, f64)>);

/// Run `matlabc -simulate <mflow>`, fold the CSV into a `SimTrace`, and render
/// the production overlay scope to `<mflow>.scope-preview.png` — the exact
/// `scope_render` the mflowLink window uses, offscreen.
fn render_scope_preview(mflow: &std::path::Path) -> glib::ExitCode {
    use matforge_core::services::scope::{signal_color, ScopeView};
    use matforge_core::services::sim_trace::SimTrace;
    use matforge_core::theme::{ThemeId, ThemeTokens};

    let settings = Settings::from_env();
    let output = std::process::Command::new(&settings.matlabc_path)
        .arg("-simulate")
        .arg(mflow)
        .output();
    let Ok(output) = output else {
        eprintln!("scope preview: failed to run matlabc");
        return glib::ExitCode::FAILURE;
    };
    let mut trace = SimTrace::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        trace.feed_line(line);
    }

    theme_css::set_current(ThemeTokens::for_id(ThemeId::Midnight));

    let data: Vec<PreviewSignal> = (0..trace.signal_count())
        .map(|i| {
            let (xs, ys) = trace.series(i);
            let pts: Vec<(f64, f64)> = xs.iter().zip(&ys).map(|(&x, &y)| (x, y)).collect();
            let name = trace.signal_name(i).unwrap_or("signal").to_string();
            (name, signal_color(i), pts)
        })
        .collect();
    let points: Vec<Vec<(f64, f64)>> = data.iter().map(|d| d.2.clone()).collect();
    let window = ScopeView::default().resolve(&points);
    let series: Vec<scope_render::ScopeSeries> = data
        .iter()
        .map(|(name, color, pts)| scope_render::ScopeSeries {
            name,
            color: *color,
            points: pts,
        })
        .collect();

    let (w, h) = (720, 400);
    let surface = match gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("scope preview: surface error: {e}");
            return glib::ExitCode::FAILURE;
        }
    };
    if let Ok(ctx) = gtk::cairo::Context::new(&surface) {
        scope_render::draw_overlay(&ctx, w as f64, h as f64, &series, window, None, None);
    }
    let dest = mflow.with_extension("scope-preview.png");
    let result = std::fs::File::create(&dest)
        .map_err(|e| e.to_string())
        .and_then(|mut f| surface.write_to_png(&mut f).map_err(|e| e.to_string()));
    match result {
        Ok(()) => {
            println!(
                "scope preview: wrote {} ({} signals, {} samples)",
                dest.display(),
                trace.signal_count(),
                trace.series(0).0.len()
            );
            glib::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("scope preview: {e}");
            glib::ExitCode::FAILURE
        }
    }
}

fn build_main_window(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("MatForge IDE")
        .default_width(1600)
        .default_height(980)
        .build();
    window.add_css_class("mf-root");
    // App icon (installed into the icon search path by `icons::install`).
    window.set_icon_name(Some(icons::APP));
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
        prefs.appearance.code_font_scale,
        prefs.appearance.code_font.clone(),
    );
    // Restore panel visibility before the panels are built.
    app.vm
        .layout
        .sidebar_visible
        .set(prefs.layout.sidebar_visible);
    app.vm
        .layout
        .workspace_visible
        .set(prefs.layout.workspace_visible);
    app.vm.layout.plots_visible.set(prefs.layout.plots_visible);
    app.vm
        .layout
        .flow_palette_visible
        .set(prefs.layout.flow_palette_visible);

    ui::build(&window, app.clone());

    install_theming(&window, &app);

    // When a program writes a video (VideoWriter), play it back in-IDE.
    {
        let app2 = app.clone();
        let last_video = app.vm.last_video.clone();
        last_video.subscribe(move |v| {
            let Some(path) = v.as_ref().map(std::path::PathBuf::from) else {
                return;
            };
            if path.exists() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                app2.vm.toast.show(format!("🎬 Playing {name}"));
                video_view::open(&app2, &path);
            }
        });
    }

    // When a sim3d script exports a scene (`sim3d.export(w, '…html')`), open it
    // in the embedded 3-D Scene viewer.
    {
        let app2 = app.clone();
        let last_scene3d = app.vm.last_scene3d.clone();
        last_scene3d.subscribe(move |v| {
            let Some(path) = v.as_ref().map(std::path::PathBuf::from) else {
                return;
            };
            if path.exists() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                app2.vm.toast.show(format!("🧊 3-D scene {name}"));
                crate::e2e::record_scene3d_generated(&path);
                // The window is suppressed under the e2e harness (the test
                // asserts on the recorded scene, not an un-introspectable view).
                if !crate::e2e::is_active() {
                    scene3d_window::open(&app2, &path);
                }
            }
        });
    }

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
        let ys: Vec<f64> = xs
            .iter()
            .map(|x| (x * 1.5).sin() * (-x * 0.1).exp())
            .collect();
        app.vm.plots.add(PlotFigure::series(
            1,
            "damped sine",
            PlotKind::Line2D,
            xs,
            ys,
        ));
    }
    if let Ok(kind) = std::env::var("MATFORGE_NEWFLOW") {
        ui::open_demo_flowchart(&app, kind == "signal");
    }
    // Deterministically drive one flowchart editor operation (wire select/delete,
    // edge breakpoint, MATLAB Function port growth) for the e2e harness.
    if let Ok(op) = std::env::var("MATFORGE_FLOW_OP") {
        ui::flow_e2e_op(&op);
    }
    if let Ok(p) = std::env::var("MATFORGE_SIMULATE") {
        let path = std::path::PathBuf::from(&p);
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(doc) = matforge_core::services::flowchart_codec::decode_str(&text) {
                // MATFORGE_SIMULATE_LIVE drives the interactive live `--sim-dap`
                // path (what the editor's Simulate→Play does) instead of the
                // one-shot autostart; the window auto-continues to completion.
                let live = std::env::var("MATFORGE_SIMULATE_LIVE").is_ok();
                mflowlink_window::open(&app, doc, Some(path), !live);
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
    // Deterministically open the Search panel (what Ctrl+F does), so the e2e
    // harness needn't rely on a keyboard accelerator landing under headless X.
    if std::env::var("MATFORGE_SEARCH").is_ok() {
        app.vm
            .activity_bar
            .select(matforge_core::viewmodels::ActivityItem::Search);
        app.vm.layout.sidebar_visible.set(true);
    }
    // Demo/verification: force a theme/accent at launch.
    if let Ok(theme) = std::env::var("MATFORGE_THEME") {
        app.vm
            .appearance
            .set_theme(matforge_core::theme::ThemeId::from_key(&theme));
    }
    if let Ok(accent) = std::env::var("MATFORGE_ACCENT") {
        app.vm
            .appearance
            .set_accent(matforge_core::theme::Accent::from_key(&accent));
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

    if let Ok(v) = std::env::var("MATFORGE_VIDEO") {
        video_view::open(&app, std::path::Path::new(&v));
    }
    if std::env::var("MATFORGE_PREFS").is_ok() {
        settings_view::open(&app, Some(&window));
    }
    if std::env::var("MATFORGE_PALETTE").is_ok() {
        palette::open_command_palette(&app, &window);
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
            let ui_scale = app.vm.appearance.font_scale.get();
            let code_scale = app.vm.appearance.code_font_scale.get();
            provider.load_from_string(&theme_css::render(&tokens, ui_scale, code_scale));
            theme_css::set_current(tokens);
            theme_css::set_code_scale(code_scale);
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
    prefs.appearance.code_font_scale = a.code_font_scale.get();
    prefs.appearance.code_font = a.code_font_family.get();

    let l = &app.vm.layout;
    prefs.layout.sidebar_visible = l.sidebar_visible.get();
    prefs.layout.workspace_visible = l.workspace_visible.get();
    prefs.layout.plots_visible = l.plots_visible.get();
    prefs.layout.flow_palette_visible = l.flow_palette_visible.get();
    // Persist the live divider positions so the layout is restored next launch.
    if let Some((sidebar_width, workspace_split)) = ui::layout_pane_positions() {
        if sidebar_width > 0 {
            prefs.layout.sidebar_width = sidebar_width;
        }
        if workspace_split > 0 {
            prefs.layout.workspace_split = workspace_split;
        }
    }

    prefs.open_tabs = app.vm.editor.tabs.with(|ts| {
        ts.iter()
            .filter_map(|t| t.url.as_ref().map(|u| u.display().to_string()))
            .collect()
    });
    prefs.last_folder = app
        .vm
        .project
        .root_url
        .get()
        .map(|u| u.display().to_string());
    if let Some(folder) = prefs.last_folder.clone() {
        prefs.push_recent(folder);
    }
    let _ = prefs.save();
}
