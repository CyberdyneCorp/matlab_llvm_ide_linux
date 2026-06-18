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
use matforge_core::services::dap::{parse_message, DapMessage};
use matforge_core::services::scope::{signal_color, ScopeView};
use matforge_core::services::sim_dap::{self, SimRequest};
use matforge_core::viewmodels::{MflowLinkViewModel, SimState};

use crate::scope_render::ScopeSeries;

use crate::app_state::AppState;
use crate::flow_render::{self, Viewport};
use crate::process::{DapSession, SimHandle};

/// A live `matlabc -simulate --sim-dap` session, when the transport is in live
/// mode (vs the one-shot CSV path).
type LiveSession = Rc<RefCell<Option<DapSession>>>;

/// `(name, rgb color, (time, value) points)` for one logged signal.
type SignalSeries = (String, (f64, f64, f64), Vec<(f64, f64)>);

/// In-progress box-zoom rectangle in widget pixels `(x0, y0, x1, y1)`.
type DragCell = Rc<std::cell::Cell<Option<(f64, f64, f64, f64)>>>;

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

/// `(time, value)` points for every logged signal, truncated to the playback
/// cursor in one-shot replay (live mode shows every streamed sample).
fn collect_points(vm: &Rc<MflowLinkViewModel>) -> Vec<SignalSeries> {
    let n = vm.signal_count();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (xs, ys) = vm.scope_series(i);
        let cut = if vm.live.get() {
            xs.len()
        } else {
            vm.cursor.get().min(xs.len())
        };
        let pts: Vec<(f64, f64)> = xs
            .iter()
            .zip(&ys)
            .take(cut)
            .map(|(&x, &y)| (x, y))
            .collect();
        let name = vm.scope_name(i).unwrap_or_else(|| "signal".to_string());
        out.push((name, signal_color(i), pts));
    }
    out
}

/// The resolved data window for the current view (autoscale or pinned).
fn resolve_window(vm: &Rc<MflowLinkViewModel>, view: ScopeView) -> (f64, f64, f64, f64) {
    let points: Vec<Vec<(f64, f64)>> = collect_points(vm).into_iter().map(|d| d.2).collect();
    view.resolve(&points)
}

/// Draw the overlay scope into `ctx` for the given view + interaction state.
fn draw_scope(
    ctx: &gtk::cairo::Context,
    w: f64,
    h: f64,
    vm: &Rc<MflowLinkViewModel>,
    view: ScopeView,
    hover: Option<(f64, f64)>,
    drag: Option<(f64, f64, f64, f64)>,
) {
    let data = collect_points(vm);
    let points: Vec<Vec<(f64, f64)>> = data.iter().map(|d| d.2.clone()).collect();
    let window = view.resolve(&points);
    let series: Vec<ScopeSeries> = data
        .iter()
        .map(|(name, color, pts)| ScopeSeries {
            name,
            color: *color,
            points: pts,
        })
        .collect();
    crate::scope_render::draw_overlay(ctx, w, h, &series, window, hover, drag);
}

/// Render the current scope to a PNG beside the model (`<model>.scope.png`).
fn export_scope_png(
    app: &Rc<AppState>,
    da: &DrawingArea,
    vm: &Rc<MflowLinkViewModel>,
    view: ScopeView,
    path: Option<&Path>,
) {
    let (w, h) = (da.width(), da.height());
    if w == 0 || h == 0 {
        app.vm
            .toast
            .show("Resize the window before exporting a PNG");
        return;
    }
    let surface = match gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h) {
        Ok(s) => s,
        Err(e) => {
            app.vm.toast.show(format!("PNG export failed: {e}"));
            return;
        }
    };
    if let Ok(ctx) = gtk::cairo::Context::new(&surface) {
        draw_scope(&ctx, w as f64, h as f64, vm, view, None, None);
    }
    let dest = match path {
        Some(p) => p.with_extension("scope.png"),
        None => std::env::temp_dir().join("mflowlink_scope.png"),
    };
    let result = std::fs::File::create(&dest)
        .map_err(|e| e.to_string())
        .and_then(|mut f| surface.write_to_png(&mut f).map_err(|e| e.to_string()));
    match result {
        Ok(()) => app
            .vm
            .toast
            .show(format!("Saved scope to {}", dest.display())),
        Err(e) => app.vm.toast.show(format!("PNG export failed: {e}")),
    }
}

