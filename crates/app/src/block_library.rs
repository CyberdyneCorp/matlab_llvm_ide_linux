//! Block Library: a standalone, searchable browser of every block available in
//! the active flowchart's dialect, grouped by category. Clicking a block drops
//! it on the canvas — so the inline palette can stay collapsed for more working
//! area, and the full block set (not just the curated few) is discoverable.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, FlowBox, FlowBoxChild, Label, Orientation, ScrolledWindow, SearchEntry,
    Window,
};

use matforge_core::models::flowchart::library_blocks;
use matforge_core::viewmodels::FlowchartViewModel;

use crate::app_state::AppState;

/// Open the Block Library for `fc`. Stays open so several blocks can be added.
pub fn open(app: &Rc<AppState>, fc: &Rc<FlowchartViewModel>) {
    let schema = fc.document.with(|d| d.schema_kind());
    let groups = library_blocks(schema);

    let window = Window::builder()
        .title("Block Library")
        .default_width(420)
        .default_height(640)
        .build();
    window.add_css_class("mf-root");
    if let Some(parent) = crate::ui::main_window() {
        window.set_transient_for(Some(&parent));
    }

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    let search = SearchEntry::new();
    search.set_placeholder_text(Some("Search blocks…"));
    search.set_margin_top(8);
    search.set_margin_bottom(8);
    search.set_margin_start(8);
    search.set_margin_end(8);
    root.append(&search);

    let content = GtkBox::new(Orientation::Vertical, 10);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.set_margin_bottom(8);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&content));
    root.append(&scroll);
    window.set_child(Some(&root));

    // Build one section per category; remember each block's lowercase name and
    // its FlowBoxChild so the search can hide non-matching blocks + empty sections.
    type Section = (GtkBox, Vec<(String, FlowBoxChild)>);
    let sections: Rc<RefCell<Vec<Section>>> = Rc::new(RefCell::new(Vec::new()));

    for (category, kinds) in groups {
        let section = GtkBox::new(Orientation::Vertical, 4);

        let header = Label::new(None);
        header.set_markup(&format!(
            "<span foreground=\"{}\" weight=\"bold\">{}</span>",
            category.accent().to_css(),
            category.label()
        ));
        header.set_xalign(0.0);
        header.add_css_class("mf-col-title");
        section.append(&header);

        let flow = FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_max_children_per_line(8);
        flow.set_column_spacing(4);
        flow.set_row_spacing(4);
        flow.set_homogeneous(false);

        let mut entries: Vec<(String, FlowBoxChild)> = Vec::new();
        for kind in kinds {
            let btn = Button::with_label(kind.display_name());
            btn.set_has_frame(false);
            btn.add_css_class("mf-block-chip");
            btn.set_tooltip_text(Some(kind.display_name()));
            {
                let app = app.clone();
                let fc = fc.clone();
                btn.connect_clicked(move |_| {
                    let n = fc.node_count() as f64;
                    fc.add_node(kind, 140.0 + (n % 5.0) * 28.0, 90.0 + (n % 12.0) * 26.0);
                    app.vm.toast.show(format!("Added {}", kind.display_name()));
                });
            }
            let child = FlowBoxChild::new();
            child.set_child(Some(&btn));
            flow.append(&child);
            entries.push((kind.display_name().to_ascii_lowercase(), child));
        }
        section.append(&flow);
        content.append(&section);
        sections.borrow_mut().push((section, entries));
    }

    {
        let sections = sections.clone();
        search.connect_search_changed(move |e| {
            let q = e.text().to_ascii_lowercase();
            for (section, entries) in sections.borrow().iter() {
                let mut any = false;
                for (name, child) in entries {
                    let show = q.is_empty() || name.contains(q.trim());
                    child.set_visible(show);
                    any |= show;
                }
                section.set_visible(any);
            }
        });
    }

    let keys = gtk::EventControllerKey::new();
    {
        let window = window.clone();
        keys.connect_key_pressed(move |_c, key, _code, _state| {
            if key == gtk::gdk::Key::Escape {
                window.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    window.add_controller(keys);

    window.present();
    search.grab_focus();
}
