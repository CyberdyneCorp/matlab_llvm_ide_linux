//! Shared application state: the `MainViewModel`, resolved settings, and the
//! live REPL / DAP sessions. Also hosts the client-side DAP protocol driver that
//! turns decoded adapter messages into `DebugViewModel` updates.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::{json, Value};

use matforge_core::models::{ConsoleLevel, DapStackFrame, DapVariable};
use matforge_core::services::dap::{parse_message, DapMessage};
use matforge_core::services::settings::Settings;
use matforge_core::viewmodels::MainViewModel;

use crate::process::{DapSession, ReplSession};

pub struct AppState {
    pub vm: Rc<MainViewModel>,
    pub settings: Settings,
    pub repl: RefCell<Option<ReplSession>>,
    pub dap: RefCell<Option<DapSession>>,
    dbg_frames: RefCell<Vec<DapStackFrame>>,
    dbg_thread: RefCell<i64>,
    dbg_file: RefCell<Option<PathBuf>>,
    /// Pending watch evaluations: DAP request seq → expression.
    dbg_watch_pending: RefCell<HashMap<i64, String>>,
}

impl AppState {
    pub fn new(vm: Rc<MainViewModel>, settings: Settings) -> Rc<AppState> {
        Rc::new(AppState {
            vm,
            settings,
            repl: RefCell::new(None),
            dap: RefCell::new(None),
            dbg_frames: RefCell::new(Vec::new()),
            dbg_thread: RefCell::new(1),
            dbg_file: RefCell::new(None),
            dbg_watch_pending: RefCell::new(HashMap::new()),
        })
    }

    fn matlabc_ok(&self) -> bool {
        self.settings.matlabc_path.exists()
    }

    // ---- Live REPL ---------------------------------------------------------

    /// Ensure a `matlabc -repl` process is running. Returns whether one is live.
    pub fn ensure_repl(self: &Rc<Self>) -> bool {
        if self.repl.borrow().is_some() {
            return true;
        }
        if !self.matlabc_ok() {
            self.vm.console.log(ConsoleLevel::Error, "matlabc not found — cannot start REPL");
            return false;
        }
        let cwd = self.vm.project.root_url.get().unwrap_or_else(std::env::temp_dir);
        let app = self.clone();
        match ReplSession::start(&self.settings.matlabc_path, &cwd, move |line| {
            app.vm.feed_repl_line(&line);
        }) {
            Ok(session) => {
                *self.repl.borrow_mut() = Some(session);
                self.vm.repl.set_running(true);
                true
            }
            Err(e) => {
                self.vm.console.log(ConsoleLevel::Error, format!("REPL failed to start: {e}"));
                false
            }
        }
    }

    /// Send a command to the live REPL (starting it on first use).
    pub fn repl_send(self: &Rc<Self>, command: &str) {
        if self.ensure_repl() {
            if let Some(session) = self.repl.borrow_mut().as_mut() {
                if let Err(e) = session.send(command) {
                    self.vm.console.log(ConsoleLevel::Error, format!("REPL write failed: {e}"));
                }
            }
        }
    }

    /// Select a workspace variable and capture its value into the Matrix Viewer
    /// via a sentinel-wrapped `disp`. The value block is routed to the inspector
    /// by `MainViewModel::feed_repl_line`.
    pub fn inspect_variable(self: &Rc<Self>, name: &str) {
        self.vm.workspace.select(name);
        // Guard: `disp(struct)` (and other non-matrix classes) currently crashes
        // the matlabc REPL, so don't probe variables the inspector can't parse.
        let dtype = self
            .vm
            .workspace
            .variables
            .with(|vs| vs.iter().find(|v| v.name == name).map(|v| v.dtype.clone()));
        if let Some(dt) = dtype {
            if !dt.is_inspectable_matrix() {
                self.vm.status_bar.set_message(format!(
                    "{name}: {} values can't be inspected yet",
                    dt.display_name()
                ));
                return;
            }
        }
        if self.ensure_repl() {
            if let Some(session) = self.repl.borrow_mut().as_mut() {
                let probe = format!(
                    "disp('___MF_VAL_BEGIN___'); disp({name}); disp('___MF_VAL_END___')"
                );
                let _ = session.eval(&probe);
            }
        }
    }

