//! The mflowLink **3-D Scene** window: renders the compiler's self-contained
//! Babylon.js HTML (`matlabc -emit-mflowlink-babylon`) so the user can orbit,
//! zoom, and pan the 3-D scene inside the IDE.
//!
//! The embedded WebKitGTK view is gated behind the `scene3d` cargo feature so
//! the default build needs no WebKitGTK dependency. Without the feature the
//! scene still opens — in the system browser — so the action degrades
//! gracefully rather than disappearing.

use std::path::Path;
use std::rc::Rc;

#[cfg(not(feature = "scene3d"))]
use matforge_core::models::ConsoleLevel;

use crate::app_state::AppState;

/// Display the generated scene `html`. Embedded WebView when built with
/// `scene3d`; otherwise the system browser.
pub fn open(app: &Rc<AppState>, html: &Path) {
    #[cfg(feature = "scene3d")]
    embed::open_embedded(app, html);
    #[cfg(not(feature = "scene3d"))]
    open_in_browser(app, html);
}

/// The window title for a given scene file (`3-D Scene — <file>`).
#[cfg(feature = "scene3d")]
fn window_title(html: &Path) -> String {
    let name = html
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "scene".into());
    format!("3-D Scene — {name}")
}

#[cfg(not(feature = "scene3d"))]
fn open_in_browser(app: &Rc<AppState>, html: &Path) {
    use std::process::Command;
    match Command::new("xdg-open").arg(html).spawn() {
        Ok(_) => app
            .vm
            .status_bar
            .set_message("Opened 3-D scene in the system browser"),
        Err(e) => app.vm.console.log(
            ConsoleLevel::Error,
            format!(
                "could not open the 3-D scene ({e}); generated at {}",
                html.display()
            ),
        ),
    }
}

#[cfg(feature = "scene3d")]
mod embed {
    use std::path::Path;
    use std::rc::Rc;

    use gtk::prelude::*;
    use gtk::{Box as GtkBox, Orientation, Window};
    use webkit6::prelude::*;
    use webkit6::WebView;

    use crate::app_state::AppState;

    /// Open a window hosting a `webkit6::WebView` that loads the scene HTML.
    /// Babylon's camera provides orbit/zoom/pan/play inside the view.
    pub fn open_embedded(app: &Rc<AppState>, html: &Path) {
        let window = Window::builder()
            .title(super::window_title(html))
            .default_width(960)
            .default_height(680)
            .build();
        window.add_css_class("mf-root");

        let root = GtkBox::new(Orientation::Vertical, 0);
        root.add_css_class("mf-window");

        let webview = WebView::new();
        webview.set_vexpand(true);
        webview.set_hexpand(true);
        webview.load_uri(&format!("file://{}", html.display()));

        root.append(&webview);
        window.set_child(Some(&root));
        window.present();

        app.vm
            .status_bar
            .set_message(format!("3-D scene: {}", html.display()));
    }
}
