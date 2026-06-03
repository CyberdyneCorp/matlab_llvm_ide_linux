//! The Preferences dialog: Appearance (theme / accent / font) and Toolchain
//! tabs, wired live to `MainViewModel.appearance`. Changing any control bumps the
//! appearance revision, which re-renders the CSS instantly — so the whole IDE
//! re-themes while the dialog is open. Thin GTK; all state lives in the VM.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    ApplicationWindow, Box as GtkBox, Button, DropDown, Entry, Label, Notebook, Orientation, Scale,
    Window,
};

use matforge_core::theme::{Accent, ThemeId};
use matforge_core::viewmodels::appearance::{FONT_SCALE_MAX, FONT_SCALE_MIN};

use crate::app_state::AppState;

/// Open the Preferences dialog as a modal window over `parent`.
pub fn open(app: &Rc<AppState>, parent: Option<&ApplicationWindow>) {
    let win = Window::builder().title("Preferences").default_width(440).default_height(360).modal(true).build();
    win.add_css_class("mf-root");
    if let Some(p) = parent {
        win.set_transient_for(Some(p));
    }

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");
    let nb = Notebook::new();
    nb.set_vexpand(true);
    nb.append_page(&appearance_tab(app), Some(&Label::new(Some("Appearance"))));
    nb.append_page(&toolchain_tab(app), Some(&Label::new(Some("Toolchain"))));
    root.append(&nb);

    let close = Button::with_label("Close");
    close.add_css_class("mf-tool");
    close.set_halign(gtk::Align::End);
    close.set_margin_top(8);
    close.set_margin_end(10);
    close.set_margin_bottom(8);
    {
        let win = win.clone();
        close.connect_clicked(move |_| win.close());
    }
    root.append(&close);

    win.set_child(Some(&root));
    win.present();
}

fn appearance_tab(app: &Rc<AppState>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 10);
    v.set_margin_top(14);
    v.set_margin_bottom(14);
    v.set_margin_start(16);
    v.set_margin_end(16);

    // Theme.
    let theme_dd = DropDown::from_strings(&ThemeId::ALL.iter().map(|t| t.label()).collect::<Vec<_>>());
    theme_dd.set_selected(ThemeId::ALL.iter().position(|t| *t == app.vm.appearance.theme_id.get()).unwrap_or(0) as u32);
    {
        let app = app.clone();
        theme_dd.connect_selected_notify(move |dd| {
            app.vm.appearance.set_theme(ThemeId::ALL[dd.selected() as usize]);
        });
    }
    {
        let dd = theme_dd.clone();
        app.vm.appearance.theme_id.bind(move |id| {
            if let Some(i) = ThemeId::ALL.iter().position(|t| t == id) {
                dd.set_selected(i as u32);
            }
        });
    }
    v.append(&field_row("Theme", &theme_dd));

    // Accent.
    let accent_dd = DropDown::from_strings(&Accent::ALL.iter().map(|a| a.label()).collect::<Vec<_>>());
    accent_dd.set_selected(Accent::ALL.iter().position(|a| *a == app.vm.appearance.accent.get()).unwrap_or(0) as u32);
    {
        let app = app.clone();
        accent_dd.connect_selected_notify(move |dd| {
            app.vm.appearance.set_accent(Accent::ALL[dd.selected() as usize]);
        });
    }
    {
        let dd = accent_dd.clone();
        app.vm.appearance.accent.bind(move |a| {
            if let Some(i) = Accent::ALL.iter().position(|x| x == a) {
                dd.set_selected(i as u32);
            }
        });
    }
    v.append(&field_row("Accent", &accent_dd));

    // Independent UI + editor font-size sliders.
    {
        let app = app.clone();
        v.append(&scale_row(
            "UI font size",
            app.vm.appearance.font_scale.clone(),
            move |s| app.vm.appearance.set_font_scale(s),
        ));
    }
    {
        let app = app.clone();
        v.append(&scale_row(
            "Editor font size",
            app.vm.appearance.code_font_scale.clone(),
            move |s| app.vm.appearance.set_code_font_scale(s),
        ));
    }

    // Code font family.
    let entry = Entry::new();
    entry.set_text(&app.vm.appearance.code_font_family.get());
    entry.set_hexpand(true);
    {
        let app = app.clone();
        entry.connect_changed(move |e| app.vm.appearance.set_code_font(e.text().to_string()));
    }
    v.append(&field_row("Code font", &entry));

    let hint = Label::new(Some("Zoom anywhere with Ctrl + =, Ctrl + −, Ctrl + 0."));
    hint.add_css_class("mf-text-muted");
    hint.set_halign(gtk::Align::Start);
    hint.set_margin_top(6);
    v.append(&hint);

    v
}

fn toolchain_tab(app: &Rc<AppState>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 10);
    v.set_margin_top(14);
    v.set_margin_bottom(14);
    v.set_margin_start(16);
    v.set_margin_end(16);
    v.append(&path_row("matlabc", &app.settings.matlabc_path));
    v.append(&path_row("libMatlabRuntime.a", &app.settings.runtime_archive));
    let note = Label::new(Some("Set $MATLABC_PATH or edit ~/.config/matforge/config.toml to change these."));
    note.add_css_class("mf-text-muted");
    note.set_halign(gtk::Align::Start);
    note.set_wrap(true);
    note.set_margin_top(6);
    v.append(&note);
    v
}

/// A labelled font-scale slider with a live % readout, two-way bound to `prop`.
fn scale_row(
    label: &str,
    prop: matforge_core::observable::Property<f64>,
    on_change: impl Fn(f64) -> f64 + 'static,
) -> GtkBox {
    let scale = Scale::with_range(Orientation::Horizontal, FONT_SCALE_MIN, FONT_SCALE_MAX, 0.05);
    scale.set_value(prop.get());
    scale.set_hexpand(true);
    scale.set_draw_value(false);
    let pct = Label::new(None);
    pct.add_css_class("mf-mono");
    pct.set_width_chars(5);
    let set_pct = {
        let pct = pct.clone();
        move |s: f64| pct.set_text(&format!("{}%", (s * 100.0).round() as i64))
    };
    set_pct(prop.get());
    {
        let set_pct = set_pct.clone();
        scale.connect_value_changed(move |s| set_pct(on_change(s.value())));
    }
    {
        let scale = scale.clone();
        prop.bind(move |s| {
            if (scale.value() - *s).abs() > 1e-6 {
                scale.set_value(*s);
            }
            set_pct(*s);
        });
    }
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.append(&row_label(label));
    row.append(&scale);
    row.append(&pct);
    row
}

fn field_row(label: &str, control: &impl IsA<gtk::Widget>) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.append(&row_label(label));
    control.set_hexpand(true);
    row.append(control);
    row
}

fn row_label(text: &str) -> Label {
    let l = Label::new(Some(text));
    l.set_halign(gtk::Align::Start);
    l.set_width_chars(14);
    l.set_xalign(0.0);
    l
}

fn path_row(name: &str, path: &std::path::Path) -> GtkBox {
    let row = GtkBox::new(Orientation::Vertical, 0);
    let ok = path.exists();
    let head = Label::new(Some(&format!("{} {name}", if ok { "✓" } else { "✗" })));
    head.set_halign(gtk::Align::Start);
    head.add_css_class(if ok { "mf-badge-ok" } else { "mf-badge-fail" });
    let p = Label::new(Some(&path.display().to_string()));
    p.add_css_class("mf-text-muted");
    p.add_css_class("mf-mono");
    p.set_halign(gtk::Align::Start);
    p.set_selectable(true);
    row.append(&head);
    row.append(&p);
    row
}
