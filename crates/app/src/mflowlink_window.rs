//! Standalone mflowLink window: a signal-flow model canvas beside live scope
//! tiles, driven by `matlabc -simulate`. The tested `MflowLinkViewModel` holds
//! the trace + transport state; this is GTK + Cairo glue plus the subprocess.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DrawingArea, Label, Orientation, ScrolledWindow, Window};

use serde_json::json;

use matforge_core::models::flowchart::FlowchartDocument;
use matforge_core::models::{PlotFigure, PlotKind};
use matforge_core::services::dap::{parse_message, DapMessage};
use matforge_core::services::sim_dap::{self, SimRequest};
use matforge_core::viewmodels::{MflowLinkViewModel, SimState};

use crate::app_state::AppState;
use crate::flow_render::{self, Viewport};
use crate::process::{DapSession, SimHandle};

/// A live `matlabc -simulate --sim-dap` session, when the transport is in live
/// mode (vs the one-shot CSV path).
type LiveSession = Rc<RefCell<Option<DapSession>>>;

/// Open a simulation window for a signal-flow document. `autostart` immediately
/// runs the simulation (used by the `MATFORGE_SIMULATE` demo hook).
pub fn open(
    app: &Rc<AppState>,
    document: FlowchartDocument,
    path: Option<PathBuf>,
    autostart: bool,
) {
    let vm = Rc::new(MflowLinkViewModel::new(document));
    let sim: Rc<RefCell<Option<SimHandle>>> = Rc::new(RefCell::new(None));
    let dap: LiveSession = Rc::new(RefCell::new(None));

    // Publish state for end-to-end tests (no-op unless $MATFORGE_E2E_STATE set).
    {
        let vm = vm.clone();
        crate::e2e::set_mflowlink_probe(move || {
            serde_json::json!({
                "state": format!("{:?}", vm.state.get()),
                "samples": vm.sample_count.get(),
                "signals": vm.signal_count(),
            })
        });
    }

    let window = Window::builder()
        .title(format!(
            "mflowLink — {}",
            path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled".into())
        ))
        .default_width(1100)
        .default_height(680)
        .build();
    window.add_css_class("mf-root");

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");
    root.append(&build_transport(app, &vm, &sim, &dap, path.clone()));

    let split = gtk::Paned::new(Orientation::Horizontal);
    split.set_wide_handle(true);
    split.set_vexpand(true);
    split.set_start_child(Some(&build_model_canvas(&vm)));
    split.set_end_child(Some(&build_scopes(app, &vm, path.clone())));
    split.set_position(560);
    root.append(&split);

    window.set_child(Some(&root));

    // Kill the simulation if the window is closed.
    {
        let sim = sim.clone();
        let dap = dap.clone();
        let vm = vm.clone();
        window.connect_close_request(move |_| {
            *sim.borrow_mut() = None; // drops SimHandle -> kills process
            *dap.borrow_mut() = None; // drops the live --sim-dap session
            vm.reset();
            crate::e2e::clear_mflowlink_probe();
            glib_proceed()
        });
    }
    window.present();

    if autostart {
        start_simulation(app, &vm, &sim, path.as_deref());
    }
}

fn glib_proceed() -> gtk::glib::Propagation {
    gtk::glib::Propagation::Proceed
}

