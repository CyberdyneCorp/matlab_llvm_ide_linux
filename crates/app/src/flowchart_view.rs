//! Interactive flowchart editor surface: a palette of dialect-appropriate node
//! kinds, a Cairo canvas that renders the document and handles
//! select / drag / zoom / add / connect, and a property inspector that edits the
//! selected block. Save writes the `.mflow`; Compile lowers it to MATLAB via
//! `matlabc -emit-matlab` and opens the generated source. All editing goes
//! through the tested `FlowchartViewModel`; this is GTK + Cairo glue.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DrawingArea, Entry, Label, Orientation, ScrolledWindow};

use std::cell::Cell;

use matforge_core::models::flowchart::{FlowNode, NodeKind, ParamValue, SignalFlowParamSpec};
use matforge_core::models::ConsoleLevel;
use matforge_core::viewmodels::flowchart::{ZOOM_MAX, ZOOM_MIN};
use matforge_core::viewmodels::FlowchartViewModel;

use crate::app_state::AppState;
use crate::flow_render::{self, Viewport};

/// In-progress canvas gesture.
enum DragMode {
    /// Moving the node under the cursor.
    Move { id: String, px: f64, py: f64 },
    /// Drawing an edge out of a port (rubber band to the cursor).
    Edge { from_node: String, from_port: String },
}