/// The production overlay scope: every logged signal on one set of axes with a
/// legend + stable colors, a hover crosshair value/time readout, box-zoom (drag)
/// and pan (middle-drag), autoscale / manual-Y, and CSV (visible) + PNG export.
/// Driven entirely by the `SimTrace` — no compiler involvement.
fn build_scopes(app: &Rc<AppState>, vm: &Rc<MflowLinkViewModel>, path: Option<PathBuf>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-left");
    panel.set_size_request(440, -1);

    let view: Rc<RefCell<ScopeView>> = Rc::new(RefCell::new(ScopeView::default()));
    let hover: Rc<std::cell::Cell<Option<(f64, f64)>>> = Rc::new(std::cell::Cell::new(None));
    let drag: DragCell = Rc::new(std::cell::Cell::new(None));

    let da = DrawingArea::new();
    da.set_vexpand(true);
    da.set_hexpand(true);
    da.set_size_request(-1, 260);
    da.add_css_class("mf-thumb");
    {
        let vm = vm.clone();
        let view = view.clone();
        let hover = hover.clone();
        let drag = drag.clone();
        da.set_draw_func(move |_a, ctx, w, h| {
            draw_scope(
                ctx,
                w as f64,
                h as f64,
                &vm,
                *view.borrow(),
                hover.get(),
                drag.get(),
            );
        });
    }

    panel.append(&build_scope_controls(app, vm, &da, &view, path));
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&da));
    panel.append(&scroll);

    // Hover → crosshair readout.
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

    // Left-drag → box-zoom.
    let zoom = gtk::GestureDrag::new();
    zoom.set_button(gtk::gdk::BUTTON_PRIMARY);
    {
        let drag = drag.clone();
        let da2 = da.clone();
        zoom.connect_drag_begin(move |_g, x, y| {
            drag.set(Some((x, y, x, y)));
            da2.queue_draw();
        });
    }
    {
        let drag = drag.clone();
        let da2 = da.clone();
        zoom.connect_drag_update(move |_g, dx, dy| {
            if let Some((sx, sy, _, _)) = drag.get() {
                drag.set(Some((sx, sy, sx + dx, sy + dy)));
                da2.queue_draw();
            }
        });
    }
    {
        let drag = drag.clone();
        let da2 = da.clone();
        let vm = vm.clone();
        let view = view.clone();
        zoom.connect_drag_end(move |_g, dx, dy| {
            if let Some((sx, sy, _, _)) = drag.take() {
                let (w, h) = (da2.width() as f64, da2.height() as f64);
                let window = resolve_window(&vm, *view.borrow());
                let plot = crate::scope_render::plot_rect(w, h);
                let next = view
                    .borrow()
                    .zoom_to_box(window, plot, (sx, sy, sx + dx, sy + dy));
                *view.borrow_mut() = next;
                da2.queue_draw();
            }
        });
    }
    da.add_controller(zoom);

    // Middle-drag → pan (relative to the window at drag start).
    let pan = gtk::GestureDrag::new();
    pan.set_button(gtk::gdk::BUTTON_MIDDLE);
    let pan_start: Rc<std::cell::Cell<(f64, f64, f64, f64)>> =
        Rc::new(std::cell::Cell::new((0.0, 1.0, 0.0, 1.0)));
    {
        let pan_start = pan_start.clone();
        let vm = vm.clone();
        let view = view.clone();
        pan.connect_drag_begin(move |_g, _x, _y| {
            pan_start.set(resolve_window(&vm, *view.borrow()));
        });
    }
    {
        let pan_start = pan_start.clone();
        let da2 = da.clone();
        let view = view.clone();
        pan.connect_drag_update(move |_g, dx, dy| {
            let win = pan_start.get();
            let (_, _, pw, ph) =
                crate::scope_render::plot_rect(da2.width() as f64, da2.height() as f64);
            let (x0, x1, y0, y1) = win;
            let ddx = -dx / pw * (x1 - x0);
            let ddy = dy / ph * (y1 - y0);
            *view.borrow_mut() = ScopeView::panned(win, ddx, ddy);
            da2.queue_draw();
        });
    }
    da.add_controller(pan);

    // Redraw as samples stream in and as the playback cursor moves.
    {
        let da2 = da.clone();
        vm.sample_count.subscribe(move |_| da2.queue_draw());
    }
    {
        let da2 = da.clone();
        vm.cursor.subscribe(move |_| da2.queue_draw());
    }
    panel
}