fn build_transport(
    app: &Rc<AppState>,
    vm: &Rc<MflowLinkViewModel>,
    sim: &Rc<RefCell<Option<SimHandle>>>,
    dap: &LiveSession,
    path: Option<PathBuf>,
) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 6);
    bar.add_css_class("mf-toolbar");
    bar.set_margin_top(4);
    bar.set_margin_bottom(4);
    bar.set_margin_start(8);
    bar.set_margin_end(8);

    let play = Button::with_label("▶ Play");
    play.add_css_class("mf-tool");
    play.add_css_class("mf-run");
    let pause = Button::with_label("⏸ Pause");
    pause.add_css_class("mf-tool");
    let step = Button::with_label("⏭ Step");
    step.add_css_class("mf-tool");
    let back = Button::with_label("⏮ Back");
    back.add_css_class("mf-tool");
    back.set_tooltip_text(Some("Step back one major step (live --sim-dap)"));
    let stop = Button::with_label("⏹ Stop");
    stop.add_css_class("mf-tool");
    stop.add_css_class("mf-stop");
    let reset = Button::with_label("⟲ Restart");
    reset.add_css_class("mf-tool");

    // Playback timer that scrubs the cursor through a finished trace.
    let timer: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::new(RefCell::new(None));
    let stop_timer = {
        let timer = timer.clone();
        move || {
            if let Some(id) = timer.borrow_mut().take() {
                id.remove();
            }
        }
    };
    // Model path the sim-DAP adapter keys signal-breakpoint edge ids against.
    let source: String = path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model.mflow".to_string());

    {
        let app = app.clone();
        let vm = vm.clone();
        let dap = dap.clone();
        let timer = timer.clone();
        play.connect_clicked(move |_| match vm.state.get() {
            // First Play boots a live --sim-dap session (paused at entry).
            SimState::Idle => start_live_simulation(&app, &vm, &dap, path.as_deref()),
            // Paused live session → resume free-running.
            SimState::Paused if vm.live.get() => {
                send_sim(&dap, &SimRequest::Continue);
                vm.resume();
            }
            // Already running live → ignore.
            _ if vm.live.get() => {}
            // One-shot CSV replay: animate the cursor through the trace.
            _ => {
                if vm.at_end() {
                    vm.rewind();
                }
                if timer.borrow().is_none() {
                    let vm2 = vm.clone();
                    let timer2 = timer.clone();
                    let id = gtk::glib::timeout_add_local(
                        std::time::Duration::from_millis(33),
                        move || {
                            if vm2.at_end() {
                                *timer2.borrow_mut() = None;
                                gtk::glib::ControlFlow::Break
                            } else {
                                vm2.step();
                                gtk::glib::ControlFlow::Continue
                            }
                        },
                    );
                    *timer.borrow_mut() = Some(id);
                }
            }
        });
    }
    {
        let vm = vm.clone();
        let dap = dap.clone();
        let stop_timer = stop_timer.clone();
        pause.connect_clicked(move |_| {
            stop_timer();
            if vm.live.get() {
                send_sim(&dap, &SimRequest::Pause);
            } else {
                vm.pause();
            }
        });
    }
    {
        let vm = vm.clone();
        let dap = dap.clone();
        let stop_timer = stop_timer.clone();
        step.connect_clicked(move |_| {
            stop_timer();
            if vm.live.get() {
                send_sim(&dap, &SimRequest::StepMajor);
            } else {
                vm.step();
            }
        });
    }
    {
        let vm = vm.clone();
        let dap = dap.clone();
        back.connect_clicked(move |_| {
            if vm.live.get() {
                send_sim(&dap, &SimRequest::StepBackMajor);
            }
        });
    }
    {
        let vm = vm.clone();
        let sim = sim.clone();
        let dap = dap.clone();
        let stop_timer = stop_timer.clone();
        stop.connect_clicked(move |_| {
            stop_timer();
            *sim.borrow_mut() = None; // kill the one-shot simulator
            *dap.borrow_mut() = None; // kill the live session
            vm.finish();
        });
    }
    {
        let vm = vm.clone();
        let sim = sim.clone();
        let dap = dap.clone();
        let stop_timer = stop_timer.clone();
        reset.connect_clicked(move |_| {
            stop_timer();
            if vm.live.get() {
                send_sim(&dap, &SimRequest::ResetSimulation);
            } else {
                *sim.borrow_mut() = None;
                vm.finish(); // stop collecting, then rewind playback to the start
                vm.rewind();
            }
        });
    }
    let bp = Button::with_label("Breakpoints…");
    bp.add_css_class("mf-tool");
    bp.set_tooltip_text(Some("Signal / time breakpoints + live solver tuning"));
    {
        let vm = vm.clone();
        let dap = dap.clone();
        bp.connect_clicked(move |b| open_breakpoints_popover(&vm, &dap, &source, b));
    }
    bar.append(&play);
    bar.append(&pause);
    bar.append(&step);
    bar.append(&back);
    bar.append(&stop);
    bar.append(&reset);
    bar.append(&bp);

    let status = Label::new(Some("idle"));
    status.add_css_class("mf-text-secondary");
    status.set_margin_start(12);
    bar.append(&status);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    let pos = Label::new(Some("0 / 0"));
    pos.add_css_class("mf-mono");
    pos.set_margin_end(10);
    bar.append(&pos);
    let clock = Label::new(Some("t = 0.000 s"));
    clock.add_css_class("mf-mono");
    bar.append(&clock);
    let snaps = Label::new(Some(""));
    snaps.add_css_class("mf-mono");
    snaps.set_margin_start(10);
    bar.append(&snaps);

    {
        let status = status.clone();
        vm.state.bind(move |s| {
            status.set_text(match s {
                SimState::Idle => "idle",
                SimState::Running => "running…",
                SimState::Paused => "paused",
                SimState::Finished => "finished",
            });
        });
    }
    // Surface why the live run paused (breakpoint / step / crossing / entry).
    {
        let status = status.clone();
        vm.stop_reason.bind(move |r| {
            if let Some(reason) = r {
                status.set_text(&format!("paused — {reason}"));
            }
        });
    }
    // Snapshot-ring depth from snapshotTaken events.
    {
        let snaps = snaps.clone();
        vm.snapshots.bind(move |s| {
            let text = if s.is_empty() {
                String::new()
            } else {
                format!("⛁ {} snapshots", s.len())
            };
            snaps.set_text(&text);
        });
    }
    // Clock + position follow the playback cursor.
    {
        let vm = vm.clone();
        let clock = clock.clone();
        let pos = pos.clone();
        let cur = vm.cursor.clone();
        cur.bind(move |c| {
            let total = vm.total_samples();
            pos.set_text(&format!("{c} / {total}"));
            let t = vm
                .trace
                .with(|tr| tr.time().get(c.saturating_sub(1)).copied())
                .unwrap_or(0.0);
            clock.set_text(&format!("t = {t:.3} s"));
        });
    }
    // In live mode the clock + step counter follow the streamed simulation time.
    {
        let clock = clock.clone();
        let pos = pos.clone();
        let vm2 = vm.clone();
        vm.sim_time.bind(move |t| {
            if vm2.live.get() {
                clock.set_text(&format!("t = {t:.3} s"));
                pos.set_text(&format!("step {}", vm2.major_step.get()));
            }
        });
    }

    bar
}

