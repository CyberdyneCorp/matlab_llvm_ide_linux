//! Interactive flowchart editor surface: a palette of dialect-appropriate node
//! kinds beside a Cairo canvas that renders the document and handles
//! select / drag / zoom / add. All editing goes through the tested
//! `FlowchartViewModel`; this is GTK + Cairo glue.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DrawingArea, Label, Orientation, ScrolledWindow};

use matforge_core::viewmodels::FlowchartViewModel;

use crate::flow_render::{self, Viewport};

/// Build the palette + canvas surface for a flowchart tab.
pub fn build_flowchart_view(fc: Rc<FlowchartViewModel>) -> GtkBox {
    let root = GtkBox::new(Orientation::Horizontal, 0);
    root.set_hexpand(true);
    root.set_vexpand(true);

    root.append(&build_palette(&fc));

    let canvas = DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);

    // Draw.
    {
        let fc = fc.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
            fc.document.with(|doc| {
                let sel = fc.selected_id.get();
                let exec = fc.execution_node.get();
                fc.node_breakpoints.with(|bps| {
                    flow_render::draw_document(
                        ctx,
                        w as f64,
                        h as f64,
                        doc,
                        vp,
                        sel.as_deref(),
                        bps,
                        exec.as_deref(),
                    );
                });
            });
        });
    }

    // Redraw on any state change.
    for queue in redraw_hooks(&canvas) {
        match queue {
            Hook::Doc => {
                let c = canvas.clone();
                fc.document.subscribe(move |_| c.queue_draw());
            }
            Hook::Sel => {
                let c = canvas.clone();
                fc.selected_id.subscribe(move |_| c.queue_draw());
            }
            Hook::Zoom => {
                let c = canvas.clone();
                fc.zoom.subscribe(move |_| c.queue_draw());
            }
            Hook::Pan => {
                let c = canvas.clone();
                fc.pan.subscribe(move |_| c.queue_draw());
            }
            Hook::Bp => {
                let c = canvas.clone();
                fc.node_breakpoints.subscribe(move |_| c.queue_draw());
            }
            Hook::Exec => {
                let c = canvas.clone();
                fc.execution_node.subscribe(move |_| c.queue_draw());
            }
        }
    }

    // Click to select.
    let click = gtk::GestureClick::new();
    {
        let fc = fc.clone();
        let canvas2 = canvas.clone();
        click.connect_released(move |_g, _n, x, y| {
            let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
            let world = flow_render::screen_to_world(vp, x, y);
            let hit = fc.document.with(|d| flow_render::hit_test(d, world));
            fc.select(hit);
            canvas2.queue_draw();
        });
    }
    canvas.add_controller(click);

    // Drag to move the node under the cursor.
    let drag = gtk::GestureDrag::new();
    let drag_state: Rc<RefCell<Option<(String, f64, f64)>>> = Rc::new(RefCell::new(None));
    {
        let fc = fc.clone();
        let state = drag_state.clone();
        drag.connect_drag_begin(move |_g, x, y| {
            let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
            let world = flow_render::screen_to_world(vp, x, y);
            if let Some(id) = fc.document.with(|d| flow_render::hit_test(d, world)) {
                let pos = fc.document.with(|d| {
                    d.flows
                        .first()
                        .and_then(|f| f.nodes.iter().find(|n| n.id == id))
                        .map(|n| (n.ui.position.x, n.ui.position.y))
                });
                if let Some((px, py)) = pos {
                    fc.select(Some(id.clone()));
                    fc.begin_edit();
                    *state.borrow_mut() = Some((id, px, py));
                }
            }
        });
    }
    {
        let fc = fc.clone();
        let state = drag_state.clone();
        drag.connect_drag_update(move |_g, dx, dy| {
            if let Some((id, px, py)) = state.borrow().clone() {
                let zoom = fc.zoom.get();
                fc.set_node_position(&id, px + dx / zoom, py + dy / zoom);
            }
        });
    }
    {
        let state = drag_state.clone();
        drag.connect_drag_end(move |_g, _dx, _dy| {
            *state.borrow_mut() = None;
        });
    }
    canvas.add_controller(drag);

    // Scroll to zoom.
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    {
        let fc = fc.clone();
        scroll.connect_scroll(move |_c, _dx, dy| {
            let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
            fc.set_zoom(fc.zoom.get() * factor);
            gtk::glib::Propagation::Stop
        });
    }
    canvas.add_controller(scroll);

    root.append(&canvas);
    root
}

enum Hook {
    Doc,
    Sel,
    Zoom,
    Pan,
    Bp,
    Exec,
}

fn redraw_hooks(_canvas: &DrawingArea) -> Vec<Hook> {
    vec![Hook::Doc, Hook::Sel, Hook::Zoom, Hook::Pan, Hook::Bp, Hook::Exec]
}

fn build_palette(fc: &Rc<FlowchartViewModel>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-right");
    panel.set_size_request(150, -1);

    let header = Label::new(Some("BLOCKS"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    panel.append(&header);

    let list = GtkBox::new(Orientation::Vertical, 2);
    let kinds = fc.document.with(flow_render::palette_kinds);
    for kind in kinds {
        let btn = Button::with_label(kind.display_name());
        btn.set_has_frame(false);
        btn.set_halign(gtk::Align::Start);
        btn.add_css_class("mf-row");
        let fc = fc.clone();
        btn.connect_clicked(move |_| {
            // Drop new nodes in a cascading position near the top-left.
            let n = fc.node_count() as f64;
            fc.add_node(kind, 120.0 + (n % 4.0) * 30.0, 80.0 + n * 24.0);
        });
        list.append(&btn);
    }
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    // Undo / redo row.
    let actions = GtkBox::new(Orientation::Horizontal, 4);
    actions.set_margin_start(6);
    let undo = Button::with_label("↶");
    undo.set_tooltip_text(Some("undo"));
    {
        let fc = fc.clone();
        undo.connect_clicked(move |_| fc.undo());
    }
    let redo = Button::with_label("↷");
    redo.set_tooltip_text(Some("redo"));
    {
        let fc = fc.clone();
        redo.connect_clicked(move |_| fc.redo());
    }
    actions.append(&undo);
    actions.append(&redo);
    panel.append(&actions);

    panel
}
