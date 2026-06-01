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

mod highlight;
mod runner;
mod services_impl;
mod ui;

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

    ui::build(&window, vm.clone());

    // Optional startup open (used for demos / verification):
    //   MATFORGE_OPEN=<folder>  MATFORGE_FILE=<file>
    if let Ok(folder) = std::env::var("MATFORGE_OPEN") {
        let _ = vm.open_folder(std::path::Path::new(&folder));
    }
    if let Ok(file) = std::env::var("MATFORGE_FILE") {
        ui::open_file_path(&vm, std::path::Path::new(&file));
    }
    if std::env::var("MATFORGE_COMPILE").is_ok() {
        runner::compile(&vm);
    }

    if !runner::matlabc_available(&settings) {
        vm.status_bar.set_message(format!(
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