/// Build the palette + canvas + inspector surface for a flowchart tab. `path` is
/// the backing `.mflow` (None for unsaved demo charts).
pub fn build_flowchart_view(
    app: &Rc<AppState>,
    fc: Rc<FlowchartViewModel>,
    path: Option<PathBuf>,
) -> GtkBox {
    let path = Rc::new(path);
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.set_hexpand(true);
    root.set_vexpand(true);

    let palette = build_palette(&fc);

    let canvas = DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);

    // Endpoints (world coords) of the edge being dragged, drawn as a rubber band.
    let pending_edge: Rc<RefCell<Option<((f64, f64), (f64, f64))>>> = Rc::new(RefCell::new(None));

    // Draw.
    {
        let fc = fc.clone();
        let pending = pending_edge.clone();
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
            if let Some((s, e)) = *pending.borrow() {
                let to_screen = |p: (f64, f64)| (p.0 * vp.zoom + vp.pan.0, p.1 * vp.zoom + vp.pan.1);
                let (sx, sy) = to_screen(s);
                let (ex, ey) = to_screen(e);
                ctx.set_source_rgba(0.31, 0.64, 0.89, 0.9);
                ctx.set_line_width(1.6);
                ctx.set_dash(&[5.0, 4.0], 0.0);
                ctx.move_to(sx, sy);
                ctx.line_to(ex, ey);
                ctx.stroke().ok();
                ctx.set_dash(&[], 0.0);
            }
        });
    }

    // Redraw on any state change.
    for queue in redraw_hooks() {
        let c = canvas.clone();
        match queue {
            Hook::Doc => {
                fc.document.subscribe(move |_| c.queue_draw());
            }
            Hook::Sel => {
                fc.selected_id.subscribe(move |_| c.queue_draw());
            }
            Hook::Zoom => {
                fc.zoom.subscribe(move |_| c.queue_draw());
            }
            Hook::Pan => {
                fc.pan.subscribe(move |_| c.queue_draw());
            }
            Hook::Bp => {
                fc.node_breakpoints.subscribe(move |_| c.queue_draw());
            }
            Hook::Exec => {
                fc.execution_node.subscribe(move |_| c.queue_draw());
            }
        }
    }

    // Click to select (clears selection when clicking empty canvas). Left button
    // only, so the middle-button pan gesture below has the diagram to itself.
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
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

    // Right-click a node → context menu: toggle a breakpoint or delete the block.
    let menu_pop = gtk::Popover::new();
    menu_pop.set_parent(&canvas);
    menu_pop.set_has_arrow(true);
    let menu_target: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    {
        let menu = GtkBox::new(Orientation::Vertical, 1);
        let bp = Button::with_label("Toggle Breakpoint");
        bp.set_has_frame(false);
        bp.set_halign(gtk::Align::Start);
        {
            let app = app.clone();
            let fc = fc.clone();
            let target = menu_target.clone();
            let pop = menu_pop.clone();
            bp.connect_clicked(move |_| {
                if let Some(id) = target.borrow().clone() {
                    if fc.node(&id).is_some_and(|n| n.kind.is_executable()) {
                        fc.toggle_breakpoint(&id);
                    } else {
                        app.vm.toast.show("Breakpoints apply to executable blocks");
                    }
                }
                pop.popdown();
            });
        }
        menu.append(&bp);
        let del = Button::with_label("Delete Block");
        del.set_has_frame(false);
        del.set_halign(gtk::Align::Start);
        {
            let fc = fc.clone();
            let target = menu_target.clone();
            let pop = menu_pop.clone();
            del.connect_clicked(move |_| {
                if let Some(id) = target.borrow().clone() {
                    fc.select(Some(id));
                    fc.delete_selected();
                }
                pop.popdown();
            });
        }
        menu.append(&del);
        menu_pop.set_child(Some(&menu));
    }
    let rclick = gtk::GestureClick::new();
    rclick.set_button(gtk::gdk::BUTTON_SECONDARY);
    {
        let fc = fc.clone();
        let canvas2 = canvas.clone();
        let target = menu_target.clone();
        let pop = menu_pop.clone();
        rclick.connect_pressed(move |_g, _n, x, y| {
            let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
            let world = flow_render::screen_to_world(vp, x, y);
            if let Some(id) = fc.document.with(|d| flow_render::hit_test(d, world)) {
                fc.select(Some(id.clone()));
                *target.borrow_mut() = Some(id);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.popup();
                canvas2.queue_draw();
            }
        });
    }
    canvas.add_controller(rclick);

    // Drag: from a port → draw an edge; from a body → move the node. Left button
    // only — the middle button pans.
    let drag = gtk::GestureDrag::new();
    drag.set_button(gtk::gdk::BUTTON_PRIMARY);
    let drag_state: Rc<RefCell<Option<DragMode>>> = Rc::new(RefCell::new(None));
    {
        let fc = fc.clone();
        let state = drag_state.clone();
        let pending = pending_edge.clone();
        drag.connect_drag_begin(move |_g, x, y| {
            let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
            let world = flow_render::screen_to_world(vp, x, y);
            // Port stubs win over the body so you can pull an edge off a node edge.
            let port_radius = 14.0 / fc.zoom.get().max(0.1);
            if let Some((node, port)) =
                fc.document.with(|d| flow_render::output_port_hit(d, world, port_radius))
            {
                let start = fc.document.with(|d| flow_render::port_world(d, &node, &port));
                if let Some(start) = start {
                    *pending.borrow_mut() = Some((start, (world.x, world.y)));
                    *state.borrow_mut() = Some(DragMode::Edge { from_node: node, from_port: port });
                    return;
                }
            }
            if let Some(id) = fc.document.with(|d| flow_render::hit_test(d, world)) {
                let pos = fc.node(&id).map(|n| (n.ui.position.x, n.ui.position.y));
                if let Some((px, py)) = pos {
                    fc.select(Some(id.clone()));
                    fc.begin_edit();
                    *state.borrow_mut() = Some(DragMode::Move { id, px, py });
                }
            }
        });
    }
    {
        let fc = fc.clone();
        let state = drag_state.clone();
        let pending = pending_edge.clone();
        let canvas2 = canvas.clone();
        drag.connect_drag_update(move |g, dx, dy| {
            match &*state.borrow() {
                Some(DragMode::Move { id, px, py }) => {
                    let zoom = fc.zoom.get();
                    fc.set_node_position(id, px + dx / zoom, py + dy / zoom);
                }
                Some(DragMode::Edge { .. }) => {
                    if let Some((start_x, start_y)) = g.start_point() {
                        let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
                        let world = flow_render::screen_to_world(vp, start_x + dx, start_y + dy);
                        if let Some(p) = pending.borrow_mut().as_mut() {
                            p.1 = (world.x, world.y);
                        }
                        canvas2.queue_draw();
                    }
                }
                None => {}
            }
        });
    }
    {
        let fc = fc.clone();
        let state = drag_state.clone();
        let pending = pending_edge.clone();
        let canvas2 = canvas.clone();
        drag.connect_drag_end(move |g, dx, dy| {
            if let Some(DragMode::Edge { from_node, from_port }) = state.borrow_mut().take() {
                if let Some((start_x, start_y)) = g.start_point() {
                    let vp = Viewport { pan: fc.pan.get(), zoom: fc.zoom.get() };
                    let world = flow_render::screen_to_world(vp, start_x + dx, start_y + dy);
                    let target = fc.document.with(|d| flow_render::hit_test(d, world));
                    if let Some(to_node) = target {
                        if to_node != from_node {
                            if let Some(to_port) =
                                fc.document.with(|d| flow_render::nearest_input_port(d, &to_node, world))
                            {
                                fc.add_edge(&from_node, &from_port, &to_node, &to_port);
                            }
                        }
                    }
                }
            }
            *pending.borrow_mut() = None;
            *state.borrow_mut() = None;
            canvas2.queue_draw();
        });
    }
    canvas.add_controller(drag);

    // Middle-button drag pans the canvas (offset from the pan at drag start).
    let pan = gtk::GestureDrag::new();
    pan.set_button(gtk::gdk::BUTTON_MIDDLE);
    let pan_origin: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));
    {
        let fc = fc.clone();
        let pan_origin = pan_origin.clone();
        pan.connect_drag_begin(move |_g, _x, _y| pan_origin.set(fc.pan.get()));
    }
    {
        let fc = fc.clone();
        let pan_origin = pan_origin.clone();
        pan.connect_drag_update(move |_g, dx, dy| {
            let (ox, oy) = pan_origin.get();
            fc.set_pan(ox + dx, oy + dy);
        });
    }
    canvas.add_controller(pan);

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

    // Zoom-to-fit the chart the first time the canvas gets a real size, so the
    // content is always on-screen on open (there is no separate pan gesture).
    {
        let fc = fc.clone();
        let did_fit = Cell::new(false);
        canvas.connect_resize(move |_c, w, h| {
            if !did_fit.get() && w > 20 && h > 20 {
                did_fit.set(true);
                fit_view(&fc, w as f64, h as f64);
            }
        });
    }

    // The canvas fills the editor; a small "Fit" button floats in its corner
    // (the old inspector column that held it has moved to the right-side panel).
    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    overlay.set_child(Some(&canvas));
    let fit = Button::with_label("Fit");
    fit.add_css_class("mf-tool");
    fit.add_css_class("mf-flow-fit");
    fit.set_tooltip_text(Some("Zoom to fit the whole chart"));
    fit.set_halign(gtk::Align::End);
    fit.set_valign(gtk::Align::Start);
    fit.set_margin_top(8);
    fit.set_margin_end(8);
    {
        let fc = fc.clone();
        let canvas = canvas.clone();
        fit.connect_clicked(move |_| {
            fit_view(&fc, canvas.width() as f64, canvas.height() as f64);
        });
    }
    overlay.add_overlay(&fit);

    // A slim toolbar (Save / Compile / Simulate / undo·redo·delete + a Blocks
    // toggle) sits above the palette+canvas row, so the palette is just the
    // block list and can be collapsed to give the diagram the full width.
    let toolbar = build_flow_toolbar(app, &fc, path.clone(), &palette);
    let content = GtkBox::new(Orientation::Horizontal, 0);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.append(&palette);
    content.append(&overlay);
    root.append(&toolbar);
    root.append(&content);

    // The block inspector lives in the shared right-side panel: install it when
    // this flowchart tab is shown, remove it when hidden.
    let inspector: gtk::Widget = build_inspector_body(&fc).upcast();
    {
        let inspector = inspector.clone();
        let fc = fc.clone();
        let path = path.clone();
        root.connect_map(move |_| {
            crate::ui::flow_inspector_show(&inspector);
            // Make this the Run/Debug target while its tab is visible.
            crate::ui::set_active_flowchart(&fc, (*path).clone());
        });
    }
    {
        let inspector = inspector.clone();
        let fc = fc.clone();
        root.connect_unmap(move |_| {
            crate::ui::flow_inspector_hide(&inspector);
            crate::ui::clear_active_flowchart(&fc);
        });
    }
    // Selecting a block surfaces the inspector tab.
    {
        let sel = fc.selected_id.clone();
        sel.subscribe(move |id| {
            if id.is_some() {
                crate::ui::flow_inspector_focus();
            }
        });
    }
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

