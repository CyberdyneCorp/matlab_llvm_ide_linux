//! Icon helpers. Most toolbar / activity-bar / panel icons come from the
//! system Adwaita symbolic set; the three IDE-specific concepts Adwaita lacks
//! (debug bug, flowchart, HDL chip) are bundled here as small symbolic SVGs,
//! written to a temp dir at startup and registered as an icon search path.

use std::io::Write;

/// Custom icons not present in Adwaita. Baked light-gray so they stay visible
/// on the dark theme even when symbolic recoloring is a no-op.
const CUSTOM: &[(&str, &str)] = &[
    (
        "mf-debug-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><g fill="#c3cad6">
<ellipse cx="8" cy="9" rx="3.1" ry="4"/>
<rect x="7.4" y="2.4" width="1.2" height="2.4" rx="0.6"/>
<rect x="1.8" y="7.1" width="2.6" height="1.1" rx="0.5"/><rect x="11.6" y="7.1" width="2.6" height="1.1" rx="0.5"/>
<rect x="1.9" y="4.2" width="2.8" height="1" rx="0.5" transform="rotate(33 3.3 4.7)"/>
<rect x="11.3" y="4.2" width="2.8" height="1" rx="0.5" transform="rotate(-33 12.7 4.7)"/>
<rect x="1.9" y="10.8" width="2.8" height="1" rx="0.5" transform="rotate(-33 3.3 11.3)"/>
<rect x="11.3" y="10.8" width="2.8" height="1" rx="0.5" transform="rotate(33 12.7 11.3)"/></g></svg>"##,
    ),
    (
        "mf-flowchart-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><g fill="none" stroke="#c3cad6" stroke-width="1.2">
<rect x="3" y="1.4" width="6" height="3.4" rx="0.6"/>
<rect x="7" y="11" width="6.4" height="3.4" rx="0.6"/>
<path d="M6 4.8 V8 H10.2 V11"/></g></svg>"##,
    ),
    (
        "mf-hdl-symbolic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><g fill="none" stroke="#c3cad6" stroke-width="1.2">
<rect x="4.5" y="4.5" width="7" height="7" rx="0.6"/>
<path d="M6.5 4.5V2.5M9.5 4.5V2.5M6.5 13.5v-2M9.5 13.5v-2M4.5 6.5h-2M4.5 9.5h-2M13.5 6.5h-2M13.5 9.5h-2"/></g></svg>"##,
    ),
];

/// Write the custom icons to a temp dir and register it as an icon search path.
pub fn install() {
    let dir = std::env::temp_dir().join("matforge-icons");
    let scalable = dir.join("scalable/actions");
    if std::fs::create_dir_all(&scalable).is_err() {
        return;
    }
    // A minimal index.theme so GTK treats the dir as an icon theme path.
    let _ = std::fs::write(
        dir.join("index.theme"),
        "[Icon Theme]\nName=MatForge\nDirectories=scalable/actions\n\n[scalable/actions]\nSize=16\nType=Scalable\n",
    );
    for (name, svg) in CUSTOM {
        if let Ok(mut f) = std::fs::File::create(scalable.join(format!("{name}.svg"))) {
            let _ = f.write_all(svg.as_bytes());
        }
    }
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

/// Resolved icon name for each toolbar/activity concept (Adwaita where it
/// exists, our custom names otherwise).
pub mod name {
    pub const NEW: &str = "document-new-symbolic";
    pub const OPEN: &str = "folder-open-symbolic";
    pub const SAVE: &str = "document-save-symbolic";
    pub const RUN: &str = "media-playback-start-symbolic";
    pub const STOP: &str = "media-playback-stop-symbolic";
    pub const COMPILE: &str = "applications-engineering-symbolic";
    pub const DEBUG: &str = "mf-debug-symbolic";
    pub const LAYOUTS: &str = "view-grid-symbolic";
    pub const HELP: &str = "help-about-symbolic";

    // Debug transport
    pub const CONTINUE: &str = "media-playback-start-symbolic";
    pub const PAUSE: &str = "media-playback-pause-symbolic";
    pub const STEP_OVER: &str = "media-seek-forward-symbolic";
    pub const STEP_IN: &str = "go-down-symbolic";
    pub const STEP_OUT: &str = "go-up-symbolic";
    pub const STEP_BACK: &str = "media-seek-backward-symbolic";

    pub const EXPLORER: &str = "folder-symbolic";
    pub const SEARCH: &str = "system-search-symbolic";
    pub const HDL: &str = "mf-hdl-symbolic";
    pub const DOCS: &str = "accessories-dictionary-symbolic";
    pub const FLOWCHART: &str = "mf-flowchart-symbolic";

    pub const REFRESH: &str = "view-refresh-symbolic";
    pub const CLOSE: &str = "window-close-symbolic";
    pub const ADD: &str = "list-add-symbolic";
    pub const TRASH: &str = "user-trash-symbolic";
    pub const CLEAR: &str = "edit-clear-all-symbolic";
    pub const FOLDER: &str = "folder-symbolic";
    pub const FILE: &str = "text-x-generic-symbolic";
}