    /// Plot a named workspace variable as `kind` (the "Plot As" context menu).
    /// Captures the value over the REPL; the figure is created when it arrives.
    pub fn plot_variable_as(self: &Rc<Self>, name: &str, kind: matforge_core::models::PlotKind) {
        self.vm.request_plot(name, kind);
        self.inspect_variable(name);
    }

    /// Plot the currently inspected workspace variable as a line series.
    pub fn plot_inspected(self: &Rc<Self>) {
        use matforge_core::models::{PlotFigure, PlotKind};
        let Some(m) = self.vm.workspace.inspected_matrix.get() else {
            self.vm.status_bar.set_message("Click a workspace variable first, then +");
            return;
        };
        let ys: Vec<f64> = m.cells.iter().flatten().copied().collect();
        if ys.is_empty() {
            return;
        }
        let xs: Vec<f64> = (0..ys.len()).map(|i| i as f64).collect();
        let index = self.vm.plots.figures.with(|f| f.len() as i32) + 1;
        let fig = PlotFigure::series(index, m.title.clone(), PlotKind::Line2D, xs, ys)
            .with_source(m.title.clone());
        self.vm.plots.add(fig);
    }

    // ---- DAP debugger ------------------------------------------------------

    /// Launch a debug session on the active tab's file.
    pub fn start_debug(self: &Rc<Self>) {
        let Some(tab) = self.vm.editor.active_tab() else {
            self.vm.status_bar.set_message("Open a file to debug");
            return;
        };
        let Some(file) = tab.url else {
            self.vm.status_bar.set_message("Save the file before debugging");
            return;
        };
        if !self.matlabc_ok() {
            self.vm.console.log(ConsoleLevel::Error, "matlabc not found — cannot debug");
            return;
        }
        self.stop_debug();
        self.vm.activity_bar.select(matforge_core::viewmodels::ActivityItem::Debug);
        self.vm.debug.launch();
        self.vm.toolbar.is_debugging.set(true);
        self.vm.console.log(ConsoleLevel::Info, format!("debug: launching {}", file.display()));
        *self.dbg_file.borrow_mut() = Some(file.clone());

        let app = self.clone();
        match DapSession::start(&self.settings.matlabc_path, &file, move |body| {
            app.on_dap_body(&body);
        }) {
            Ok(session) => {
                *self.dap.borrow_mut() = Some(session);
                self.send_request(
                    "initialize",
                    Some(json!({
                        "clientID": "matforge",
                        "adapterID": "matlabc",
                        "linesStartAt1": true,
                        "columnsStartAt1": true,
                        "pathFormat": "path"
                    })),
                );
                self.vm.status_bar.set_message("Debugging…");
            }
            Err(e) => self.vm.console.log(ConsoleLevel::Error, format!("debug failed to start: {e}")),
        }
    }

    /// Stepping / control command (`continue`, `next`, `stepIn`, `stepOut`,
    /// `pause`, `stepBack`).
    pub fn debug_command(self: &Rc<Self>, command: &str) {
        let thread_id = *self.dbg_thread.borrow();
        self.send_request(command, Some(json!({ "threadId": thread_id })));
        if command != "pause" {
            self.vm.debug.on_running();
            self.vm.editor.clear_execution_lines();
        }
    }

    /// Evaluate a watch expression against the paused top frame.
    pub fn evaluate_watch(self: &Rc<Self>, expr: &str) {
        let Some(fid) = self.dbg_frames.borrow().first().map(|f| f.id) else {
            self.vm.console.log(ConsoleLevel::Warning, "Not paused — can't evaluate a watch");
            return;
        };
        if let Some(session) = self.dap.borrow_mut().as_mut() {
            let frame = session.client.request(
                "evaluate",
                Some(json!({ "expression": expr, "frameId": fid, "context": "watch" })),
            );
            let seq = session.client.last_seq();
            let _ = session.write_frame(&frame);
            self.dbg_watch_pending.borrow_mut().insert(seq, expr.to_string());
        }
    }

    /// Tear down any running debug session.
    pub fn stop_debug(self: &Rc<Self>) {
        if self.dap.borrow().is_some() {
            self.send_request("disconnect", Some(json!({ "terminateDebuggee": true })));
        }
        *self.dap.borrow_mut() = None;
        self.vm.debug.terminate();
        self.vm.editor.clear_execution_lines();
        self.vm.toolbar.is_debugging.set(false);
        self.vm.workspace.live.set(false);
    }