/// Send a generic DAP request (handshake verbs) to the live session.
fn send_dap(dap: &LiveSession, command: &str, args: Option<serde_json::Value>) {
    if let Some(s) = dap.borrow_mut().as_mut() {
        let frame = s.client.request(command, args);
        let _ = s.write_frame(&frame);
    }
}

/// Send a simulation-control request to the live session.
fn send_sim(dap: &LiveSession, req: &SimRequest) {
    if let Some(s) = dap.borrow_mut().as_mut() {
        let frame = sim_dap::build_request(&mut s.client, req);
        let _ = s.write_frame(&frame);
    }
}

/// Popover to set time / signal breakpoints and tune the solver live. Applying
/// sends the corresponding `--sim-dap` requests to the running session.
fn open_breakpoints_popover(
    vm: &Rc<MflowLinkViewModel>,
    dap: &LiveSession,
    source: &str,
    anchor: &Button,
) {
    use matforge_core::services::sim_dap::{SignalBreakpoint, TimeBreakpoint};

    let entry = |placeholder: &str| {
        let e = gtk::Entry::new();
        e.set_placeholder_text(Some(placeholder));
        e.set_width_chars(16);
        e
    };
    let label = |t: &str| {
        let l = Label::new(Some(t));
        l.add_css_class("mf-col-title");
        l.set_halign(gtk::Align::Start);
        l
    };

    let times = entry("2.5, 5, 7.5");
    let cond = entry("abs(value) > 1e3");
    let rtol = entry("relTol e.g. 1e-4");
    let mstep = entry("maxStep e.g. 0.01");
    let edges: Vec<String> = vm.document.with(|d| {
        d.flows
            .first()
            .map(|f| f.edges.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default()
    });
    let edge_dd =
        gtk::DropDown::from_strings(&edges.iter().map(String::as_str).collect::<Vec<_>>());

    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    grid.set_margin_top(10);
    grid.set_margin_bottom(10);
    grid.set_margin_start(10);
    grid.set_margin_end(10);
    let rows: [(&str, &gtk::Widget); 5] = [
        ("Time bps (s)", times.upcast_ref()),
        ("Signal edge", edge_dd.upcast_ref()),
        ("Condition", cond.upcast_ref()),
        ("relTol", rtol.upcast_ref()),
        ("maxStep", mstep.upcast_ref()),
    ];
    for (r, (text, widget)) in rows.iter().enumerate() {
        grid.attach(&label(text), 0, r as i32, 1, 1);
        grid.attach(*widget, 1, r as i32, 1, 1);
    }
    let apply = Button::with_label("Apply");
    apply.add_css_class("mf-compile-cta");
    grid.attach(&apply, 0, 5, 2, 1);

    let pop = gtk::Popover::new();
    pop.set_parent(anchor);
    pop.set_child(Some(&grid));

    {
        let dap = dap.clone();
        let pop = pop.clone();
        let source = source.to_string();
        apply.connect_clicked(move |_| {
            // Time breakpoints — comma-separated seconds.
            let ts: Vec<TimeBreakpoint> = times
                .text()
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok())
                .map(|t| TimeBreakpoint { t, condition: None })
                .collect();
            send_sim(&dap, &SimRequest::SetTimeBreakpoints(ts));

            // Signal breakpoint — selected edge + value condition.
            let condition = cond.text().to_string();
            if !condition.trim().is_empty() {
                if let Some(edge_id) = edges.get(edge_dd.selected() as usize) {
                    send_sim(
                        &dap,
                        &SimRequest::SetSignalBreakpoints {
                            source: source.clone(),
                            breakpoints: vec![SignalBreakpoint {
                                edge_id: edge_id.clone(),
                                condition: Some(condition),
                            }],
                        },
                    );
                }
            }

            // Live solver tuning.
            let rel_tol = rtol.text().trim().parse::<f64>().ok();
            let max_step = mstep.text().trim().parse::<f64>().ok();
            if rel_tol.is_some() || max_step.is_some() {
                send_sim(&dap, &SimRequest::ConfigureSolver { rel_tol, max_step });
            }
            pop.popdown();
        });
    }
    pop.popup();
}