fn redraw_hooks() -> Vec<Hook> {
    vec![Hook::Doc, Hook::Sel, Hook::Zoom, Hook::Pan, Hook::Bp, Hook::Exec]
}

/// The collapsible BLOCKS palette: a list of dialect-appropriate node kinds that
/// drop a new block onto the canvas when clicked.
fn build_palette(fc: &Rc<FlowchartViewModel>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 4);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-right");
    panel.set_size_request(132, -1);

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
    panel
}

/// The flowchart editor's slim top toolbar: a Blocks-palette toggle, Save /
/// Compile / dialect run action, and undo·redo·delete.
fn build_flow_toolbar(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Rc<Option<PathBuf>>,
    palette: &GtkBox,
) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 4);
    bar.add_css_class("mf-flow-toolbar");
    bar.add_css_class("mf-border-bottom");

    // Collapse / show the BLOCKS palette to give the diagram the full width.
    // The visibility lives in the layout view model so it persists across runs
    // and stays in sync across all open flowchart tabs.
    let toggle = gtk::ToggleButton::new();
    toggle.set_icon_name("view-list-symbolic");
    toggle.add_css_class("mf-header-action");
    toggle.set_tooltip_text(Some("Show / hide the blocks palette"));
    let visible = app.vm.layout.flow_palette_visible.get();
    toggle.set_active(visible);
    palette.set_visible(visible);
    {
        let app = app.clone();
        toggle.connect_toggled(move |t| app.vm.layout.flow_palette_visible.set(t.is_active()));
    }
    {
        let palette = palette.clone();
        let toggle = toggle.clone();
        app.vm.layout.flow_palette_visible.subscribe(move |v| {
            palette.set_visible(*v);
            toggle.set_active(*v);
        });
    }
    bar.append(&toggle);

    let save = Button::with_label("Save");
    save.add_css_class("mf-tool");
    {
        let app = app.clone();
        let fc = fc.clone();
        let path = path.clone();
        save.connect_clicked(move |_| save_flowchart(&app, &fc, path.as_deref()));
    }
    bar.append(&save);

    let compile = Button::with_label("Compile");
    compile.add_css_class("mf-compile-cta");
    compile.set_tooltip_text(Some("Lower to MATLAB (matlabc -emit-matlab)"));
    {
        let app = app.clone();
        let fc = fc.clone();
        let path = path.clone();
        compile.connect_clicked(move |_| {
            emit_matlab(&app, &fc, path.as_deref());
        });
    }
    bar.append(&compile);

    // Signal-flow → Simulate (mflowLink); state-chart → Run Chart (mStateflow).
    let schema = fc.document.with(|d| d.schema_kind());
    use matforge_core::models::flowchart::SchemaKind;
    if schema == SchemaKind::SignalFlow {
        let sim = Button::with_label("▶ Simulate");
        sim.add_css_class("mf-tool");
        sim.add_css_class("mf-run");
        let app = app.clone();
        let fc = fc.clone();
        let path = path.clone();
        sim.connect_clicked(move |_| {
            crate::mflowlink_window::open(&app, fc.document.get(), (*path).clone(), false);
        });
        bar.append(&sim);
    } else if schema == SchemaKind::StateChart {
        let run = Button::with_label("▶ Run Chart");
        run.add_css_class("mf-tool");
        run.add_css_class("mf-run");
        let app = app.clone();
        let fc = fc.clone();
        let path = path.clone();
        run.connect_clicked(move |_| {
            crate::statechart_window::open(&app, fc.document.get(), (*path).clone(), false);
        });
        bar.append(&run);
    }

    // Push undo/redo/delete to the right.
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    let undo = Button::from_icon_name("edit-undo-symbolic");
    undo.add_css_class("mf-header-action");
    undo.set_tooltip_text(Some("Undo"));
    {
        let fc = fc.clone();
        undo.connect_clicked(move |_| fc.undo());
    }
    let redo = Button::from_icon_name("edit-redo-symbolic");
    redo.add_css_class("mf-header-action");
    redo.set_tooltip_text(Some("Redo"));
    {
        let fc = fc.clone();
        redo.connect_clicked(move |_| fc.redo());
    }
    let del = Button::from_icon_name(crate::icons::name::TRASH);
    del.add_css_class("mf-header-action");
    del.set_tooltip_text(Some("Delete selected block"));
    {
        let fc = fc.clone();
        del.connect_clicked(move |_| fc.delete_selected());
    }
    bar.append(&undo);
    bar.append(&redo);
    bar.append(&del);
    bar
}

