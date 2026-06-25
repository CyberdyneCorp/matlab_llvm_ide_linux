//! Test-only state introspection for end-to-end tests.
//!
//! When `$MATFORGE_E2E_STATE` is set, a periodic JSON snapshot of testable state
//! is written to that path (atomically). It carries the view-model state an e2e
//! harness asserts on (active tab, breakpoints, workspace vars, plots, status,
//! panel visibility) **plus** the on-screen rectangles of the main drive targets
//! (the editor gutter, the REPL entry) so the harness clicks real coordinates
//! instead of guessing. Zero cost unless the env var is set.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::json;

use crate::app_state::AppState;

thread_local! {
    static ACTIVE_GUTTER: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static REPL_ENTRY: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static WORKSPACE_TABLE: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static PLOTS_ADD: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static SEARCH_ENTRY: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static WATCH_ENTRY: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static PLOTS_PLAY: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static EXPLORER_LIST: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static DEBUG_NEXT: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static DEBUG_CONTINUE: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static FLOWCHART_PALETTE: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
    static SCENE3D_BUTTON: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };

    // Records each successful 3-D scene generation: `{count, path}`. The display
    // (WebView / browser) is suppressed under e2e, so the harness asserts that
    // the babylon emit produced an HTML rather than on an un-introspectable window.
    static SCENE3D_GENERATED: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };

    // State probes for surfaces whose view models are not held on `app.vm`
    // (flowchart tabs and the standalone mflowLink / mStateflow windows). Each
    // captures its view model and returns the JSON the harness asserts on.
    static FLOWCHART_PROBE: RefCell<Option<Box<dyn Fn() -> serde_json::Value>>> = const { RefCell::new(None) };
    static MFLOWLINK_PROBE: RefCell<Option<Box<dyn Fn() -> serde_json::Value>>> = const { RefCell::new(None) };
    static STATECHART_PROBE: RefCell<Option<Box<dyn Fn() -> serde_json::Value>>> = const { RefCell::new(None) };
}