/// Persist the current model to disk for the simulator to read; returns the
/// written path, or `None` (after logging) if encoding/writing/matlabc fails.
fn write_model_to(
    app: &Rc<AppState>,
    vm: &Rc<MflowLinkViewModel>,
    path: Option<&Path>,
) -> Option<PathBuf> {
    let file = path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::temp_dir().join("matforge_sim.mflow"));
    let json = match vm
        .document
        .with(matforge_core::services::flowchart_codec::encode_string)
    {
        Ok(j) => j,
        Err(e) => {
            app.vm.console.log(
                matforge_core::models::ConsoleLevel::Error,
                format!("encode: {e}"),
            );
            return None;
        }
    };
    if std::fs::write(&file, json).is_err() {
        app.vm.console.log(
            matforge_core::models::ConsoleLevel::Error,
            "could not write model",
        );
        return None;
    }
    if !app.settings.matlabc_path.exists() {
        app.vm.console.log(
            matforge_core::models::ConsoleLevel::Error,
            "matlabc not found",
        );
        return None;
    }
    Some(file)
}

/// Boot a live `matlabc -simulate --sim-dap` session: handshake, then fold the
/// streamed simulation events into the view model. The transport row drives it
/// with [`SimRequest`]s.
fn start_live_simulation(
    app: &Rc<AppState>,
    vm: &Rc<MflowLinkViewModel>,
    dap: &LiveSession,
    path: Option<&Path>,
) {
    let Some(file) = write_model_to(app, vm, path) else {
        return;
    };
    vm.start_live();
    let vm2 = vm.clone();
    let dap2 = dap.clone();
    let program = file.to_string_lossy().into_owned();
    let started = DapSession::start_simulate(&app.settings.matlabc_path, &file, move |body| {
        if body == crate::process::DAP_EXIT {
            vm2.finish();
            return;
        }
        let Some(msg) = parse_message(&body) else {
            return;
        };
        match &msg {
            // Handshake: initialize → launch → (initialized) → configurationDone.
            DapMessage::Response { command, .. } if command.as_str() == "initialize" => {
                send_dap(
                    &dap2,
                    "launch",
                    Some(json!({ "program": program, "stopOnEntry": true })),
                );
            }
            DapMessage::Event { event, .. } if event.as_str() == "initialized" => {
                send_dap(&dap2, "configurationDone", None);
            }
            _ => {
                if let Some(ev) = sim_dap::parse_sim_event(&msg) {
                    vm2.on_sim_event(&ev);
                }
            }
        }
    });
    match started {
        Ok(session) => {
            *dap.borrow_mut() = Some(session);
            send_dap(
                dap,
                "initialize",
                Some(json!({ "clientID": "matforge", "adapterID": "matlabc" })),
            );
        }
        Err(e) => {
            app.vm.console.log(
                matforge_core::models::ConsoleLevel::Error,
                format!("sim-dap: {e}"),
            );
            vm.reset();
        }
    }
}

