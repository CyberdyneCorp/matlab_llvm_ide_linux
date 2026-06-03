//! Standalone mflowLink window: a signal-flow model canvas beside live scope
//! tiles, driven by `matlabc -simulate`. The tested `MflowLinkViewModel` holds
//! the trace + transport state; this is GTK + Cairo glue plus the subprocess.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DrawingArea, Label, Orientation, ScrolledWindow, Window};

use matforge_core::models::flowchart::FlowchartDocument;
use matforge_core::models::{PlotFigure, PlotKind};
use matforge_core::viewmodels::{MflowLinkViewModel, SimState};

use crate::app_state::AppState;
use crate::flow_render::{self, Viewport};
use crate::process::SimHandle;

/// Open a simulation window for a signal-flow document. `autostart` immediately
/// runs the simulation (used by the `MATFORGE_SIMULATE` demo hook).
pub fn open(app: &Rc<AppState>, document: FlowchartDocument, path: Option<PathBuf>, autostart: bool) {
    let vm = Rc::new(MflowLinkViewModel::new(document));
    let sim: Rc<RefCell<Option<SimHandle>>> = Rc::new(RefCell::new(None));

    let window = Window::builder()
        .title(format!(
            "mflowLink — {}",
            path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled".into())
        ))
        .default_width(1100)
        .default_height(680)
        .build();
    window.add_css_class("mf-root");

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");
    root.append(&build_transport(app, &vm, &sim, path.clone()));

    let split = gtk::Paned::new(Orientation::Horizontal);
    split.set_wide_handle(true);
    split.set_vexpand(true);
    split.set_start_child(Some(&build_model_canvas(&vm)));
    split.set_end_child(Some(&build_scopes(&vm)));
    split.set_position(560);
    root.append(&split);

    window.set_child(Some(&root));

    // Kill the simulation if the window is closed.
    {
        let sim = sim.clone();
        let vm = vm.clone();
        window.connect_close_request(move |_| {
            *sim.borrow_mut() = None; // drops SimHandle -> kills process
            vm.reset();
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

    {
        let app = app.clone();
        let vm = vm.clone();
        let sim = sim.clone();
        let timer = timer.clone();
        play.connect_clicked(move |_| match vm.state.get() {
            // First Play collects the trace live (the cursor follows the edge).
            SimState::Idle => start_simulation(&app, &vm, &sim, path.as_deref()),
            // Afterwards, Play animates the cursor through the collected trace.
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
        let stop_timer = stop_timer.clone();
        pause.connect_clicked(move |_| {
            stop_timer();
            vm.pause();
        });
    }
    {
        let vm = vm.clone();
        let stop_timer = stop_timer.clone();
        step.connect_clicked(move |_| {
            stop_timer();
            vm.step();
        });
    }
    {
        let vm = vm.clone();
        let sim = sim.clone();
        let stop_timer = stop_timer.clone();
        stop.connect_clicked(move |_| {
            stop_timer();
            *sim.borrow_mut() = None; // kill the simulator
            vm.finish();
        });
    }
    {
        let vm = vm.clone();
        let sim = sim.clone();
        let stop_timer = stop_timer.clone();
        reset.connect_clicked(move |_| {
            stop_timer();
            *sim.borrow_mut() = None;
            vm.finish(); // stop collecting, then rewind playback to the start
            vm.rewind();
        });
    }
    bar.append(&play);
    bar.append(&pause);
    bar.append(&step);
    bar.append(&stop);
    bar.append(&reset);

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

    bar
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
    let json = match vm.document.with(matforge_core::services::flowchart_codec::encode_string) {
        Ok(j) => j,
        Err(e) => {
            app.vm.console.log(matforge_core::models::ConsoleLevel::Error, format!("encode: {e}"));
            return;
        }
    };
    if std::fs::write(file, json).is_err() {
        app.vm.console.log(matforge_core::models::ConsoleLevel::Error, "could not write model");
        return;
    }
    if !app.settings.matlabc_path.exists() {
        app.vm.console.log(matforge_core::models::ConsoleLevel::Error, "matlabc not found");
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
        Err(e) => app.vm.console.log(matforge_core::models::ConsoleLevel::Error, format!("simulate: {e}")),
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
    {
        let vm = vm.clone();
        canvas.set_draw_func(move |_a, ctx, w, h| {
            vm.document.with(|doc| {
                let bounds = flow_render::content_bounds(doc);
                let vp = fit_viewport(bounds, w as f64, h as f64);
                let bps = std::collections::BTreeMap::new();
                flow_render::draw_document(ctx, w as f64, h as f64, doc, vp, None, &bps, None);
            });
        });
    }
    v.append(&canvas);
    v
}

/// A viewport that frames `bounds` within `(w, h)` with a margin.
fn fit_viewport(bounds: Option<(f64, f64, f64, f64)>, w: f64, h: f64) -> Viewport {
    let Some((minx, miny, maxx, maxy)) = bounds else {
        return Viewport { pan: (0.0, 0.0), zoom: 1.0 };
    };
    let (bw, bh) = ((maxx - minx).max(1.0), (maxy - miny).max(1.0));
    let margin = 40.0;
    let zoom = ((w - 2.0 * margin) / bw).min((h - 2.0 * margin) / bh).clamp(0.2, 2.0);
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    Viewport { pan: (w / 2.0 - cx * zoom, h / 2.0 - cy * zoom), zoom }
}

/// The scope tiles: one line plot per logged signal, rebuilt when the signal
/// set changes and redrawn as samples stream in.
fn build_scopes(vm: &Rc<MflowLinkViewModel>) -> GtkBox {
    let panel = GtkBox::new(Orientation::Vertical, 0);
    panel.add_css_class("mf-panel");
    panel.add_css_class("mf-border-left");
    panel.set_size_request(420, -1);
    let header = Label::new(Some("SCOPES"));
    header.add_css_class("mf-panel-header");
    header.set_halign(gtk::Align::Start);
    header.set_margin_start(8);
    header.set_margin_top(6);
    panel.append(&header);

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
                    let name = vm.trace.with(|t| t.signal_name(i).unwrap_or("signal").to_string());
                    tiles.append(&scope_label(&name));
                    let da = DrawingArea::new();
                    da.set_size_request(-1, 130);
                    da.add_css_class("mf-thumb");
                    let vm2 = vm.clone();
                    let idx = i;
                    let title = name.clone();
                    da.set_draw_func(move |_a, ctx, w, h| {
                        let (mut xs, mut ys) = vm2.trace.with(|t| t.series(idx));
                        // Only draw up to the playback cursor (live edge while
                        // collecting; scrubbed by play/step afterwards).
                        let n = vm2.cursor.get().min(xs.len());
                        xs.truncate(n);
                        ys.truncate(n);
                        let fig = PlotFigure::series(idx as i32 + 1, title.clone(), PlotKind::Line2D, xs, ys);
                        crate::plot_render::draw_figure(ctx, w as f64, h as f64, &fig);
                    });
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