/// Record the BLOCKS palette list of the active flowchart tab (drive target).
pub fn set_flowchart_palette(w: &impl IsA<gtk::Widget>) {
    FLOWCHART_PALETTE.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the flowchart toolbar / mflowLink window 3-D Scene button (drive
/// target). Only present for models that contain 3-D scene blocks.
pub fn set_scene3d_button(w: &impl IsA<gtk::Widget>) {
    SCENE3D_BUTTON.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Note a successful 3-D scene generation (`{count, path}`). The display step is
/// suppressed under e2e (see [`is_active`]) so this is the assertable signal.
pub fn record_scene3d_generated(path: &std::path::Path) {
    SCENE3D_GENERATED.with(|c| {
        let mut slot = c.borrow_mut();
        let count = slot
            .as_ref()
            .and_then(|v| v.get("count"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            + 1;
        *slot = Some(json!({ "count": count, "path": path.to_string_lossy() }));
    });
}

/// Whether the e2e state harness is driving the app (env var set at launch).
/// Used to suppress the 3-D Scene window so the scenario stays deterministic.
pub fn is_active() -> bool {
    std::env::var_os("MATFORGE_E2E_STATE").is_some()
}

/// Publish the active flowchart tab's state (node/edge counts, selection, zoom).
pub fn set_flowchart_probe(f: impl Fn() -> serde_json::Value + 'static) {
    FLOWCHART_PROBE.with(|c| *c.borrow_mut() = Some(Box::new(f)));
}

/// Publish the open mflowLink window's simulation state (sim state, samples).
pub fn set_mflowlink_probe(f: impl Fn() -> serde_json::Value + 'static) {
    MFLOWLINK_PROBE.with(|c| *c.borrow_mut() = Some(Box::new(f)));
}

/// Stop publishing mflowLink state (the window closed).
pub fn clear_mflowlink_probe() {
    MFLOWLINK_PROBE.with(|c| *c.borrow_mut() = None);
}

/// Publish the open mStateflow window's trace state (run state, events, active).
pub fn set_statechart_probe(f: impl Fn() -> serde_json::Value + 'static) {
    STATECHART_PROBE.with(|c| *c.borrow_mut() = Some(Box::new(f)));
}

/// Stop publishing mStateflow state (the window closed).
pub fn clear_statechart_probe() {
    STATECHART_PROBE.with(|c| *c.borrow_mut() = None);
}

/// Record the Explorer file-tree list (drive target).
pub fn set_explorer_list(w: &impl IsA<gtk::Widget>) {
    EXPLORER_LIST.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the toolbar "Step Over" debug button (drive target).
pub fn set_debug_next(w: &impl IsA<gtk::Widget>) {
    DEBUG_NEXT.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the toolbar "Continue" debug button (drive target).
pub fn set_debug_continue(w: &impl IsA<gtk::Widget>) {
    DEBUG_CONTINUE.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the debug Watch expression entry (drive target).
pub fn set_watch_entry(w: &impl IsA<gtk::Widget>) {
    WATCH_ENTRY.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the Plots animation play/pause button (drive target).
pub fn set_plots_play(w: &impl IsA<gtk::Widget>) {
    PLOTS_PLAY.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the find-in-files entry (drive target).
pub fn set_search_entry(w: &impl IsA<gtk::Widget>) {
    SEARCH_ENTRY.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the gutter of the most recently built code view (drive target).
pub fn set_active_gutter(w: &impl IsA<gtk::Widget>) {
    ACTIVE_GUTTER.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the REPL input entry (drive target).
pub fn set_repl_entry(w: &impl IsA<gtk::Widget>) {
    REPL_ENTRY.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the workspace variable table (drive target).
pub fn set_workspace_table(w: &impl IsA<gtk::Widget>) {
    WORKSPACE_TABLE.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// Record the Plots "add" button (drive target).
pub fn set_plots_add(w: &impl IsA<gtk::Widget>) {
    PLOTS_ADD.with(|c| *c.borrow_mut() = Some(w.clone().upcast()));
}

/// `[x, y, w, h]` of `w` in window-client coordinates (the harness adds the
/// window's absolute screen origin). `None` until the widget is laid out.
fn rect_in_window(w: &gtk::Widget) -> Option<[i32; 4]> {
    let win = w.ancestor(gtk::Window::static_type())?;
    let p = w.compute_point(&win, &gtk::graphene::Point::new(0.0, 0.0))?;
    let (ww, wh) = (w.width(), w.height());
    if ww == 0 || wh == 0 {
        return None;
    }
    Some([p.x() as i32, p.y() as i32, ww, wh])
}

/// Start writing the state snapshot to `path` every 200 ms.
pub fn install_state_dump(app: Rc<AppState>, path: PathBuf) {
    glib::timeout_add_local(Duration::from_millis(200), move || {
        let active = app.vm.editor.active_tab();
        let mut breakpoints: Vec<usize> = active
            .as_ref()
            .map(|t| t.breakpoints.keys().copied().collect())
            .unwrap_or_default();
        breakpoints.sort_unstable();

        // Selected (or latest) figure: animation length + kind, for plot tests.
        let sel = app.vm.plots.selected_id.get();
        let (plot_anim, plot_kind) = app.vm.plots.figures.with(|figs| {
            let f = sel
                .and_then(|id| figs.iter().find(|f| f.id == id))
                .or_else(|| figs.last());
            (
                f.map(|f| f.animation_len()).unwrap_or(0),
                f.map(|f| f.kind.label().to_string()),
            )
        });

        let snap = json!({
            "active_tab": active.as_ref().map(|t| t.name.clone()),
            "active_breakpoints": breakpoints,
            "execution_line": active.as_ref().and_then(|t| t.execution_line),
            "tabs": app.vm.editor.tabs.with(|ts| ts.iter().map(|t| t.name.clone()).collect::<Vec<_>>()),
            "workspace": app.vm.workspace.variables.with(|vs| vs.iter().map(|v| v.name.clone()).collect::<Vec<_>>()),
            "inspected_matrix": app.vm.workspace.inspected_matrix.with(|m| {
                m.as_ref().map(|mm| json!({"title": mm.title, "rows": mm.rows, "cols": mm.cols}))
            }),
            "plots": app.vm.plots.figures.with(|f| f.len()),
            "search_results": app.vm.search.results.with(|r| r.len()),
            "problems": app.vm.console.problems.with(|p| p.len()),
            "console": app.vm.console.messages.with(|m| m.len()),
            "watch": app.vm.debug.evaluations.with(|e| e.len()),
            "function_breakpoints": app.vm.breakpoints.function_bps.with(|b| b.len()),
            "debug_state": format!("{:?}", app.vm.debug.state.get()),
            "debug_line": app.vm.debug.current_line.get(),
            "plot_anim": plot_anim,
            "plot_kind": plot_kind,
            "status": app.vm.status_bar.state.with(|s| s.message.clone()),
            "sidebar_visible": app.vm.layout.sidebar_visible.get(),
            "right_visible": app.vm.layout.workspace_visible.get(),
            "gutter_rect": ACTIVE_GUTTER.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "repl_entry_rect": REPL_ENTRY.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "workspace_table_rect": WORKSPACE_TABLE.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "plots_add_rect": PLOTS_ADD.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "search_entry_rect": SEARCH_ENTRY.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "watch_entry_rect": WATCH_ENTRY.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "plots_play_rect": PLOTS_PLAY.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "explorer_list_rect": EXPLORER_LIST.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "debug_next_rect": DEBUG_NEXT.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "debug_continue_rect": DEBUG_CONTINUE.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "flowchart_palette_rect": FLOWCHART_PALETTE.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "scene3d_button_rect": SCENE3D_BUTTON.with(|c| c.borrow().as_ref().and_then(rect_in_window)),
            "scene3d_generated": SCENE3D_GENERATED.with(|c| c.borrow().clone()),
            "flowchart": FLOWCHART_PROBE.with(|c| c.borrow().as_ref().map(|f| f())),
            "mflowlink": MFLOWLINK_PROBE.with(|c| c.borrow().as_ref().map(|f| f())),
            "statechart": STATECHART_PROBE.with(|c| c.borrow().as_ref().map(|f| f())),
        });

        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, snap.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
        glib::ControlFlow::Continue
    });
}