    /// Re-send the active tab's breakpoints if a debug session is live.
    pub fn refresh_breakpoints(self: &Rc<Self>) {
        if self.dap.borrow().is_some() {
            self.send_breakpoints();
        }
    }

    /// Push the function breakpoints to a live adapter.
    pub fn send_function_breakpoints(self: &Rc<Self>) {
        if self.dap.borrow().is_none() {
            return;
        }
        let bps: Vec<Value> = self.vm.breakpoints.function_bps.with(|list| {
            list.iter()
                .filter(|bp| bp.enabled)
                .map(|bp| {
                    let mut v = json!({ "name": bp.name });
                    if let Some(c) = &bp.condition {
                        v["condition"] = json!(c);
                    }
                    v
                })
                .collect()
        });
        self.send_request("setFunctionBreakpoints", Some(json!({ "breakpoints": bps })));
    }

    /// Push the enabled exception filters to a live adapter.
    pub fn send_exception_breakpoints(self: &Rc<Self>) {
        if self.dap.borrow().is_none() {
            return;
        }
        let filters: Vec<String> = self.vm.breakpoints.exception_filters.with(|list| {
            list.iter().filter(|f| f.enabled).map(|f| f.filter.clone()).collect()
        });
        self.send_request("setExceptionBreakpoints", Some(json!({ "filters": filters })));
    }

    fn send_request(self: &Rc<Self>, command: &str, args: Option<Value>) {
        if let Some(session) = self.dap.borrow_mut().as_mut() {
            let frame = session.client.request(command, args);
            let _ = session.write_frame(&frame);
        }
    }

    fn on_dap_body(self: &Rc<Self>, body: &str) {
        if body == crate::process::DAP_EXIT {
            if matches!(
                self.vm.debug.state.get(),
                matforge_core::viewmodels::DebugState::Launching
                    | matforge_core::viewmodels::DebugState::Running
            ) {
                self.vm.console.log(
                    ConsoleLevel::Error,
                    "debug adapter exited unexpectedly (matlabc -dap crashed)",
                );
            }
            self.vm.status_bar.set_message("Debugger exited");
            self.stop_debug();
            return;
        }
        match parse_message(body) {
            Some(DapMessage::Response { request_seq, command, success, body }) => {
                self.on_dap_response(request_seq, &command, success, &body)
            }
            Some(DapMessage::Event { event, body }) => self.on_dap_event(&event, &body),
            _ => {}
        }
    }

    fn on_dap_response(self: &Rc<Self>, request_seq: i64, command: &str, success: bool, body: &Value) {
        if !success && command != "disconnect" {
            // A failed watch still resolves its pending slot.
            self.dbg_watch_pending.borrow_mut().remove(&request_seq);
            return;
        }
        match command {
            "evaluate" => {
                if let Some(expr) = self.dbg_watch_pending.borrow_mut().remove(&request_seq) {
                    let result = body.get("result").and_then(Value::as_str).unwrap_or("").to_string();
                    self.vm.debug.add_evaluation(expr, result);
                }
            }
            "initialize" => {
                let data_bp = body.get("supportsDataBreakpoints").and_then(Value::as_bool).unwrap_or(false);
                let step_back = body.get("supportsStepBack").and_then(Value::as_bool).unwrap_or(false);
                self.vm.debug.set_capabilities(data_bp, step_back);
                let program = self.dbg_file.borrow().clone().unwrap_or_default();
                self.send_request(
                    "launch",
                    Some(json!({ "program": program.to_string_lossy(), "stopOnEntry": true })),
                );
            }
            "stackTrace" => {
                let frames = parse_frames(body);
                *self.dbg_frames.borrow_mut() = frames.clone();
                match frames.first() {
                    Some(top) => self.send_request("scopes", Some(json!({ "frameId": top.id }))),
                    None => self.vm.debug.on_stopped(frames, vec![]),
                }
            }
            "scopes" => match locals_reference(body) {
                Some(r) => self.send_request("variables", Some(json!({ "variablesReference": r }))),
                None => {
                    let frames = self.dbg_frames.borrow().clone();
                    self.vm.debug.on_stopped(frames, vec![]);
                }
            },
            "variables" => {
                let locals = parse_variables(body);
                let frames = self.dbg_frames.borrow().clone();
                if let Some(top) = frames.first() {
                    self.mark_exec_line(top);
                }
                // Mirror the frame's locals into the Workspace table too, like
                // the macOS reference (in addition to the Debug panel's Locals).
                self.vm.workspace.set_from_debug_locals(&locals);
                self.vm.debug.on_stopped(frames, locals);
            }
            _ => {}
        }
    }