/// Start (or restart) `matlabc -simulate`, routing each line into the VM.
fn start_simulation(
    app: &Rc<AppState>,
    vm: &Rc<MflowLinkViewModel>,
    sim: &Rc<RefCell<Option<SimHandle>>>,
    path: Option<&Path>,
) {
    // Persist the current document so the simulator reads the latest model.
    let owned;
    let file: &Path = match path {
        Some(p) => p,
        None => {
            owned = std::env::temp_dir().join("matforge_sim.mflow");
            &owned
        }
    };
    let json = match vm
        .document
        .with(matforge_core::services::flowchart_codec::encode_string)
    {
        Ok(j) => j,
        Err(e) => {
            app.vm.console.log(
                matforge_core::models::ConsoleLevel::Error,
                format!("encode: {e}"),
            );
            return;
        }
    };
    if std::fs::write(file, json).is_err() {
        app.vm.console.log(
            matforge_core::models::ConsoleLevel::Error,
            "could not write model",
        );
        return;
    }
    if !app.settings.matlabc_path.exists() {
        app.vm.console.log(
            matforge_core::models::ConsoleLevel::Error,
            "matlabc not found",
        );
        return;
    }

    vm.start();
    let vm2 = vm.clone();
    let handle = crate::process::run_simulation(&app.settings.matlabc_path, file, move |line| {
        if let Some(_code) = line.strip_prefix(crate::process::RUN_EXIT_PREFIX) {
            vm2.finish();
        } else if vm2.state.get() != SimState::Paused {
            vm2.feed_line(&line);
        }
    });
    match handle {
        Ok(h) => *sim.borrow_mut() = Some(h),
        Err(e) => app.vm.console.log(
            matforge_core::models::ConsoleLevel::Error,
            format!("simulate: {e}"),
        ),
    }
}

