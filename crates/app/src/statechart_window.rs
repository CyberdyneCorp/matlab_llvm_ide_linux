//! Standalone mStateflow window: a state-chart canvas (active states highlighted)
//! beside a streaming event log, driven by `matlabc -emit-trace`. The tested
//! `StateChartViewModel` holds the events + active set; this is GTK glue plus the
//! subprocess.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, DrawingArea, Label, ListBox, Orientation, ScrolledWindow, Window,
};

use serde_json::json;

use matforge_core::models::flowchart::FlowchartDocument;
use matforge_core::models::ConsoleLevel;
use matforge_core::services::chart_trace::parse_chart_dap_event;
use matforge_core::services::dap::{parse_message, DapMessage};
use matforge_core::viewmodels::{SimState, StateChartViewModel};

use crate::app_state::AppState;
use crate::flow_render::{self, Viewport};
use crate::process::{DapSession, SimHandle};

/// A live `matlabc -simulate --sim-dap` chart session (vs one-shot -emit-trace).
type LiveSession = Rc<RefCell<Option<DapSession>>>;

/// Open a state-machine window for a state-chart document. `autostart` runs the
/// trace immediately (used by the `MATFORGE_STATECHART` demo hook).
pub fn open(
    app: &Rc<AppState>,
    document: FlowchartDocument,
    path: Option<PathBuf>,
    autostart: bool,
) {
    let vm = Rc::new(StateChartViewModel::new(document));
    let sim: Rc<RefCell<Option<SimHandle>>> = Rc::new(RefCell::new(None));
    let dap: LiveSession = Rc::new(RefCell::new(None));

    // Publish state for end-to-end tests (no-op unless $MATFORGE_E2E_STATE set).
    {
        let vm = vm.clone();
        crate::e2e::set_statechart_probe(move || {
            serde_json::json!({
                "state": format!("{:?}", vm.state.get()),
                "events": vm.event_count(),
                "active": vm.active_states.with(|s| s.len()),
            })
        });
    }

    let window = Window::builder()
        .title(format!(
            "mStateflow — {}",
            path.as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled".into())
        ))
        .default_width(1040)
        .default_height(640)
        .build();
    window.add_css_class("mf-root");

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");
    root.append(&build_transport(app, &vm, &sim, &dap, path.clone()));

    let split = gtk::Paned::new(Orientation::Horizontal);
    split.set_wide_handle(true);
    split.set_vexpand(true);
    split.set_start_child(Some(&build_chart_canvas(&vm)));
    split.set_end_child(Some(&build_event_log(&vm)));
    split.set_position(600);
    root.append(&split);
    window.set_child(Some(&root));

    {
        let sim = sim.clone();
        let dap = dap.clone();
        let vm = vm.clone();
        window.connect_close_request(move |_| {
            *sim.borrow_mut() = None;
            *dap.borrow_mut() = None;
            vm.reset();
            crate::e2e::clear_statechart_probe();
            gtk::glib::Propagation::Proceed
        });
    }
    window.present();

    if autostart {
        start_trace(app, &vm, &sim, path.as_deref());
    }
}

fn build_transport(
    app: &Rc<AppState>,
    vm: &Rc<StateChartViewModel>,
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

    let play = Button::with_label("▶ Run");
    play.add_css_class("mf-tool");
    play.add_css_class("mf-run");
    let super_step = Button::with_label("Step Super-Step");
    super_step.add_css_class("mf-tool");
    super_step.set_tooltip_text(Some("Fire transitions until quiescent (live)"));
    let transition = Button::with_label("Step Transition");
    transition.add_css_class("mf-tool");
    transition.set_tooltip_text(Some("Fire exactly one transition (live)"));
    let stop = Button::with_label("⏹ Stop");
    stop.add_css_class("mf-tool");
    stop.add_css_class("mf-stop");
    let reset = Button::with_label("⟲ Reset");
    reset.add_css_class("mf-tool");
    {
        let app = app.clone();
        let vm = vm.clone();
        let dap = dap.clone();
        play.connect_clicked(move |_| start_live_chart(&app, &vm, &dap, path.as_deref()));
    }
    {
        let dap = dap.clone();
        super_step.connect_clicked(move |_| send_chart(&dap, "stateChart/stepSuperStep"));
    }
    {
        let dap = dap.clone();
        transition.connect_clicked(move |_| send_chart(&dap, "stateChart/stepTransition"));
    }
    {
        let vm = vm.clone();
        let sim = sim.clone();
        let dap = dap.clone();
        stop.connect_clicked(move |_| {
            *sim.borrow_mut() = None;
            *dap.borrow_mut() = None;
            vm.finish();
        });
    }
    {
        let vm = vm.clone();
        let sim = sim.clone();
        let dap = dap.clone();
        reset.connect_clicked(move |_| {
            *sim.borrow_mut() = None;
            *dap.borrow_mut() = None;
            vm.reset();
        });
    }
    bar.append(&play);
    bar.append(&super_step);
    bar.append(&transition);
    bar.append(&stop);
    bar.append(&reset);

    let status = Label::new(Some("idle"));
    status.add_css_class("mf-text-secondary");
    status.set_margin_start(12);
    bar.append(&status);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    let active = Label::new(Some("active: —"));
    active.add_css_class("mf-mono");
    bar.append(&active);

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
    {
        let active = active.clone();
        vm.active_states.bind(move |set| {
            let names: Vec<&str> = set.iter().map(String::as_str).collect();
            active.set_text(&format!(
                "active: {}",
                if names.is_empty() {
                    "—".into()
                } else {
                    names.join(", ")
                }
            ));
        });
    }
    bar
}