    fn on_dap_event(self: &Rc<Self>, event: &str, body: &Value) {
        match event {
            "initialized" => {
                self.send_breakpoints();
                self.send_request("configurationDone", None);
            }
            "stopped" => {
                let tid = body.get("threadId").and_then(Value::as_i64).unwrap_or(1);
                *self.dbg_thread.borrow_mut() = tid;
                self.send_request("stackTrace", Some(json!({ "threadId": tid })));
            }
            "continued" => {
                self.vm.debug.on_running();
                self.vm.editor.clear_execution_lines();
            }
            "output" => {
                if let Some(text) = body.get("output").and_then(Value::as_str) {
                    for line in text.lines() {
                        self.vm.console.log(ConsoleLevel::Debug, line.to_string());
                    }
                }
            }
            "terminated" | "exited" => {
                self.vm.status_bar.set_message("Debug session ended");
                self.stop_debug();
            }
            _ => {}
        }
    }

    fn send_breakpoints(self: &Rc<Self>) {
        let Some(file) = self.dbg_file.borrow().clone() else { return };
        let Some(tab) = self.vm.editor.active_tab() else { return };
        let lines: Vec<Value> = tab
            .breakpoints
            .iter()
            .map(|(line, cfg)| {
                let mut bp = json!({ "line": line });
                if let Some(c) = &cfg.condition {
                    bp["condition"] = json!(c);
                }
                if let Some(m) = &cfg.log_message {
                    bp["logMessage"] = json!(m);
                }
                if let Some(h) = &cfg.hit_condition {
                    bp["hitCondition"] = json!(h);
                }
                bp
            })
            .collect();
        self.send_request(
            "setBreakpoints",
            Some(json!({
                "source": { "path": file.to_string_lossy() },
                "breakpoints": lines
            })),
        );
    }

    fn mark_exec_line(self: &Rc<Self>, frame: &DapStackFrame) {
        let Some(line) = frame.line else { return };
        let Some(tab) = self.vm.editor.active_tab() else { return };
        // Only mark when the paused source matches the active tab.
        let matches = match (&frame.source_path, &tab.url) {
            (Some(sp), Some(url)) => url.to_string_lossy() == *sp,
            _ => true,
        };
        if matches {
            self.vm.editor.set_execution_line(tab.id, Some(line));
        }
    }
}

// ---- DAP JSON parsing ------------------------------------------------------

fn parse_frames(body: &Value) -> Vec<DapStackFrame> {
    body.get("stackFrames")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|f| DapStackFrame {
                    id: f.get("id").and_then(Value::as_i64).unwrap_or(0),
                    name: f.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    source_path: f
                        .get("source")
                        .and_then(|s| s.get("path"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    line: f.get("line").and_then(Value::as_i64).map(|l| l as usize),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn locals_reference(body: &Value) -> Option<i64> {
    let scopes = body.get("scopes")?.as_array()?;
    // Prefer a scope literally named "Locals"; fall back to the first.
    scopes
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str).map(|n| n.eq_ignore_ascii_case("locals")).unwrap_or(false))
        .or_else(|| scopes.first())
        .and_then(|s| s.get("variablesReference").and_then(Value::as_i64))
        .filter(|r| *r != 0)
}

fn parse_variables(body: &Value) -> Vec<DapVariable> {
    body.get("variables")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| DapVariable {
                    name: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                    value: v.get("value").and_then(Value::as_str).unwrap_or("").to_string(),
                    type_hint: v.get("type").and_then(Value::as_str).map(str::to_string),
                    variables_reference: v.get("variablesReference").and_then(Value::as_i64).unwrap_or(0),
                    indexed_variables: v.get("indexedVariables").and_then(Value::as_i64),
                    named_variables: v.get("namedVariables").and_then(Value::as_i64),
                })
                .collect()
        })
        .unwrap_or_default()
}