fn build_model_canvas(vm: &Rc<MflowLinkViewModel>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 0);
    v.add_css_class("mf-panel");
    let header = Label::new(Some("MODEL"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    v.append(&header);

    let canvas = DrawingArea::new();
    canvas.set_vexpand(true);
    canvas.set_hexpand(true);
    // The model auto-fits each frame; this offset lets the middle button pan it.
    let user_pan: Rc<std::cell::Cell<(f64, f64)>> = Rc::new(std::cell::Cell::new((0.0, 0.0)));
    {
        let vm = vm.clone();
        let user_pan = user_pan.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            // In live mode the active block (simulationActiveBlock) gets the
            // execution halo so you can watch the solver walk the diagram.
            let active = vm.active_block.get();
            vm.document.with(|doc| {
                let bounds = flow_render::content_bounds(doc);
                let mut vp = fit_viewport(bounds, w as f64, h as f64);
                let (ux, uy) = user_pan.get();
                vp.pan = (vp.pan.0 + ux, vp.pan.1 + uy);
                let bps = std::collections::BTreeMap::new();
                let algebraic = doc.algebraic_loop_nodes();
                flow_render::draw_document(
                    ctx,
                    w as f64,
                    h as f64,
                    doc,
                    vp,
                    None,
                    &bps,
                    active.as_deref(),
                    &algebraic,
                );
            });
        });
    }
    // Redraw the model when the active block changes so the halo follows along.
    {
        let canvas = canvas.clone();
        vm.active_block.subscribe(move |_| canvas.queue_draw());
    }

    // Middle-button drag pans the model (offset from the pan at drag start).
    let pan = gtk::GestureDrag::new();
    pan.set_button(gtk::gdk::BUTTON_MIDDLE);
    let pan_origin: Rc<std::cell::Cell<(f64, f64)>> = Rc::new(std::cell::Cell::new((0.0, 0.0)));
    {
        let user_pan = user_pan.clone();
        let pan_origin = pan_origin.clone();
        pan.connect_drag_begin(move |_g, _x, _y| pan_origin.set(user_pan.get()));
    }
    {
        let canvas2 = canvas.clone();
        pan.connect_drag_update(move |_g, dx, dy| {
            let (ox, oy) = pan_origin.get();
            user_pan.set((ox + dx, oy + dy));
            canvas2.queue_draw();
        });
    }
    canvas.add_controller(pan);

    v.append(&canvas);
    v
}

/// A viewport that frames `bounds` within `(w, h)` with a margin.
fn fit_viewport(bounds: Option<(f64, f64, f64, f64)>, w: f64, h: f64) -> Viewport {
    let Some((minx, miny, maxx, maxy)) = bounds else {
        return Viewport {
            pan: (0.0, 0.0),
            zoom: 1.0,
        };
    };
    let (bw, bh) = ((maxx - minx).max(1.0), (maxy - miny).max(1.0));
    let margin = 40.0;
    let zoom = ((w - 2.0 * margin) / bw)
        .min((h - 2.0 * margin) / bh)
        .clamp(0.2, 2.0);
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    Viewport {
        pan: (w / 2.0 - cx * zoom, h / 2.0 - cy * zoom),
        zoom,
    }
}