/// Send a generic DAP request to the live chart session.
fn send_dap(dap: &LiveSession, command: &str, args: Option<serde_json::Value>) {
    if let Some(s) = dap.borrow_mut().as_mut() {
        let frame = s.client.request(command, args);
        let _ = s.write_frame(&frame);
    }
}

/// Send a no-argument `stateChart/*` control request.
fn send_chart(dap: &LiveSession, command: &str) {
    send_dap(dap, command, None);
}

/// Boot a live `matlabc -simulate --sim-dap` chart session: handshake, then fold
/// the `stateChart/*` event stream into the view model. The transport drives it
/// with stepSuperStep / stepTransition.
fn start_live_chart(
    app: &Rc<AppState>,
    vm: &Rc<StateChartViewModel>,
    dap: &LiveSession,
    path: Option<&Path>,
) {
    let file = match path {
        Some(p) => p.to_path_buf(),
        None => std::env::temp_dir().join("matforge_chart.mflow"),
    };
    let json = match vm
        .document
        .with(matforge_core::services::flowchart_codec::encode_string)
    {
        Ok(j) => j,
        Err(e) => {
            return app
                .vm
                .console
                .log(ConsoleLevel::Error, format!("encode: {e}"))
        }
    };
    if std::fs::write(&file, json).is_err() {
        return app
            .vm
            .console
            .log(ConsoleLevel::Error, "could not write chart");
    }
    if !app.settings.matlabc_path.exists() {
        return app.vm.console.log(ConsoleLevel::Error, "matlabc not found");
    }

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
                if let Some(ev) = parse_chart_dap_event(&msg) {
                    vm2.apply_event(ev);
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
            app.vm
                .console
                .log(ConsoleLevel::Error, format!("sim-dap: {e}"));
            vm.reset();
        }
    }
}

fn start_trace(
    app: &Rc<AppState>,
    vm: &Rc<StateChartViewModel>,
    sim: &Rc<RefCell<Option<SimHandle>>>,
    path: Option<&Path>,
) {
    let owned;
    let file: &Path = match path {
        Some(p) => p,
        None => {
            owned = std::env::temp_dir().join("matforge_chart.mflow");
            &owned
        }
    };
    let json = match vm
        .document
        .with(matforge_core::services::flowchart_codec::encode_string)
    {
        Ok(j) => j,
        Err(e) => {
            return app
                .vm
                .console
                .log(ConsoleLevel::Error, format!("encode: {e}"))
        }
    };
    if std::fs::write(file, json).is_err() {
        return app
            .vm
            .console
            .log(ConsoleLevel::Error, "could not write chart");
    }
    if !app.settings.matlabc_path.exists() {
        return app.vm.console.log(ConsoleLevel::Error, "matlabc not found");
    }

    vm.start();
    let vm2 = vm.clone();
    let handle = crate::process::run_chart_trace(&app.settings.matlabc_path, file, move |line| {
        if line.starts_with(crate::process::RUN_EXIT_PREFIX) {
            vm2.finish();
        } else {
            vm2.feed_line(&line);
        }
    });
    match handle {
        Ok(h) => *sim.borrow_mut() = Some(h),
        Err(e) => app
            .vm
            .console
            .log(ConsoleLevel::Error, format!("trace: {e}")),
    }
}

fn build_chart_canvas(vm: &Rc<StateChartViewModel>) -> GtkBox {
    let v = GtkBox::new(Orientation::Vertical, 0);
    v.add_css_class("mf-panel");
    let header = Label::new(Some("CHART"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    v.append(&header);

    let canvas = DrawingArea::new();
    canvas.set_vexpand(true);
    canvas.set_hexpand(true);
    {
        let vm = vm.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            vm.document.with(|doc| {
                let vp = fit_viewport(flow_render::content_bounds(doc), w as f64, h as f64);
                // Highlight the most recently entered active state.
                let active = vm.active_states.with(|s| s.iter().next_back().cloned());
                let bps = BTreeMap::new();
                // Charts have no algebraic loops; the live runner doesn't lint.
                let algebraic = std::collections::BTreeSet::new();
                let lint = std::collections::BTreeSet::new();
                let hierarchy_lint = std::collections::BTreeSet::new();
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
                    &lint,
                    &hierarchy_lint,
                );
            });
        });
    }
    {
        let canvas = canvas.clone();
        vm.active_states.subscribe(move |_| canvas.queue_draw());
    }
    v.append(&canvas);
    v
}

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

fn build_event_log(vm: &Rc<StateChartViewModel>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-left");
    panel.set_size_request(380, -1);
    let header = Label::new(Some("EVENT LOG"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    panel.append(&header);

    let list = ListBox::new();
    list.add_css_class("mf-event-log");
    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    panel.append(&scroll);

    {
        let list = list.clone();
        vm.events.bind(move |events| {
            while let Some(c) = list.first_child() {
                list.remove(&c);
            }
            // Show the most recent events last; cap to keep the list snappy.
            let start = events.len().saturating_sub(500);
            for e in &events[start..] {
                let row = Label::new(Some(&e.summary()));
                row.set_halign(gtk::Align::Start);
                row.add_css_class("mf-event-row");
                list.append(&row);
            }
        });
    }
    panel
}