/// The scope's control row: autoscale reset, manual Y range, and CSV / PNG export.
fn build_scope_controls(
    app: &Rc<AppState>,
    vm: &Rc<MflowLinkViewModel>,
    da: &DrawingArea,
    view: &Rc<RefCell<ScopeView>>,
    path: Option<PathBuf>,
) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 6);
    row.set_margin_start(8);
    row.set_margin_end(8);
    row.set_margin_top(6);
    let header = Label::new(Some("SCOPES"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_hexpand(true);
    row.append(&header);

    let auto = Button::with_label("Auto");
    auto.add_css_class("mf-tool");
    auto.set_tooltip_text(Some("Autoscale (reset zoom / Y range)"));
    {
        let view = view.clone();
        let da = da.clone();
        auto.connect_clicked(move |_| {
            *view.borrow_mut() = ScopeView::default();
            da.queue_draw();
        });
    }
    row.append(&auto);

    let y_min = gtk::Entry::new();
    y_min.set_placeholder_text(Some("Y min"));
    y_min.set_width_chars(6);
    let y_max = gtk::Entry::new();
    y_max.set_placeholder_text(Some("Y max"));
    y_max.set_width_chars(6);
    let set_y = Button::with_label("Set Y");
    set_y.add_css_class("mf-tool");
    set_y.set_tooltip_text(Some("Pin the Y range"));
    {
        let view = view.clone();
        let da = da.clone();
        let app = app.clone();
        let (y_min, y_max) = (y_min.clone(), y_max.clone());
        set_y.connect_clicked(move |_| {
            match (
                y_min.text().trim().parse::<f64>(),
                y_max.text().trim().parse::<f64>(),
            ) {
                (Ok(lo), Ok(hi)) if hi > lo => {
                    view.borrow_mut().y = Some((lo, hi));
                    da.queue_draw();
                }
                _ => app.vm.toast.show("Enter a valid Y min < Y max"),
            }
        });
    }
    row.append(&y_min);
    row.append(&y_max);
    row.append(&set_y);

    let csv = Button::with_label("CSV");
    csv.add_css_class("mf-tool");
    csv.set_tooltip_text(Some("Export the visible trace as CSV"));
    {
        let app = app.clone();
        let vm = vm.clone();
        let view = view.clone();
        let path = path.clone();
        csv.connect_clicked(move |_| {
            export_trace_csv(&app, &vm, path.as_deref(), view.borrow().x);
        });
    }
    row.append(&csv);

    let png = Button::with_label("PNG");
    png.add_css_class("mf-tool");
    png.set_tooltip_text(Some("Export the scope as PNG"));
    {
        let app = app.clone();
        let vm = vm.clone();
        let view = view.clone();
        let da = da.clone();
        png.connect_clicked(move |_| {
            export_scope_png(&app, &da, &vm, *view.borrow(), path.as_deref());
        });
    }
    row.append(&png);
    row
}
/// Write the trace as CSV beside the model (`<model>.trace.csv`), or to a temp
/// file for an untitled model, and toast the result. `window` restricts the
/// export to the visible time range (the scope's pinned X), or `None` for all.
fn export_trace_csv(
    app: &Rc<AppState>,
    vm: &Rc<MflowLinkViewModel>,
    path: Option<&Path>,
    window: Option<(f64, f64)>,
) {
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
    let csv = vm.trace.with(|t| t.to_csv_window(window));
    match std::fs::write(&dest, csv) {
        Ok(()) => app
            .vm
            .toast
            .show(format!("Exported trace to {}", dest.display())),
        Err(e) => app.vm.toast.show(format!("Export failed: {e}")),
    }
}