/// The scope tiles: one line plot per logged signal, rebuilt when the signal
/// set changes and redrawn as samples stream in.
fn build_scopes(app: &Rc<AppState>, vm: &Rc<MflowLinkViewModel>, path: Option<PathBuf>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-left");
    panel.set_size_request(420, -1);

    // Header row: title + an Export-CSV action for the collected trace.
    let header_row = GtkBox::new(Orientation::Horizontal, 6);
    header_row.set_margin_start(8);
    header_row.set_margin_end(8);
    header_row.set_margin_top(6);
    let header = Label::new(Some("SCOPES"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_hexpand(true);
    header_row.append(&header);
    let export = Button::with_label("Export CSV");
    export.add_css_class("mf-tool");
    export.set_tooltip_text(Some("Save the collected trace as CSV"));
    {
        let app = app.clone();
        let vm = vm.clone();
        export.connect_clicked(move |_| export_trace_csv(&app, &vm, path.as_deref()));
    }
    header_row.append(&export);
    panel.append(&header_row);

    let tiles = GtkBox::new(Orientation::Vertical, 6);
    tiles.set_margin_start(6);
    tiles.set_margin_end(6);
    tiles.set_margin_top(4);
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&tiles));
    panel.append(&scroll);

    let empty = Label::new(Some("Press Play to run the simulation."));
    empty.add_css_class("mf-text-muted");
    empty.set_margin_top(12);
    tiles.append(&empty);

    // Rebuild the tile list when the signal count changes; otherwise just
    // redraw existing tiles. `tile_count` caches the current tile arity.
    let tile_count = Rc::new(RefCell::new(0usize));
    let draws: Rc<RefCell<Vec<DrawingArea>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let vm = vm.clone();
        let tiles = tiles.clone();
        let tile_count = tile_count.clone();
        let draws = draws.clone();
        let sc = vm.sample_count.clone();
        sc.subscribe(move |_| {
            let n = vm.signal_count();
            if n != *tile_count.borrow() {
                *tile_count.borrow_mut() = n;
                while let Some(c) = tiles.first_child() {
                    tiles.remove(&c);
                }
                draws.borrow_mut().clear();
                if n == 0 {
                    return;
                }
                for i in 0..n {
                    let name = vm.scope_name(i).unwrap_or_else(|| "signal".to_string());
                    tiles.append(&scope_label(&name));
                    let da = DrawingArea::new();
                    da.set_size_request(-1, 130);
                    da.add_css_class("mf-thumb");
                    // Cursor pixel for the crosshair value-readout (None = no hover).
                    let hover: Rc<std::cell::Cell<Option<(f64, f64)>>> =
                        Rc::new(std::cell::Cell::new(None));
                    let vm2 = vm.clone();
                    let idx = i;
                    let title = name.clone();
                    let hover_draw = hover.clone();
                    da.set_draw_func(move |_a, ctx, w, h| {
                        let (mut xs, mut ys) = vm2.scope_series(idx);
                        // Live mode shows every streamed sample; one-shot CSV
                        // replay draws only up to the scrubbable playback cursor.
                        let n = if vm2.live.get() {
                            xs.len()
                        } else {
                            vm2.cursor.get().min(xs.len())
                        };
                        xs.truncate(n);
                        ys.truncate(n);
                        let fig = PlotFigure::series(
                            idx as i32 + 1,
                            title.clone(),
                            PlotKind::Line2D,
                            xs,
                            ys,
                        );
                        crate::plot_render::draw_figure(
                            ctx,
                            w as f64,
                            h as f64,
                            &fig,
                            None,
                            hover_draw.get(),
                            None,
                        );
                    });
                    // Hovering a tile shows a crosshair + nearest-sample readout.
                    let motion = gtk::EventControllerMotion::new();
                    {
                        let hover = hover.clone();
                        let da2 = da.clone();
                        motion.connect_motion(move |_m, x, y| {
                            hover.set(Some((x, y)));
                            da2.queue_draw();
                        });
                    }
                    {
                        let hover = hover.clone();
                        let da2 = da.clone();
                        motion.connect_leave(move |_m| {
                            hover.set(None);
                            da2.queue_draw();
                        });
                    }
                    da.add_controller(motion);
                    tiles.append(&da);
                    draws.borrow_mut().push(da);
                }
            } else {
                for da in draws.borrow().iter() {
                    da.queue_draw();
                }
            }
        });
    }
    // Redraw the scopes whenever the playback cursor moves (play / step / scrub).
    {
        let draws = draws.clone();
        let cur = vm.cursor.clone();
        cur.subscribe(move |_| {
            for da in draws.borrow().iter() {
                da.queue_draw();
            }
        });
    }

    panel
}

fn scope_label(name: &str) -> Label {
    let l = Label::new(Some(name));
    l.add_css_class("mf-col-title");
    l.set_halign(gtk::Align::Start);
    l
}

/// Write the collected trace as CSV beside the model (`<model>.trace.csv`), or
/// to a temp file for an untitled model, and toast the result.
fn export_trace_csv(app: &Rc<AppState>, vm: &Rc<MflowLinkViewModel>, path: Option<&Path>) {
    if vm.total_samples() == 0 {
        app.vm
            .toast
            .show("No trace to export — run the simulation first.");
        return;
    }
    let dest = match path {
        Some(p) => p.with_extension("trace.csv"),
        None => std::env::temp_dir().join("mflowlink_trace.csv"),
    };
    let csv = vm.trace.with(|t| t.to_csv());
    match std::fs::write(&dest, csv) {
        Ok(()) => app
            .vm
            .toast
            .show(format!("Exported trace to {}", dest.display())),
        Err(e) => app.vm.toast.show(format!("Export failed: {e}")),
    }
}
