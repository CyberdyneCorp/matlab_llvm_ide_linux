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
mod editor_view;
mod flow_render;
mod flowchart_view;
mod highlight;
mod plot_render;
mod process;
mod runner;
mod services_impl;
mod ui;

use app_state::AppState;
use services_impl::{GtkClipboard, NoopFilePicker};

const APP_ID: &str = "org.matlab_llvm.MatForge";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| install_css());
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

    ui::build(&window, app.clone());

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

    if !runner::matlabc_available(&settings) {
        app.vm.status_bar.set_message(format!(
            "matlabc not found at {} — set $MATLABC_PATH",
            settings.matlabc_path.display()
        ));
    }

    window.present();
}

/// Load the bundled CSS theme into the default display.
fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_string(include_str!("../resources/theme.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