/// Center the chart in the canvas at a zoom that fits all nodes.
fn fit_view(fc: &Rc<FlowchartViewModel>, cw: f64, ch: f64) {
    let Some((minx, miny, maxx, maxy)) = fc.document.with(flow_render::content_bounds) else {
        return;
    };
    let bw = (maxx - minx).max(1.0);
    let bh = (maxy - miny).max(1.0);
    let margin = 48.0;
    let zoom = ((cw - 2.0 * margin) / bw).min((ch - 2.0 * margin) / bh).clamp(ZOOM_MIN, ZOOM_MAX);
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    fc.set_zoom(zoom);
    fc.set_pan(cw / 2.0 - cx * zoom, ch / 2.0 - cy * zoom);
}

/// Right-hand property inspector. Rebuilds its fields whenever the selection
/// changes; field edits flow straight into `fc.edit_node`.
/// Build the block-property editor body. It lives in the shared right-side
/// BLOCK INSPECTOR tab (installed/removed as the flowchart tab is shown/hidden),
/// so the diagram canvas keeps the full editor width.
fn build_inspector_body(fc: &Rc<FlowchartViewModel>) -> ScrolledWindow {
    let body = GtkBox::new(Orientation::Vertical, 8);
    body.set_margin_start(10);
    body.set_margin_end(10);
    body.set_margin_top(8);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&body));

    let rebuild = {
        let fc = fc.clone();
        let body = body.clone();
        move || {
            while let Some(child) = body.first_child() {
                body.remove(&child);
            }
            let Some(id) = fc.selected_id.get() else {
                let empty = Label::new(Some("No block selected"));
                empty.add_css_class("mf-text-muted");
                empty.set_halign(gtk::Align::Start);
                body.append(&empty);
                return;
            };
            let Some(node) = fc.node(&id) else { return };

            let title = Label::new(Some(node.kind.display_name()));
            title.add_css_class("mf-empty-title");
            title.set_halign(gtk::Align::Start);
            body.append(&title);

            for (label_text, key) in node_fields(&node) {
                let field = GtkBox::new(Orientation::Vertical, 2);
                let lbl = Label::new(Some(&label_text));
                lbl.add_css_class("mf-col-title");
                lbl.set_halign(gtk::Align::Start);
                let entry = Entry::new();
                entry.set_text(&field_get(&node, &key));
                entry.set_hexpand(true);
                // Connect *after* set_text so the initial value is not echoed
                // back through edit_node (which would falsely mark dirty).
                let fc2 = fc.clone();
                let id2 = id.clone();
                entry.connect_changed(move |e| {
                    let value = e.text().to_string();
                    fc2.edit_node(&id2, |n| field_set(n, &key, &value));
                });
                field.append(&lbl);
                field.append(&entry);
                body.append(&field);
            }

            if node.kind.is_executable() {
                let bp = Button::with_label("Toggle breakpoint");
                bp.add_css_class("mf-tool");
                let fc2 = fc.clone();
                let id2 = id.clone();
                bp.connect_clicked(move |_| {
                    fc2.toggle_breakpoint(&id2);
                });
                body.append(&bp);
            }
        }
    };

    rebuild();
    fc.selected_id.subscribe(move |_| rebuild());
    scroll
}

