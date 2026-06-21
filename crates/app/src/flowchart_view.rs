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
use gtk::{
    Box as GtkBox, Button, DrawingArea, Entry, Label, Orientation, ScrolledWindow, TextView,
};

use std::cell::Cell;

use matforge_core::models::flowchart::{
    FlowNode, NodeKind, ParamValue, SignalFlowParamSpec, StateDecomposition,
};
use matforge_core::models::ConsoleLevel;
use matforge_core::services::codegen::ExportTarget;
use matforge_core::viewmodels::flowchart::{ZOOM_MAX, ZOOM_MIN};
use matforge_core::viewmodels::FlowchartViewModel;

use crate::app_state::AppState;
use crate::flow_render::{self, Viewport};

/// In-progress canvas gesture.
enum DragMode {
    /// Moving the node under the cursor.
    Move { id: String, px: f64, py: f64 },
    /// Drawing an edge out of a port (rubber band to the cursor).
    Edge {
        from_node: String,
        from_port: String,
    },
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

    // Publish state for end-to-end tests (no-op unless $MATFORGE_E2E_STATE set).
    {
        let fc = fc.clone();
        crate::e2e::set_flowchart_probe(move || {
            serde_json::json!({
                "nodes": fc.node_count(),
                "edges": fc.edge_count(),
                "selected": fc.selected_id.get().is_some(),
                "zoom": fc.zoom.get(),
                "can_undo": fc.can_undo(),
            })
        });
    }

    let palette = build_palette(&fc);

