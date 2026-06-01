//! Debug session view model — the client-side DAP state machine. Mirrors
//! `DebugViewModel`: the session state, call stack, locals, current execution
//! line/source, watch evaluations, and adapter capabilities. The transport
//! itself lives in the app; this VM is driven by decoded DAP events and is
//! tested by feeding those events directly.

use crate::models::{DapEvaluation, DapStackFrame, DapVariable};
use crate::observable::Property;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DebugState {
    Idle,
    Launching,
    Running,
    Paused,
    Terminated,
}

pub struct DebugViewModel {
    pub state: Property<DebugState>,
    pub stack_frames: Property<Vec<DapStackFrame>>,
    pub locals: Property<Vec<DapVariable>>,
    pub current_line: Property<Option<usize>>,
    pub current_source: Property<Option<String>>,
    pub evaluations: Property<Vec<DapEvaluation>>,
    pub supports_data_breakpoints: Property<bool>,
    pub supports_step_back: Property<bool>,
}

impl Default for DebugViewModel {
    fn default() -> Self {
        DebugViewModel::new()
    }
}

impl DebugViewModel {
    pub fn new() -> DebugViewModel {
        DebugViewModel {
            state: Property::new(DebugState::Idle),
            stack_frames: Property::new(Vec::new()),
            locals: Property::new(Vec::new()),
            current_line: Property::new(None),
            current_source: Property::new(None),
            evaluations: Property::new(Vec::new()),
            supports_data_breakpoints: Property::new(false),
            supports_step_back: Property::new(false),
        }
    }

    /// Begin a launch handshake.
    pub fn launch(&self) {
        self.state.set(DebugState::Launching);
        self.evaluations.set(Vec::new());
    }

    /// Record adapter capabilities from the `initialize` response.
    pub fn set_capabilities(&self, data_breakpoints: bool, step_back: bool) {
        self.supports_data_breakpoints.set(data_breakpoints);
        self.supports_step_back.set(step_back);
    }

    /// The program resumed — clear the paused-line marker.
    pub fn on_running(&self) {
        self.state.set(DebugState::Running);
        self.current_line.set(None);
    }

    /// A `stopped` event landed with a fetched stack + locals. The top frame
    /// drives the execution-line marker.
    pub fn on_stopped(&self, frames: Vec<DapStackFrame>, locals: Vec<DapVariable>) {
        let top = frames.first();
        self.current_line.set(top.and_then(|f| f.line));
        self.current_source.set(top.and_then(|f| f.source_path.clone()));
        self.stack_frames.set(frames);
        self.locals.set(locals);
        self.state.set(DebugState::Paused);
    }

    /// Record a watch-box evaluation result.
    pub fn add_evaluation(&self, expression: impl Into<String>, result: impl Into<String>) {
        self.evaluations.update(|e| e.push(DapEvaluation::new(expression, result)));
    }

    /// The session ended — clear transient state.
    pub fn terminate(&self) {
        self.state.set(DebugState::Terminated);
        self.stack_frames.set(Vec::new());
        self.locals.set(Vec::new());
        self.current_line.set(None);
        self.current_source.set(None);
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state.get(), DebugState::Launching | DebugState::Running | DebugState::Paused)
    }

    pub fn is_paused(&self) -> bool {
        self.state.get() == DebugState::Paused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(line: usize) -> DapStackFrame {
        DapStackFrame { id: 1, name: "main".into(), source_path: Some("/p/a.m".into()), line: Some(line) }
    }

    #[test]
    fn launch_sets_launching() {
        let vm = DebugViewModel::new();
        vm.launch();
        assert_eq!(vm.state.get(), DebugState::Launching);
        assert!(vm.is_active());
    }

    #[test]
    fn stopped_drives_execution_line_from_top_frame() {
        let vm = DebugViewModel::new();
        vm.launch();
        vm.on_stopped(vec![frame(7)], vec![DapVariable::scalar("x", "3")]);
        assert!(vm.is_paused());
        assert_eq!(vm.current_line.get(), Some(7));
        assert_eq!(vm.current_source.get().as_deref(), Some("/p/a.m"));
        assert_eq!(vm.locals.get().len(), 1);
    }

    #[test]
    fn running_clears_line() {
        let vm = DebugViewModel::new();
        vm.on_stopped(vec![frame(3)], vec![]);
        vm.on_running();
        assert_eq!(vm.state.get(), DebugState::Running);
        assert!(vm.current_line.get().is_none());
    }

    #[test]
    fn terminate_clears_state() {
        let vm = DebugViewModel::new();
        vm.on_stopped(vec![frame(3)], vec![DapVariable::scalar("x", "1")]);
        vm.terminate();
        assert_eq!(vm.state.get(), DebugState::Terminated);
        assert!(vm.stack_frames.get().is_empty());
        assert!(vm.current_line.get().is_none());
        assert!(!vm.is_active());
    }

    #[test]
    fn capabilities_recorded() {
        let vm = DebugViewModel::new();
        vm.set_capabilities(true, false);
        assert!(vm.supports_data_breakpoints.get());
        assert!(!vm.supports_step_back.get());
    }

    #[test]
    fn evaluations_accumulate() {
        let vm = DebugViewModel::new();
        vm.add_evaluation("x+1", "4");
        vm.add_evaluation("y", "[1 2 3]");
        assert_eq!(vm.evaluations.get().len(), 2);
        assert_eq!(vm.evaluations.get()[0].result, "4");
    }
}