/// Which `FlowNode` field an inspector entry edits.
#[derive(Clone)]
enum FieldKey {
    Label,
    Name,
    Value,
    Expression,
    Prompt,
    Lhs,
    Rhs,
    Callee,
    Args,
    Cond,
    LoopVar,
    Iter,
    Text,
    EntryAction,
    DuringAction,
    ExitAction,
    Param(String),
}

/// The editable fields for a node, in display order (Label first).
fn node_fields(node: &FlowNode) -> Vec<(String, FieldKey)> {
    use NodeKind::*;
    let s = |t: &str, k: FieldKey| (t.to_string(), k);
    let mut fields = vec![s("Label", FieldKey::Label)];
    match node.kind {
        Constant | Variable => {
            fields.push(s("Name", FieldKey::Name));
            fields.push(s("Value", FieldKey::Value));
        }
        Assignment => {
            fields.push(s("Target", FieldKey::Lhs));
            fields.push(s("Expression", FieldKey::Rhs));
        }
        Expression | Display => fields.push(s("Expression", FieldKey::Expression)),
        Input => {
            fields.push(s("Prompt", FieldKey::Prompt));
            fields.push(s("Variable", FieldKey::Name));
        }
        FunctionCall | SubflowCall => {
            fields.push(s("Function", FieldKey::Callee));
            fields.push(s("Arguments", FieldKey::Args));
        }
        IfBlock | WhileLoop => fields.push(s("Condition", FieldKey::Cond)),
        ForLoop => {
            fields.push(s("Loop variable", FieldKey::LoopVar));
            fields.push(s("Iterable", FieldKey::Iter));
        }
        Comment => fields.push(s("Text", FieldKey::Text)),
        MatrixLiteral => fields.push(s("Value", FieldKey::Value)),
        State => {
            fields.push(s("Name", FieldKey::Name));
            fields.push(s("Entry action", FieldKey::EntryAction));
            fields.push(s("During action", FieldKey::DuringAction));
            fields.push(s("Exit action", FieldKey::ExitAction));
        }
        kind if kind.is_signal_flow() => {
            for spec in SignalFlowParamSpec::fields(kind) {
                fields.push(s(spec.label, FieldKey::Param(spec.key.to_string())));
            }
        }
        _ => {}
    }
    fields
}