    let canvas = DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);
    // Focusable so the canvas can receive Delete/Backspace key presses.
    canvas.set_focusable(true);

    // Endpoints (world coords) of the edge being dragged, drawn as a rubber band.
    #[allow(clippy::type_complexity)]
    let pending_edge: Rc<RefCell<Option<((f64, f64), (f64, f64))>>> = Rc::new(RefCell::new(None));

    // Draw.
    {
        let fc = fc.clone();
        let pending = pending_edge.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            let vp = Viewport {
                pan: fc.pan.get(),
                zoom: fc.zoom.get(),
            };
            let algebraic = fc.algebraic_loop_nodes();
            let lint: std::collections::BTreeSet<String> =
                fc.action_lint_nodes().into_keys().collect();
            let hierarchy_lint: std::collections::BTreeSet<String> =
                fc.hierarchy_errors().into_keys().collect();
            let flow_index = fc.current_flow_index();
            fc.document.with(|doc| {
                let sel = fc.selected_id.get();
                let sel_edge = fc.selected_edge.get();
                let exec = fc.execution_node.get();
                fc.node_breakpoints.with(|bps| {
                    flow_render::draw_document(
                        ctx,
                        w as f64,
                        h as f64,
                        doc,
                        flow_index,
                        vp,
                        sel.as_deref(),
                        sel_edge.as_deref(),
                        bps,
                        exec.as_deref(),
                        &algebraic,
                        &lint,
                        &hierarchy_lint,
                        &std::collections::BTreeSet::new(),
                    );
                });
            });
            if let Some((s, e)) = *pending.borrow() {
                let to_screen =
                    |p: (f64, f64)| (p.0 * vp.zoom + vp.pan.0, p.1 * vp.zoom + vp.pan.1);
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

    // Navigating in/out of a subsystem swaps the visible flow: refit and redraw.
    {
        let c = canvas.clone();
        let fc2 = fc.clone();
        fc.nav_stack.subscribe(move |_| {
            fit_view(&fc2, c.width() as f64, c.height() as f64);
            c.queue_draw();
        });
    }

    // Redraw when the selected connection changes.
    {
        let c = canvas.clone();
        fc.selected_edge.subscribe(move |_| c.queue_draw());
    }

    // Delete / Backspace removes the current selection (node or connection).
    let keys = gtk::EventControllerKey::new();
    {
        let fc = fc.clone();
        let canvas2 = canvas.clone();
        keys.connect_key_pressed(move |_c, key, _code, _state| {
            if matches!(key, gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace) {
                fc.delete_selected();
                canvas2.queue_draw();
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
    }
    canvas.add_controller(keys);

    // Click to select (clears selection when clicking empty canvas). Left button
    // only, so the middle-button pan gesture below has the diagram to itself.
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
    {
        let fc = fc.clone();
        let canvas2 = canvas.clone();
        click.connect_released(move |_g, _n, x, y| {
            let vp = Viewport {
                pan: fc.pan.get(),
                zoom: fc.zoom.get(),
            };
            let world = flow_render::screen_to_world(vp, x, y);
            let idx = fc.current_flow_index();
            let hit = fc.document.with(|d| flow_render::hit_test(d, idx, world));
            if let Some(id) = hit {
                fc.select(Some(id));
            } else {
                // No node under the cursor: try to pick a connection. The
                // tolerance is ~6px on screen, scaled into world units.
                let tol = 6.0 / vp.zoom.max(0.01);
                let edge = fc
                    .document
                    .with(|d| flow_render::edge_hit_test(d, idx, world, tol));
                match edge {
                    Some(eid) => fc.select_edge(Some(eid)),
                    None => fc.select(None),
                }
            }
            canvas2.grab_focus();
            canvas2.queue_draw();
        });
    }
    canvas.add_controller(click);

    // Double-click a subsystem block to enter its sub-flow.
    let dclick = gtk::GestureClick::new();
    dclick.set_button(gtk::gdk::BUTTON_PRIMARY);
    {
        let fc = fc.clone();
        let canvas2 = canvas.clone();
        dclick.connect_pressed(move |_g, n_press, x, y| {
            if n_press < 2 {
                return;
            }
            let vp = Viewport {
                pan: fc.pan.get(),
                zoom: fc.zoom.get(),
            };
            let world = flow_render::screen_to_world(vp, x, y);
            let idx = fc.current_flow_index();
            if let Some(hit) = fc.document.with(|d| flow_render::hit_test(d, idx, world)) {
                if fc.enter_subflow(&hit) {
                    canvas2.queue_draw();
                } else if fc.node(&hit).map(|n| n.kind) == Some(NodeKind::SignalMatlabFcn) {
                    // Not a subsystem: a MATLAB Function block opens its source.
                    open_matlab_fcn_editor(&fc, &hit, &canvas2);
                }
            }
        });
    }
    canvas.add_controller(dclick);

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

        // Extract the right-clicked block into its own subsystem (signal flow).
        let extract = Button::with_label("Extract to Subsystem");
        extract.set_has_frame(false);
        extract.set_halign(gtk::Align::Start);
        {
            let app = app.clone();
            let fc = fc.clone();
            let target = menu_target.clone();
            let pop = menu_pop.clone();
            extract.connect_clicked(move |_| {
                if let Some(id) = target.borrow().clone() {
                    if fc.extract_to_subsystem(&[id]).is_some() {
                        app.vm.toast.show("Extracted to subsystem");
                    }
                }
                pop.popdown();
            });
        }
        menu.append(&extract);
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
            let vp = Viewport {
                pan: fc.pan.get(),
                zoom: fc.zoom.get(),
            };
            let world = flow_render::screen_to_world(vp, x, y);
            let idx = fc.current_flow_index();
            if let Some(id) = fc.document.with(|d| flow_render::hit_test(d, idx, world)) {
                fc.select(Some(id.clone()));
                *target.borrow_mut() = Some(id);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.popup();
                canvas2.queue_draw();
            } else {
                // Right-clicking a wire opens its signal-breakpoint editor.
                let tol = 6.0 / vp.zoom.max(0.01);
                if let Some(eid) = fc
                    .document
                    .with(|d| flow_render::edge_hit_test(d, idx, world, tol))
                {
                    fc.select_edge(Some(eid.clone()));
                    open_edge_breakpoint_popover(&fc, &eid, &canvas2, x, y);
                    canvas2.queue_draw();
                }
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
            let vp = Viewport {
                pan: fc.pan.get(),
                zoom: fc.zoom.get(),
            };
            let world = flow_render::screen_to_world(vp, x, y);
            let idx = fc.current_flow_index();
            // Port stubs win over the body so you can pull an edge off a node edge.
            let port_radius = 14.0 / fc.zoom.get().max(0.1);
            if let Some((node, port)) = fc
                .document
                .with(|d| flow_render::output_port_hit(d, idx, world, port_radius))
            {
                let start = fc
                    .document
                    .with(|d| flow_render::port_world(d, idx, &node, &port));
                if let Some(start) = start {
                    *pending.borrow_mut() = Some((start, (world.x, world.y)));
                    *state.borrow_mut() = Some(DragMode::Edge {
                        from_node: node,
                        from_port: port,
                    });
                    return;
                }
            }
            if let Some(id) = fc.document.with(|d| flow_render::hit_test(d, idx, world)) {
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
        drag.connect_drag_update(move |g, dx, dy| match &*state.borrow() {
            Some(DragMode::Move { id, px, py }) => {
                let zoom = fc.zoom.get();
                fc.set_node_position(id, px + dx / zoom, py + dy / zoom);
            }
            Some(DragMode::Edge { .. }) => {
                if let Some((start_x, start_y)) = g.start_point() {
                    let vp = Viewport {
                        pan: fc.pan.get(),
                        zoom: fc.zoom.get(),
                    };
                    let world = flow_render::screen_to_world(vp, start_x + dx, start_y + dy);
                    if let Some(p) = pending.borrow_mut().as_mut() {
                        p.1 = (world.x, world.y);
                    }
                    canvas2.queue_draw();
                }
            }
            None => {}
        });
    }
    {
        let fc = fc.clone();
        let state = drag_state.clone();
        let pending = pending_edge.clone();
        let canvas2 = canvas.clone();
        let app = app.clone();
        drag.connect_drag_end(move |g, dx, dy| {
            let taken = state.borrow_mut().take();
            if let Some(DragMode::Move { id, .. }) = &taken {
                reparent_on_drop(&fc, id);
            }
            if let Some(DragMode::Edge {
                from_node,
                from_port,
            }) = taken
            {
                if let Some((start_x, start_y)) = g.start_point() {
                    let vp = Viewport {
                        pan: fc.pan.get(),
                        zoom: fc.zoom.get(),
                    };
                    let world = flow_render::screen_to_world(vp, start_x + dx, start_y + dy);
                    let idx = fc.current_flow_index();
                    let target = fc.document.with(|d| flow_render::hit_test(d, idx, world));
                    if let Some(to_node) = target {
                        let to_port = fc
                            .document
                            .with(|d| flow_render::nearest_input_port(d, idx, &to_node, world));
                        match to_port {
                            Some(to_port)
                                if fc.can_add_edge(&from_node, &from_port, &to_node, &to_port) =>
                            {
                                fc.add_edge(&from_node, &from_port, &to_node, &to_port);
                            }
                            Some(_) if to_node == from_node => {
                                app.vm.toast.show("Can't wire a block to itself");
                            }
                            Some(_) => {
                                app.vm.toast.show("That input port is already connected");
                            }
                            None => {}
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

    // Floating canvas controls: Organize (auto-layout) + Fit, top-right corner.
    let corner = GtkBox::new(Orientation::Horizontal, 6);
    corner.set_halign(gtk::Align::End);
    corner.set_valign(gtk::Align::Start);
    corner.set_margin_top(8);
    corner.set_margin_end(8);

    // Signal-flow lays out left→right (Simulink-style); the rest top→down.
    let horizontal = fc.document.with(|d| d.schema_kind())
        == matforge_core::models::flowchart::SchemaKind::SignalFlow;
    let organize = Button::with_label("Organize");
    organize.add_css_class("mf-tool");
    organize.add_css_class("mf-flow-fit");
    organize.set_tooltip_text(Some("Auto-arrange the diagram into clean layers"));
    {
        let fc = fc.clone();
        let canvas = canvas.clone();
        organize.connect_clicked(move |_| {
            fc.auto_layout(horizontal);
            fit_view(&fc, canvas.width() as f64, canvas.height() as f64);
        });
    }
    corner.append(&organize);

    let fit = Button::with_label("Fit");
    fit.add_css_class("mf-tool");
    fit.add_css_class("mf-flow-fit");
    fit.set_tooltip_text(Some("Zoom to fit the whole chart"));
    {
        let fc = fc.clone();
        let canvas = canvas.clone();
        fit.connect_clicked(move |_| {
            fit_view(&fc, canvas.width() as f64, canvas.height() as f64);
        });
    }
    corner.append(&fit);
    overlay.add_overlay(&corner);

    // A slim toolbar (Save / Compile / Simulate / undo·redo·delete + a Blocks
    // toggle) sits above the palette+canvas row, so the palette is just the
    // block list and can be collapsed to give the diagram the full width.
    let toolbar = build_flow_toolbar(app, &fc, path.clone(), &palette);
    let content = GtkBox::new(Orientation::Horizontal, 0);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.append(&palette);
    content.append(&overlay);

    // Live MATLAB preview pane (hidden until the Preview toggle is on).
    let (preview_pane, preview_toggle) = build_preview_pane(app, &fc, path.clone());
    toolbar.append(&preview_toggle);
    content.append(&preview_pane);

    root.append(&toolbar);
    root.append(&build_breadcrumb(&fc));
    root.append(&content);

    // The block inspector lives in the shared right-side panel: install it when
    // this flowchart tab is shown, remove it when hidden.
    let inspector: gtk::Widget = build_inspector_body(app, &fc).upcast();
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
    vec![
        Hook::Doc,
        Hook::Sel,
        Hook::Zoom,
        Hook::Pan,
        Hook::Bp,
        Hook::Exec,
    ]
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
    crate::e2e::set_flowchart_palette(&list); // drive target for e2e tests
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);
    panel
}

/// The flowchart editor's slim top toolbar: a Blocks-palette toggle, Save /
/// Compile / dialect run action, and undo·redo·delete.
/// Popover form for the signal-flow solver settings (`settings.solver`),
/// anchored on the toolbar's Solver… button. Applying writes one undo step.
fn open_solver_popover(app: &Rc<AppState>, fc: &Rc<FlowchartViewModel>, anchor: &Button) {
    use matforge_core::models::flowchart::{
        AlgebraicLoopMethod as Alg, SolverAlgorithm as A, SolverConfig, SolverType as T,
    };
    let cfg = fc.solver_config();

    let entry = |text: String| {
        let e = Entry::new();
        e.set_text(&text);
        e.set_width_chars(10);
        e
    };
    let numf = |v: Option<f64>, d: f64| entry(format!("{}", v.unwrap_or(d)));

    let type_dd = gtk::DropDown::from_strings(&["fixed_step", "variable_step"]);
    type_dd.set_selected(if cfg.solver_type == Some(T::FixedStep) {
        0
    } else {
        1
    });
    let algo_dd = gtk::DropDown::from_strings(&["ode45", "ode23", "ode15s", "euler", "heun"]);
    algo_dd.set_selected(match cfg.algorithm {
        Some(A::Ode23) => 1,
        Some(A::Ode15s) => 2,
        Some(A::Euler) => 3,
        Some(A::Heun) => 4,
        _ => 0,
    });
    let loop_dd = gtk::DropDown::from_strings(&["trust_region", "newton", "off"]);
    loop_dd.set_selected(match cfg.algebraic_loop_method {
        Some(Alg::Newton) => 1,
        Some(Alg::Off) => 2,
        _ => 0,
    });
    let start = numf(cfg.start_time, 0.0);
    let stop = numf(cfg.stop_time, 10.0);
    let max_step = entry(cfg.max_step.clone().unwrap_or_else(|| "auto".into()));
    let min_step = entry(cfg.min_step.clone().unwrap_or_else(|| "auto".into()));
    let rel_tol = numf(cfg.rel_tol, 1e-3);
    let abs_tol = numf(cfg.abs_tol, 1e-6);
    let zero_crossing = gtk::CheckButton::with_label("Zero-crossing detection");
    zero_crossing.set_active(cfg.zero_crossing.unwrap_or(true));

    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    grid.set_margin_top(10);
    grid.set_margin_bottom(10);
    grid.set_margin_start(10);
    grid.set_margin_end(10);
    let rows: [(&str, &gtk::Widget); 9] = [
        ("Type", type_dd.upcast_ref()),
        ("Algorithm", algo_dd.upcast_ref()),
        ("Start time (s)", start.upcast_ref()),
        ("Stop time (s)", stop.upcast_ref()),
        ("Max step", max_step.upcast_ref()),
        ("Min step", min_step.upcast_ref()),
        ("Relative tol", rel_tol.upcast_ref()),
        ("Absolute tol", abs_tol.upcast_ref()),
        ("Algebraic loop", loop_dd.upcast_ref()),
    ];
    for (r, (label, widget)) in rows.iter().enumerate() {
        let l = Label::new(Some(label));
        l.add_css_class("mf-col-title");
        l.set_halign(gtk::Align::Start);
        grid.attach(&l, 0, r as i32, 1, 1);
        grid.attach(*widget, 1, r as i32, 1, 1);
    }
    grid.attach(&zero_crossing, 0, 9, 2, 1);

    let apply = Button::with_label("Apply");
    apply.add_css_class("mf-compile-cta");
    grid.attach(&apply, 0, 10, 2, 1);

    let pop = gtk::Popover::new();
    pop.set_parent(anchor);
    pop.set_child(Some(&grid));

    {
        let app = app.clone();
        let fc = fc.clone();
        let pop = pop.clone();
        apply.connect_clicked(move |_| {
            let f = |e: &Entry, d: f64| e.text().trim().parse::<f64>().unwrap_or(d);
            let step = |e: &Entry| {
                let s = e.text().to_string();
                Some(if s.trim().is_empty() {
                    "auto".to_string()
                } else {
                    s
                })
            };
            let cfg = SolverConfig {
                solver_type: Some(if type_dd.selected() == 0 {
                    T::FixedStep
                } else {
                    T::VariableStep
                }),
                algorithm: Some(match algo_dd.selected() {
                    1 => A::Ode23,
                    2 => A::Ode15s,
                    3 => A::Euler,
                    4 => A::Heun,
                    _ => A::Ode45,
                }),
                start_time: Some(f(&start, 0.0)),
                stop_time: Some(f(&stop, 10.0)),
                max_step: step(&max_step),
                min_step: step(&min_step),
                rel_tol: Some(f(&rel_tol, 1e-3)),
                abs_tol: Some(f(&abs_tol, 1e-6)),
                zero_crossing: Some(zero_crossing.is_active()),
                algebraic_loop_method: Some(match loop_dd.selected() {
                    1 => Alg::Newton,
                    2 => Alg::Off,
                    _ => Alg::TrustRegion,
                }),
            };
            fc.set_solver_config(cfg);
            app.vm.toast.show("Solver settings updated");
            pop.popdown();
        });
    }
    pop.popup();
}

/// Append one editable symbol row (one `Entry` per placeholder plus a remove
/// button) to `container`, pre-filled from `values` when supplied.
fn add_symbol_row(container: &gtk::Box, placeholders: &[&str], values: &[String]) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for (i, ph) in placeholders.iter().enumerate() {
        let e = Entry::new();
        e.set_placeholder_text(Some(ph));
        e.set_width_chars(if i == 0 { 12 } else { 8 });
        if let Some(v) = values.get(i) {
            e.set_text(v);
        }
        row.append(&e);
    }
    let del = Button::from_icon_name("user-trash-symbolic");
    del.add_css_class("flat");
    del.set_tooltip_text(Some("Remove this symbol"));
    {
        let container = container.clone();
        let row = row.clone();
        del.connect_clicked(move |_| container.remove(&row));
    }
    row.append(&del);
    container.append(&row);
}

/// Read back the symbol rows in `container` as `cols` trimmed fields each,
/// dropping rows whose name (first field) is blank.
fn read_symbol_rows(container: &gtk::Box, cols: usize) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut child = container.first_child();
    while let Some(row) = child {
        let mut fields = Vec::with_capacity(cols);
        let mut c = row.first_child();
        while let Some(w) = c {
            if let Some(e) = w.downcast_ref::<Entry>() {
                fields.push(e.text().trim().to_string());
            }
            c = w.next_sibling();
        }
        if fields.len() >= cols && !fields[0].is_empty() {
            fields.truncate(cols);
            out.push(fields);
        }
        child = row.next_sibling();
    }
    out
}

/// Popover editor for the chart symbol table (`flows[0].symbols`): data,
/// events, and messages. Applying writes one undo step. State-chart only.
fn open_symbols_popover(app: &Rc<AppState>, fc: &Rc<FlowchartViewModel>, anchor: &Button) {
    use matforge_core::models::flowchart::{ChartSymbol, ChartSymbols};
    const DATA_COLS: &[&str] = &["name", "scope", "type", "units", "initial"];
    const EVENT_COLS: &[&str] = &["name", "trigger"];
    const MSG_COLS: &[&str] = &["name"];

    let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
    body.set_margin_top(10);
    body.set_margin_bottom(10);
    body.set_margin_start(10);
    body.set_margin_end(10);

    let make_section = |title: &str, placeholders: &'static [&'static str]| -> gtk::Box {
        let heading = Label::new(Some(title));
        heading.add_css_class("mf-col-title");
        heading.set_halign(gtk::Align::Start);
        heading.set_margin_top(6);
        body.append(&heading);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 4);
        body.append(&rows);
        let add = Button::with_label("+ Add");
        add.add_css_class("flat");
        add.set_halign(gtk::Align::Start);
        let rows2 = rows.clone();
        add.connect_clicked(move |_| add_symbol_row(&rows2, placeholders, &[]));
        body.append(&add);
        rows
    };

    let data_box = make_section("Data", DATA_COLS);
    let event_box = make_section("Events", EVENT_COLS);
    let msg_box = make_section("Messages", MSG_COLS);

    // Pre-fill from the current symbol table.
    let v = |o: &Option<String>| o.clone().unwrap_or_default();
    let syms = fc.chart_symbols();
    for s in syms.data.unwrap_or_default() {
        let row = [
            s.name,
            v(&s.scope),
            v(&s.symbol_type),
            v(&s.units),
            v(&s.initial),
        ];
        add_symbol_row(&data_box, DATA_COLS, &row);
    }
    for s in syms.events.unwrap_or_default() {
        add_symbol_row(&event_box, EVENT_COLS, &[s.name, v(&s.trigger)]);
    }
    for s in syms.messages.unwrap_or_default() {
        add_symbol_row(&msg_box, MSG_COLS, &[s.name]);
    }

    let apply = Button::with_label("Apply");
    apply.add_css_class("mf-compile-cta");
    apply.set_margin_top(10);
    body.append(&apply);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_min_content_height(280);
    scroller.set_min_content_width(360);
    scroller.set_child(Some(&body));

    let pop = gtk::Popover::new();
    pop.set_parent(anchor);
    pop.set_child(Some(&scroller));

    {
        let app = app.clone();
        let fc = fc.clone();
        let pop = pop.clone();
        apply.connect_clicked(move |_| {
            let opt = |s: &str| {
                let t = s.trim();
                (!t.is_empty()).then(|| t.to_string())
            };
            let data: Vec<ChartSymbol> = read_symbol_rows(&data_box, 5)
                .into_iter()
                .map(|r| ChartSymbol {
                    name: r[0].clone(),
                    scope: opt(&r[1]),
                    symbol_type: opt(&r[2]),
                    units: opt(&r[3]),
                    trigger: None,
                    initial: opt(&r[4]),
                })
                .collect();
            let events: Vec<ChartSymbol> = read_symbol_rows(&event_box, 2)
                .into_iter()
                .map(|r| ChartSymbol {
                    name: r[0].clone(),
                    scope: None,
                    symbol_type: None,
                    units: None,
                    trigger: opt(&r[1]),
                    initial: None,
                })
                .collect();
            let messages: Vec<ChartSymbol> = read_symbol_rows(&msg_box, 1)
                .into_iter()
                .map(|r| ChartSymbol {
                    name: r[0].clone(),
                    scope: None,
                    symbol_type: None,
                    units: None,
                    trigger: None,
                    initial: None,
                })
                .collect();
            let syms = ChartSymbols {
                data: (!data.is_empty()).then_some(data),
                events: (!events.is_empty()).then_some(events),
                messages: (!messages.is_empty()).then_some(messages),
            };
            fc.set_chart_symbols(syms);
            app.vm.toast.show("Chart symbols updated");
            pop.popdown();
        });
    }
    pop.popup();
}

/// A navigation breadcrumb shown only inside a subsystem: an **Up** action plus
/// a clickable crumb per level (root → … → current sub-flow). Rebuilds whenever
/// the navigation stack changes.
fn build_breadcrumb(fc: &Rc<FlowchartViewModel>) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 4);
    bar.add_css_class("mf-toolbar");
    bar.set_margin_start(8);
    bar.set_margin_end(8);

    let rebuild: Rc<dyn Fn()> = {
        let fc = fc.clone();
        let bar = bar.clone();
        Rc::new(move || {
            while let Some(c) = bar.first_child() {
                bar.remove(&c);
            }
            let crumbs = fc.breadcrumb();
            if crumbs.len() <= 1 {
                bar.set_visible(false);
                return;
            }
            bar.set_visible(true);
            let up = Button::with_label("⬆ Up");
            up.add_css_class("mf-tool");
            {
                let fc = fc.clone();
                up.connect_clicked(move |_| fc.nav_back());
            }
            bar.append(&up);
            for (i, name) in crumbs.iter().enumerate() {
                if i > 0 {
                    bar.append(&Label::new(Some("›")));
                }
                let crumb = Button::with_label(name);
                crumb.set_has_frame(false);
                crumb.add_css_class("mf-row");
                let fc = fc.clone();
                crumb.connect_clicked(move |_| fc.nav_to_depth(i));
                bar.append(&crumb);
            }
        })
    };
    rebuild();
    {
        let rebuild = rebuild.clone();
        fc.nav_stack.subscribe(move |_| rebuild());
    }
    bar
}

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

    // Open the full Block Library (all blocks, grouped + searchable) in its own
    // window, so the inline palette can stay collapsed for more canvas room.
    let library = Button::from_icon_name("view-grid-symbolic");
    library.add_css_class("mf-header-action");
    library.set_tooltip_text(Some("Block Library — browse all blocks"));
    {
        let app = app.clone();
        let fc = fc.clone();
        library.connect_clicked(move |_| crate::block_library::open(&app, &fc));
    }
    bar.append(&library);

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

    // Export ▾ — the compiler's codegen lanes (-emit-matlab / -dump-chart /
    // -emit-c / -emit-cpp / -emit-llvm / -emit-systemverilog).
    let export_menu = GtkBox::new(Orientation::Vertical, 2);
    export_menu.set_margin_top(4);
    export_menu.set_margin_bottom(4);
    export_menu.set_margin_start(4);
    export_menu.set_margin_end(4);
    let export = gtk::MenuButton::new();
    export.set_label("Export ▾");
    export.add_css_class("mf-tool");
    export.set_tooltip_text(Some("Generate code via matlabc"));
    let export_pop = gtk::Popover::new();
    export_pop.set_child(Some(&export_menu));
    export.set_popover(Some(&export_pop));
    for target in ExportTarget::all() {
        let item = Button::with_label(target.label());
        item.set_has_frame(false);
        item.set_halign(gtk::Align::Start);
        item.add_css_class("mf-row");
        let app = app.clone();
        let fc = fc.clone();
        let path = path.clone();
        let export_pop = export_pop.clone();
        item.connect_clicked(move |_| {
            export_pop.popdown();
            emit_artifact(&app, &fc, path.as_deref(), target);
        });
        export_menu.append(&item);
    }
    bar.append(&export);

    // Signal-flow → Simulate (mflowLink); state-chart → Run Chart (mStateflow).
    let schema = fc.document.with(|d| d.schema_kind());
    use matforge_core::models::flowchart::SchemaKind;
    if schema == SchemaKind::SignalFlow {
        let sim = Button::with_label("▶ Simulate");
        sim.add_css_class("mf-tool");
        sim.add_css_class("mf-run");
        {
            let app = app.clone();
            let fc = fc.clone();
            let path = path.clone();
            sim.connect_clicked(move |_| {
                crate::mflowlink_window::open(&app, fc.document.get(), (*path).clone(), false);
            });
        }
        bar.append(&sim);

        // Solver settings (settings.solver) — drives `matlabc -simulate`.
        let solver = Button::with_label("Solver…");
        solver.add_css_class("mf-tool");
        solver.set_tooltip_text(Some("Configure the simulation solver"));
        {
            let app = app.clone();
            let fc = fc.clone();
            solver.connect_clicked(move |b| open_solver_popover(&app, &fc, b));
        }
        bar.append(&solver);

        // Insert a masked instance of one of the document's `kind: library` flows.
        let library = gtk::MenuButton::new();
        library.set_label("Library ▾");
        library.add_css_class("mf-tool");
        library.set_tooltip_text(Some("Insert a masked library block"));
        let lib_menu = GtkBox::new(Orientation::Vertical, 2);
        lib_menu.set_margin_top(4);
        lib_menu.set_margin_bottom(4);
        lib_menu.set_margin_start(4);
        lib_menu.set_margin_end(4);
        let lib_pop = gtk::Popover::new();
        lib_pop.set_child(Some(&lib_menu));
        library.set_popover(Some(&lib_pop));
        let libs = fc.library_flows();
        if libs.is_empty() {
            let note = Label::new(Some("No library flows in this model"));
            note.add_css_class("mf-text-muted");
            note.set_margin_start(6);
            note.set_margin_end(6);
            lib_menu.append(&note);
        } else {
            for (lib_id, name) in libs {
                let item = Button::with_label(&name);
                item.set_has_frame(false);
                item.set_halign(gtk::Align::Start);
                item.add_css_class("mf-row");
                let app = app.clone();
                let fc = fc.clone();
                let lib_pop = lib_pop.clone();
                item.connect_clicked(move |_| {
                    lib_pop.popdown();
                    if let Some(id) = fc.instantiate_library(&lib_id, 80.0, 80.0) {
                        fc.select(Some(id));
                        app.vm.toast.show("Inserted library block");
                    }
                });
                lib_menu.append(&item);
            }
        }
        bar.append(&library);
    } else if schema == SchemaKind::StateChart {
        let run = Button::with_label("▶ Run Chart");
        run.add_css_class("mf-tool");
        run.add_css_class("mf-run");
        {
            let app = app.clone();
            let fc = fc.clone();
            let path = path.clone();
            run.connect_clicked(move |_| {
                crate::statechart_window::open(&app, fc.document.get(), (*path).clone(), false);
            });
        }
        bar.append(&run);

        // Chart symbol table (flows[0].symbols): data / events / messages.
        let symbols = Button::with_label("Symbols…");
        symbols.add_css_class("mf-tool");
        symbols.set_tooltip_text(Some("Edit the chart's data, events, and messages"));
        {
            let app = app.clone();
            let fc = fc.clone();
            symbols.connect_clicked(move |b| open_symbols_popover(&app, &fc, b));
        }
        bar.append(&symbols);

        // Tabular alternative to drawing transitions on the canvas.
        let table = Button::with_label("Transitions…");
        table.add_css_class("mf-tool");
        table.set_tooltip_text(Some("Edit transitions as a table"));
        {
            let app = app.clone();
            let fc = fc.clone();
            table.connect_clicked(move |_| crate::transition_table::open(&app, &fc));
        }
        bar.append(&table);
    } else {
        // Control-flow charts get a structural visual step: highlight each block
        // in execution order (no value evaluation — runs need the DAP adapter).
        let order: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let idx: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let step = Button::with_label("Step ▸");
        step.add_css_class("mf-tool");
        step.set_tooltip_text(Some("Highlight the next block in execution order"));
        {
            let fc = fc.clone();
            let order = order.clone();
            let idx = idx.clone();
            step.connect_clicked(move |_| {
                if idx.get() == 0 {
                    *order.borrow_mut() = fc.execution_order();
                }
                let next = order.borrow().get(idx.get()).cloned();
                match next {
                    Some(node) => {
                        fc.set_execution_node(Some(node.clone()));
                        fc.select(Some(node));
                        idx.set(idx.get() + 1);
                    }
                    None => {
                        // Past the last block → clear and restart on next click.
                        fc.set_execution_node(None);
                        idx.set(0);
                    }
                }
            });
        }
        bar.append(&step);

        let stop = Button::from_icon_name("media-playback-stop-symbolic");
        stop.add_css_class("mf-header-action");
        stop.set_tooltip_text(Some("Clear the step highlight"));
        {
            let fc = fc.clone();
            let idx = idx.clone();
            stop.connect_clicked(move |_| {
                fc.set_execution_node(None);
                idx.set(0);
            });
        }
        bar.append(&stop);
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
    let idx = fc.current_flow_index();
    let Some((minx, miny, maxx, maxy)) = fc.document.with(|d| flow_render::content_bounds(d, idx))
    else {
        return;
    };
    let bw = (maxx - minx).max(1.0);
    let bh = (maxy - miny).max(1.0);
    let margin = 48.0;
    let zoom = ((cw - 2.0 * margin) / bw)
        .min((ch - 2.0 * margin) / bh)
        .clamp(ZOOM_MIN, ZOOM_MAX);
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
/// Compute Bode / step / Nyquist for a transfer-function block and add them as
/// figures in the Plots panel. Bode magnitude/phase use `log10(ω)` on the x
/// axis so the curve reads as a conventional Bode plot.
fn analyze_block(app: &Rc<AppState>, node: &FlowNode) {
    use matforge_core::models::{PlotFigure, PlotKind};
    use matforge_core::services::control_analysis::TransferFunction;
    let Some(tf) = TransferFunction::from_node(node) else {
        return;
    };
    let name = if node.label.is_empty() {
        node.kind.display_name().to_string()
    } else {
        node.label.clone()
    };

    let fr = tf.frequency_response(0.01, 1000.0, 400);
    let log_w: Vec<f64> = fr.iter().map(|p| p.w.log10()).collect();
    let mag: Vec<f64> = fr.iter().map(|p| p.mag_db).collect();
    let phase: Vec<f64> = fr.iter().map(|p| p.phase_deg).collect();
    let re: Vec<f64> = fr.iter().map(|p| p.re).collect();
    let im: Vec<f64> = fr.iter().map(|p| p.im).collect();
    let (st, sy): (Vec<f64>, Vec<f64>) = tf.step_response(10.0, 0.02).into_iter().unzip();

    let mut idx = app.vm.plots.figures.with(|f| f.len() as i32);
    let mut add = |title: String, xs: Vec<f64>, ys: Vec<f64>| {
        idx += 1;
        app.vm
            .plots
            .add(PlotFigure::series(idx, title, PlotKind::Line2D, xs, ys));
    };
    add(format!("{name} — Step response"), st, sy);
    add(
        format!("{name} — Bode magnitude dB (x=log10 ω)"),
        log_w.clone(),
        mag,
    );
    add(format!("{name} — Bode phase deg (x=log10 ω)"), log_w, phase);
    add(format!("{name} — Nyquist (Re vs Im)"), re, im);
    app.vm.toast.show("Added Bode / step / Nyquist to Plots");
}

/// Hierarchy controls shown in the inspector for a selected state: decomposition
/// (OR/AND), history-junction toggle, execution-order among AND siblings, a
/// detach-to-top-level action, and any hierarchy lint message.
fn append_state_hierarchy_controls(body: &GtkBox, fc: &Rc<FlowchartViewModel>, node: &FlowNode) {
    let id = node.id.clone();

    let titled = |text: &str, child: &gtk::Widget| -> GtkBox {
        let row = GtkBox::new(Orientation::Vertical, 2);
        let lbl = Label::new(Some(text));
        lbl.add_css_class("mf-col-title");
        lbl.set_halign(gtk::Align::Start);
        row.append(&lbl);
        row.append(child);
        row
    };

    // Decomposition (OR exclusive / AND parallel).
    let dd = gtk::DropDown::from_strings(&["OR (exclusive)", "AND (parallel)"]);
    dd.set_selected(u32::from(
        node.data.decomposition == Some(StateDecomposition::And),
    ));
    {
        let fc = fc.clone();
        let id = id.clone();
        dd.connect_selected_notify(move |d| {
            let dec = if d.selected() == 1 {
                StateDecomposition::And
            } else {
                StateDecomposition::Or
            };
            fc.set_decomposition(&id, dec);
        });
    }
    body.append(&titled("Decomposition", dd.upcast_ref()));

    // History junction.
    let hist = gtk::CheckButton::with_label("History junction");
    hist.set_active(node.data.has_history == Some(true));
    {
        let fc = fc.clone();
        let id = id.clone();
        hist.connect_toggled(move |_| fc.toggle_history(&id));
    }
    body.append(&hist);

    // Execution order (used when nested under an AND state).
    let order = Entry::new();
    order.set_text(
        &node
            .data
            .execution_order
            .map(|o| o.to_string())
            .unwrap_or_default(),
    );
    {
        let fc = fc.clone();
        let id = id.clone();
        order.connect_changed(move |e| {
            fc.set_execution_order(&id, e.text().trim().parse::<u32>().ok());
        });
    }
    body.append(&titled(
        "Execution order (AND siblings)",
        order.upcast_ref(),
    ));

    // Parent + detach.
    if node.parent.is_some() {
        let detach = Button::with_label("Move to top level");
        detach.add_css_class("mf-tool");
        let fc2 = fc.clone();
        let id2 = id.clone();
        detach.connect_clicked(move |_| {
            fc2.reparent(&id2, None);
        });
        body.append(&detach);
    }

    // Hierarchy lint for this state.
    if let Some(msg) = fc.hierarchy_errors().get(&id) {
        let warn = Label::new(Some(&format!("⚠ {msg}")));
        warn.add_css_class("mf-field-error-text");
        warn.set_halign(gtk::Align::Start);
        warn.set_wrap(true);
        body.append(&warn);
    }
}

/// After a state is dropped, reparent it into the smallest other state whose box
/// contains its center (or to the top level if dropped on empty canvas). A drop
/// into the state's own descendant is rejected by `reparent` (no cycles).
fn reparent_on_drop(fc: &Rc<FlowchartViewModel>, id: &str) {
    use matforge_core::models::flowchart::SchemaKind;
    if fc.document.with(|d| d.schema_kind()) != SchemaKind::StateChart {
        return;
    }
    let Some(node) = fc.node(id) else { return };
    if node.kind != NodeKind::State {
        return;
    }
    let (x, y, w, h) = node.rect();
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut exclude = fc.descendants(id);
    exclude.insert(id.to_string());

    let idx = fc.current_flow_index();
    let target = fc.document.with(|d| {
        d.flows.get(idx).and_then(|flow| {
            flow.nodes
                .iter()
                .filter(|n| n.kind == NodeKind::State && !exclude.contains(&n.id))
                .filter(|n| {
                    let (nx, ny, nw, nh) = n.rect();
                    cx >= nx && cx <= nx + nw && cy >= ny && cy <= ny + nh
                })
                .min_by(|a, b| {
                    let (ra, rb) = (a.rect(), b.rect());
                    (ra.2 * ra.3)
                        .partial_cmp(&(rb.2 * rb.3))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|n| n.id.clone())
        })
    });
    fc.reparent(id, target.as_deref());
}

/// Inspector section for a masked library instance: one entry per mask
/// parameter plus a live `${name}` substitution preview of the library flow.
fn append_mask_editor(body: &GtkBox, fc: &Rc<FlowchartViewModel>, node: &FlowNode) {
    let id = node.id.clone();

    let heading = Label::new(Some("Mask parameters"));
    heading.add_css_class("mf-col-title");
    heading.set_halign(gtk::Align::Start);
    heading.set_margin_top(6);
    body.append(&heading);

    let preview = Label::new(None);
    preview.add_css_class("mf-text-muted");
    preview.set_halign(gtk::Align::Start);
    preview.set_wrap(true);
    let refresh: Rc<dyn Fn()> = {
        let fc = fc.clone();
        let id = id.clone();
        let preview = preview.clone();
        Rc::new(move || {
            preview.set_text(&fc.mask_preview(&id).unwrap_or_default());
        })
    };

    let params = node.data.mask_params.clone().unwrap_or_default();
    if params.is_empty() {
        let note = Label::new(Some("This library defines no mask parameters."));
        note.add_css_class("mf-text-muted");
        note.set_halign(gtk::Align::Start);
        body.append(&note);
    }
    for (name, value) in &params {
        let row = GtkBox::new(Orientation::Vertical, 2);
        let lbl = Label::new(Some(name));
        lbl.add_css_class("mf-col-title");
        lbl.set_halign(gtk::Align::Start);
        let entry = Entry::new();
        entry.set_text(value);
        entry.set_hexpand(true);
        let fc2 = fc.clone();
        let id2 = id.clone();
        let name2 = name.clone();
        let refresh2 = refresh.clone();
        entry.connect_changed(move |e| {
            fc2.set_mask_param(&id2, &name2, &e.text());
            refresh2();
        });
        row.append(&lbl);
        row.append(&entry);
        body.append(&row);
    }

    let pv_title = Label::new(Some("Expansion preview"));
    pv_title.add_css_class("mf-col-title");
    pv_title.set_halign(gtk::Align::Start);
    pv_title.set_margin_top(6);
    body.append(&pv_title);
    body.append(&preview);
    refresh();
}

/// A small popover (anchored at the right-click point) for setting or clearing
/// the signal-breakpoint condition on a wire. The breakpoint persists on the
/// `FlowEdge` and is installed when the model is simulated.
fn open_edge_breakpoint_popover(
    fc: &Rc<FlowchartViewModel>,
    edge_id: &str,
    canvas: &DrawingArea,
    x: f64,
    y: f64,
) {
    let pop = gtk::Popover::new();
    pop.set_parent(canvas);
    pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    pop.connect_closed(|p| p.unparent());

    let col = GtkBox::new(Orientation::Vertical, 6);
    col.set_margin_top(8);
    col.set_margin_bottom(8);
    col.set_margin_start(8);
    col.set_margin_end(8);
    let lbl = Label::new(Some("Signal breakpoint — pause when:"));
    lbl.add_css_class("mf-col-title");
    lbl.set_halign(gtk::Align::Start);
    col.append(&lbl);

    let entry = Entry::new();
    entry.set_placeholder_text(Some("value > 0   /   abs(value) >= 1"));
    if let Some(c) = fc.edge_breakpoint(edge_id) {
        entry.set_text(&c);
    }
    entry.set_hexpand(true);
    col.append(&entry);

    let row = GtkBox::new(Orientation::Horizontal, 6);
    let set = Button::with_label("Set");
    set.add_css_class("mf-compile-cta");
    let clear = Button::with_label("Clear");
    row.append(&set);
    row.append(&clear);
    col.append(&row);
    pop.set_child(Some(&col));

    {
        let fc = fc.clone();
        let id = edge_id.to_string();
        let canvas = canvas.clone();
        let pop = pop.clone();
        let entry2 = entry.clone();
        set.connect_clicked(move |_| {
            fc.set_edge_breakpoint(&id, Some(entry2.text().to_string()));
            canvas.queue_draw();
            pop.popdown();
        });
    }
    {
        let fc = fc.clone();
        let id = edge_id.to_string();
        let canvas = canvas.clone();
        let pop = pop.clone();
        clear.connect_clicked(move |_| {
            fc.set_edge_breakpoint(&id, None);
            canvas.queue_draw();
            pop.popdown();
        });
    }
    entry.connect_activate({
        let set = set.clone();
        move |_| set.emit_clicked()
    });
    pop.popup();
}

fn build_inspector_body(app: &Rc<AppState>, fc: &Rc<FlowchartViewModel>) -> ScrolledWindow {
    let body = GtkBox::new(Orientation::Vertical, 8);
    body.set_margin_start(10);
    body.set_margin_end(10);
    body.set_margin_top(8);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&body));

    let rebuild = {
        let fc = fc.clone();
        let app = app.clone();
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

            // Algebraic-loop warning for the selected block.
            if fc.algebraic_loop_nodes().contains(&id) {
                let warn = Label::new(Some(
                    "⚠ On an algebraic loop — insert a state block (Integrator / Unit Delay / ZOH) to break direct feedthrough.",
                ));
                warn.add_css_class("mf-field-error-text");
                warn.set_halign(gtk::Align::Start);
                warn.set_wrap(true);
                body.append(&warn);
            }

            let kind = node.kind;
            for (label_text, key) in node_fields(&node) {
                // State-action snippets get a multi-line, MATLAB-highlighted,
                // lint-checked editor rather than a single-line entry.
                if key.is_action() {
                    let initial = field_get(&node, &key);
                    body.append(&build_action_field(&fc, &id, &label_text, key, &initial));
                    continue;
                }
                let field = GtkBox::new(Orientation::Vertical, 2);
                let lbl = Label::new(Some(&label_text));
                lbl.add_css_class("mf-col-title");
                lbl.set_halign(gtk::Align::Start);
                let entry = Entry::new();
                entry.set_text(&field_get(&node, &key));
                entry.set_hexpand(true);
                // Inline validation message, hidden until the value is invalid.
                let err = Label::new(None);
                err.add_css_class("mf-field-error-text");
                err.set_halign(gtk::Align::Start);
                err.set_wrap(true);
                err.set_visible(false);
                // Connect *after* set_text so the initial value is not echoed
                // back through edit_node (which would falsely mark dirty).
                let fc2 = fc.clone();
                let id2 = id.clone();
                let err2 = err.clone();
                entry.connect_changed(move |e| {
                    let value = e.text().to_string();
                    // Signal-flow parameters are validated; an invalid value is
                    // flagged inline and not committed (no silent bad params).
                    if let FieldKey::Param(k) = &key {
                        if let Err(msg) = SignalFlowParamSpec::validate_field(kind, k, &value) {
                            e.add_css_class("mf-field-error");
                            err2.set_text(&msg);
                            err2.set_visible(true);
                            return;
                        }
                        e.remove_css_class("mf-field-error");
                        err2.set_visible(false);
                    }
                    fc2.edit_node(&id2, |n| field_set(n, &key, &value));
                    // A MATLAB Function block's ports follow its source: editing
                    // the expression / body re-derives the u1..uN / out ports.
                    if matches!(&key, FieldKey::Param(k)
                        if k == "function_body" || k == "expression")
                    {
                        fc2.sync_matlab_fcn_ports(&id2);
                    }
                });
                field.append(&lbl);
                field.append(&entry);
                field.append(&err);
                body.append(&field);
            }

            if node.kind == NodeKind::State {
                append_state_hierarchy_controls(&body, &fc, &node);
            }

            // Masked library instance: mask-parameter editors + a live ${name}
            // substitution preview of the underlying library flow.
            if node.data.library_id.is_some() {
                append_mask_editor(&body, &fc, &node);
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

            // Linear analysis for blocks with a transfer function (Transfer
            // Fcn today): plots Bode / step / Nyquist into the Plots panel.
            if matforge_core::services::control_analysis::TransferFunction::from_node(&node)
                .is_some()
            {
                let analyze = Button::with_label("Analyze (Bode / Step / Nyquist)");
                analyze.add_css_class("mf-tool");
                let app2 = app.clone();
                let node2 = node.clone();
                analyze.connect_clicked(move |_| analyze_block(&app2, &node2));
                body.append(&analyze);
            }
        }
    };

    rebuild();
    fc.selected_id.subscribe(move |_| rebuild());
    scroll
}

/// A multi-line, MATLAB-highlighted, lint-checked editor for one state-action
/// snippet (entry / during / exit / on-event). Edits commit to the node through
/// `field_set`; a structural bracket error is shown inline (and re-surfaces on
/// the canvas via [`FlowchartViewModel::action_lint_nodes`]).
fn build_action_field(
    fc: &Rc<FlowchartViewModel>,
    id: &str,
    label_text: &str,
    key: FieldKey,
    initial: &str,
) -> GtkBox {
    use matforge_core::models::flowchart::lint_action;
    use matforge_core::services::highlighter::Language;

    let field = GtkBox::new(Orientation::Vertical, 2);
    let lbl = Label::new(Some(label_text));
    lbl.add_css_class("mf-col-title");
    lbl.set_halign(gtk::Align::Start);

    let view = TextView::new();
    view.set_monospace(true);
    view.set_top_margin(4);
    view.set_bottom_margin(4);
    view.set_left_margin(6);
    view.set_right_margin(6);
    view.add_css_class("mf-action-editor");
    let buffer = view.buffer();
    buffer.set_text(initial);
    crate::highlight::ensure_tags(&buffer);
    crate::highlight::apply(&buffer, Language::Matlab);

    let scroll = ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_min_content_height(54);
    scroll.set_child(Some(&view));

    let err = Label::new(None);
    err.add_css_class("mf-field-error-text");
    err.set_halign(gtk::Align::Start);
    err.set_wrap(true);
    let show_lint = move |err: &Label, text: &str| match lint_action(text) {
        Some(msg) => {
            err.set_text(&format!("⚠ {msg}"));
            err.set_visible(true);
        }
        None => err.set_visible(false),
    };
    show_lint(&err, initial);

    // Connect *after* set_text so the initial value is not echoed back through
    // edit_node (which would falsely mark the document dirty).
    {
        let fc = fc.clone();
        let id = id.to_string();
        let err = err.clone();
        buffer.connect_changed(move |b| {
            let text = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
            crate::highlight::apply(b, Language::Matlab);
            fc.edit_node(&id, |n| field_set(n, &key, &text));
            show_lint(&err, &text);
        });
    }

    field.append(&lbl);
    field.append(&scroll);
    field.append(&err);
    field
}

/// Open a MATLAB-highlighted source editor for a MATLAB Function block.
/// Double-clicking the block shows its `function … = name(u1, …) … end` body;
/// edits write back to `function_body` and re-derive the block's ports so they
/// follow the function signature (`u1..uN` → `out`).
fn open_matlab_fcn_editor(fc: &Rc<FlowchartViewModel>, node_id: &str, canvas: &DrawingArea) {
    use matforge_core::models::flowchart::matlab_fcn_ports;
    use matforge_core::services::highlighter::Language;

    let Some(node) = fc.node(node_id) else {
        return;
    };
    // Show the function body; seed one from the single-line expression if empty.
    let body = node.param_str("function_body").unwrap_or_default();
    let initial = if !body.trim().is_empty() {
        body
    } else {
        let expr = node.param_str("expression").unwrap_or_default();
        let arity = matlab_fcn_ports("", &expr).0.len().max(1);
        let args = (1..=arity)
            .map(|i| format!("u{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        if expr.trim().is_empty() {
            format!("function out = fcn({args})\n  out = u1;\nend\n")
        } else {
            format!(
                "function out = fcn({args})\n  out = {};\nend\n",
                expr.trim()
            )
        }
    };

    let window = gtk::Window::builder()
        .title(format!("MATLAB Function — {node_id}"))
        .default_width(580)
        .default_height(380)
        .build();
    window.add_css_class("mf-root");
    if let Some(parent) = crate::ui::main_window() {
        window.set_transient_for(Some(&parent));
    }
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    let view = TextView::new();
    view.set_monospace(true);
    view.set_top_margin(6);
    view.set_bottom_margin(6);
    view.set_left_margin(8);
    view.set_right_margin(8);
    view.add_css_class("mf-action-editor");
    let buffer = view.buffer();
    buffer.set_text(&initial);
    crate::highlight::ensure_tags(&buffer);
    crate::highlight::apply(&buffer, Language::Matlab);

    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&view));
    root.append(&scroll);

    let status = Label::new(None);
    status.add_css_class("mf-col-title");
    status.set_halign(gtk::Align::Start);
    status.set_margin_start(8);
    status.set_margin_top(4);
    status.set_margin_bottom(6);
    let set_status = |s: &Label, text: &str| {
        let ins = matlab_fcn_ports(text, "").0.join(", ");
        s.set_text(&format!("ports: {ins} → out"));
    };
    set_status(&status, &initial);
    root.append(&status);

    // Commit on every edit: write the body, re-derive ports, redraw the canvas.
    {
        let fc = fc.clone();
        let id = node_id.to_string();
        let canvas = canvas.clone();
        let status = status.clone();
        buffer.connect_changed(move |b| {
            let text = b.text(&b.start_iter(), &b.end_iter(), false).to_string();
            crate::highlight::apply(b, Language::Matlab);
            fc.edit_node(&id, |n| {
                n.data
                    .params
                    .get_or_insert_with(Default::default)
                    .insert("function_body".into(), ParamValue::Str(text.clone()));
            });
            fc.sync_matlab_fcn_ports(&id);
            set_status(&status, &text);
            canvas.queue_draw();
        });
    }

    window.set_child(Some(&root));
    window.present();
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
    OnEvent,
    Param(String),
}

impl FieldKey {
    /// Multi-line MATLAB action snippets (state-chart blocks). These render as
    /// syntax-highlighted text editors with a structural lint, not single-line
    /// entries.
    fn is_action(&self) -> bool {
        matches!(
            self,
            FieldKey::EntryAction
                | FieldKey::DuringAction
                | FieldKey::ExitAction
                | FieldKey::OnEvent
        )
    }
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
            fields.push(s("entry", FieldKey::EntryAction));
            fields.push(s("during", FieldKey::DuringAction));
            fields.push(s("exit", FieldKey::ExitAction));
            fields.push(s("on event  (one EVENT: code per line)", FieldKey::OnEvent));
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
        FieldKey::OnEvent => d
            .on_event_actions
            .as_ref()
            .map(matforge_core::models::flowchart::format_on_event)
            .unwrap_or_default(),
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
        *slot = if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        };
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
        FieldKey::EntryAction => put(&mut node.data.entry_action, value.trim()),
        FieldKey::DuringAction => put(&mut node.data.during_action, value.trim()),
        FieldKey::ExitAction => put(&mut node.data.exit_action, value.trim()),
        FieldKey::OnEvent => {
            let map = matforge_core::models::flowchart::parse_on_event(value);
            node.data.on_event_actions = if map.is_empty() { None } else { Some(map) };
        }
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
        app.vm
            .status_bar
            .set_message("Save As is not wired for unsaved charts yet");
        return;
    };
    match fc.encode() {
        Ok(json) => match std::fs::write(path, json) {
            Ok(()) => app
                .vm
                .status_bar
                .set_message(format!("Saved {}", path.display())),
            Err(e) => app
                .vm
                .console
                .log(ConsoleLevel::Error, format!("save failed: {e}")),
        },
        Err(e) => app
            .vm
            .console
            .log(ConsoleLevel::Error, format!("encode failed: {e}")),
    }
}

/// Save, then lower the chart to MATLAB and open the generated `.m` source.
/// A collapsible pane showing a live `matlabc -emit-matlab` preview of the
/// chart, refreshed (debounced) whenever the document changes while visible.
/// Returns the pane and the toolbar toggle that shows/hides it.
fn build_preview_pane(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Rc<Option<PathBuf>>,
) -> (GtkBox, gtk::ToggleButton) {
    use matforge_core::services::highlighter::Language;

    let pane = GtkBox::new(Orientation::Vertical, 0);
    pane.add_css_class("mf-panel");
    pane.add_css_class("mf-border-left");
    pane.set_size_request(360, -1);
    pane.set_visible(false);
    let header = Label::new(Some("MATLAB PREVIEW"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    pane.append(&header);

    let view = TextView::new();
    view.set_editable(false);
    view.set_monospace(true);
    view.set_left_margin(6);
    view.set_top_margin(4);
    let buffer = view.buffer();
    crate::highlight::ensure_tags(&buffer);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&view));
    pane.append(&scroll);

    let refresh: Rc<dyn Fn()> = {
        let app = app.clone();
        let fc = fc.clone();
        let path = path.clone();
        let buffer = buffer.clone();
        Rc::new(move || {
            let text = match preview_matlab(&app, &fc, path.as_deref()) {
                Ok(src) => src,
                Err(msg) => format!("% {msg}"),
            };
            buffer.set_text(&text);
            crate::highlight::apply(&buffer, Language::Matlab);
        })
    };

    // Debounce: coalesce a burst of edits into a single refresh ~500ms later,
    // and only while the pane is visible (so hidden previews cost nothing).
    let pending: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::new(RefCell::new(None));
    {
        let pane = pane.clone();
        let refresh = refresh.clone();
        let pending = pending.clone();
        fc.document.subscribe(move |_| {
            if !pane.is_visible() {
                return;
            }
            if let Some(id) = pending.borrow_mut().take() {
                id.remove();
            }
            let refresh = refresh.clone();
            let pending2 = pending.clone();
            let id = gtk::glib::timeout_add_local_once(
                std::time::Duration::from_millis(500),
                move || {
                    *pending2.borrow_mut() = None;
                    refresh();
                },
            );
            *pending.borrow_mut() = Some(id);
        });
    }

    let toggle = gtk::ToggleButton::with_label("Preview");
    toggle.add_css_class("mf-tool");
    toggle.set_tooltip_text(Some("Live MATLAB preview (matlabc -emit-matlab)"));
    {
        let pane = pane.clone();
        let refresh = refresh.clone();
        toggle.connect_toggled(move |t| {
            pane.set_visible(t.is_active());
            if t.is_active() {
                refresh();
            }
        });
    }
    (pane, toggle)
}

/// Lower the flowchart to MATLAB via `matlabc -emit-matlab`, open the generated
/// `.m` in an editor tab, and return its path (or `None` if compilation failed).
pub(crate) fn emit_matlab(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Option<&Path>,
) -> Option<PathBuf> {
    emit_artifact(app, fc, path, ExportTarget::Matlab)
}

/// Persist the document to a real `.mflow` matlabc can read (a temp file for
/// unsaved demo charts), returning that path.
fn persist_for_codegen(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Option<&Path>,
) -> Option<PathBuf> {
    let mflow = path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("matforge_demo.mflow"));
    match fc.encode() {
        Ok(json) => match std::fs::write(&mflow, json) {
            Ok(()) => Some(mflow),
            Err(e) => {
                app.vm
                    .console
                    .log(ConsoleLevel::Error, format!("save failed: {e}"));
                None
            }
        },
        Err(e) => {
            app.vm
                .console
                .log(ConsoleLevel::Error, format!("encode failed: {e}"));
            None
        }
    }
}

/// Run a `matlabc` codegen lane on the flowchart, write the artifact beside the
/// model, open it in an editor tab, and return its path (`None` on failure).
pub(crate) fn emit_artifact(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Option<&Path>,
    target: ExportTarget,
) -> Option<PathBuf> {
    let mflow = persist_for_codegen(app, fc, path)?;
    if !app.settings.matlabc_path.exists() {
        app.vm
            .console
            .log(ConsoleLevel::Error, "matlabc not found — cannot export");
        return None;
    }
    app.vm
        .status_bar
        .set_message(format!("Exporting {}…", target.flag()));
    let output = Command::new(&app.settings.matlabc_path)
        .arg(target.flag())
        .arg(&mflow)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let out_path = mflow.with_extension(target.extension());
            match std::fs::write(&out_path, &o.stdout) {
                Ok(()) => {
                    crate::ui::open_file_path(app, &out_path);
                    app.vm
                        .status_bar
                        .set_message(format!("Generated {}", out_path.display()));
                    Some(out_path)
                }
                Err(e) => {
                    app.vm
                        .console
                        .log(ConsoleLevel::Error, format!("write artifact failed: {e}"));
                    None
                }
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            for line in err.lines() {
                app.vm.console.log(ConsoleLevel::Error, line.to_string());
            }
            app.vm
                .status_bar
                .set_message(format!("Export {} failed", target.flag()));
            None
        }
        Err(e) => {
            app.vm
                .console
                .log(ConsoleLevel::Error, format!("matlabc: {e}"));
            None
        }
    }
}

/// Run `matlabc -emit-matlab` and return the generated source as text (for the
/// live preview pane). Does not write or open anything; `Err` carries a short
/// message to show in the pane.
fn preview_matlab(
    app: &Rc<AppState>,
    fc: &Rc<FlowchartViewModel>,
    path: Option<&Path>,
) -> Result<String, String> {
    if !app.settings.matlabc_path.exists() {
        return Err("matlabc not found — set $MATLABC_PATH to preview".to_string());
    }
    let mflow = persist_for_codegen(app, fc, path).ok_or("could not serialize the chart")?;
    match Command::new(&app.settings.matlabc_path)
        .arg("-emit-matlab")
        .arg(&mflow)
        .output()
    {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).into_owned()),
        Err(e) => Err(format!("matlabc: {e}")),
    }
}