fn field_get(node: &FlowNode, key: &FieldKey) -> String {
    let d = &node.data;
    let opt = |o: &Option<String>| o.clone().unwrap_or_default();
    match key {
        FieldKey::Label => node.label.clone(),
        FieldKey::Name => opt(&d.name),
        FieldKey::Value => opt(&d.value),
        FieldKey::Expression => opt(&d.expression),
        FieldKey::Prompt => opt(&d.prompt),
        FieldKey::Lhs => opt(&d.lhs),
        FieldKey::Rhs => opt(&d.rhs),
        FieldKey::Callee => opt(&d.callee),
        FieldKey::Args => opt(&d.args),
        FieldKey::Cond => opt(&d.cond),
        FieldKey::LoopVar => opt(&d.loop_var),
        FieldKey::Iter => opt(&d.iter),
        FieldKey::Text => opt(&d.text),
        FieldKey::EntryAction => opt(&d.entry_action),
        FieldKey::DuringAction => opt(&d.during_action),
        FieldKey::ExitAction => opt(&d.exit_action),
        FieldKey::Param(k) => d
            .params
            .as_ref()
            .and_then(|m| m.get(k.as_str()))
            .map(|v| v.display_string())
            .unwrap_or_default(),
    }
}

fn field_set(node: &mut FlowNode, key: &FieldKey, value: &str) {
    fn put(slot: &mut Option<String>, value: &str) {
        *slot = if value.is_empty() { None } else { Some(value.to_string()) };
    }
    match key {
        FieldKey::Label => node.label = value.to_string(),
        FieldKey::Name => put(&mut node.data.name, value),
        FieldKey::Value => put(&mut node.data.value, value),
        FieldKey::Expression => put(&mut node.data.expression, value),
        FieldKey::Prompt => put(&mut node.data.prompt, value),
        FieldKey::Lhs => put(&mut node.data.lhs, value),
        FieldKey::Rhs => put(&mut node.data.rhs, value),
        FieldKey::Callee => put(&mut node.data.callee, value),
        FieldKey::Args => put(&mut node.data.args, value),
        FieldKey::Cond => put(&mut node.data.cond, value),
        FieldKey::LoopVar => put(&mut node.data.loop_var, value),
        FieldKey::Iter => put(&mut node.data.iter, value),
        FieldKey::Text => put(&mut node.data.text, value),
        FieldKey::EntryAction => put(&mut node.data.entry_action, value),
        FieldKey::DuringAction => put(&mut node.data.during_action, value),
        FieldKey::ExitAction => put(&mut node.data.exit_action, value),
        FieldKey::Param(k) => {
            let map = node.data.params.get_or_insert_with(BTreeMap::new);
            if value.is_empty() {
                map.remove(k.as_str());
            } else {
                map.insert(k.clone(), ParamValue::parse(value));
            }
        }
    }
}

/// Write the document to its `.mflow` path.
fn save_flowchart(app: &Rc<AppState>, fc: &Rc<FlowchartViewModel>, path: Option<&Path>) {
    let Some(path) = path else {
        app.vm.status_bar.set_message("Save As is not wired for unsaved charts yet");
        return;
    };
    match fc.encode() {
        Ok(json) => match std::fs::write(path, json) {
            Ok(()) => app.vm.status_bar.set_message(format!("Saved {}", path.display())),
            Err(e) => app.vm.console.log(ConsoleLevel::Error, format!("save failed: {e}")),
        },
        Err(e) => app.vm.console.log(ConsoleLevel::Error, format!("encode failed: {e}")),
    }
}

/// Save, then lower the chart to MATLAB and open the generated `.m` source.
/// Lower the flowchart to MATLAB via `matlabc -emit-matlab`, open the generated
/// `.m` in an editor tab, and return its path (or `None` if compilation failed).
pub(crate) fn emit_matlab(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Option<&Path>,
) -> Option<PathBuf> {
    // Persist to a real file matlabc can read (a temp file for demo charts).
    let owned;
    let mflow: &Path = match path {
        Some(p) => p,
        None => {
            owned = std::env::temp_dir().join("matforge_demo.mflow");
            &owned
        }
    };
    match fc.encode() {
        Ok(json) => {
            if let Err(e) = std::fs::write(mflow, json) {
                app.vm.console.log(ConsoleLevel::Error, format!("save failed: {e}"));
                return None;
            }
        }
        Err(e) => {
            app.vm.console.log(ConsoleLevel::Error, format!("encode failed: {e}"));
            return None;
        }
    }

    if !app.settings.matlabc_path.exists() {
        app.vm.console.log(ConsoleLevel::Error, "matlabc not found — cannot compile flowchart");
        return None;
    }
    app.vm.status_bar.set_message("Compiling flowchart…");
    let output = Command::new(&app.settings.matlabc_path)
        .arg("-emit-matlab")
        .arg(mflow)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let code = String::from_utf8_lossy(&o.stdout);
            let m_path = mflow.with_extension("m");
            match std::fs::write(&m_path, code.as_bytes()) {
                Ok(()) => {
                    crate::ui::open_file_path(app, &m_path);
                    app.vm.status_bar.set_message(format!("Generated {}", m_path.display()));
                    Some(m_path)
                }
                Err(e) => {
                    app.vm.console.log(ConsoleLevel::Error, format!("write .m failed: {e}"));
                    None
                }
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            for line in err.lines() {
                app.vm.console.log(ConsoleLevel::Error, line.to_string());
            }
            app.vm.status_bar.set_message("Flowchart compile failed");
            None
        }
        Err(e) => {
            app.vm.console.log(ConsoleLevel::Error, format!("matlabc: {e}"));
            None
        }
    }
}
