//! Standalone mflowLink (signal-flow simulation) view model. Holds the opened
//! signal-flow document, the transport state, and the live [`SimTrace`] fed from
//! `matlabc -simulate` output. The GTK window subscribes to `trace`/`state` and
//! renders scope tiles; it owns the subprocess and calls the verb methods here.

use crate::models::flowchart::FlowchartDocument;
use crate::observable::Property;
use crate::services::sim_trace::SimTrace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimState {
    Idle,
    Running,
    Paused,
    Finished,
}

pub struct MflowLinkViewModel {
    pub document: Property<FlowchartDocument>,
    pub trace: Property<SimTrace>,
    pub state: Property<SimState>,
    /// Bumped on every appended sample so views can throttle redraws.
    pub sample_count: Property<usize>,
}

impl MflowLinkViewModel {
    pub fn new(document: FlowchartDocument) -> MflowLinkViewModel {
        MflowLinkViewModel {
            document: Property::new(document),
            trace: Property::new(SimTrace::new()),
            state: Property::new(SimState::Idle),
            sample_count: Property::new(0),
        }
    }

    /// Mark the simulation as started and clear any prior trace.
    pub fn start(&self) {
        self.reset();
        self.state.set(SimState::Running);
    }

    /// Feed one line of `-simulate` output; updates the trace + counters.
    pub fn feed_line(&self, line: &str) {
        let mut added = false;
        self.trace.update(|t| {
            added = t.feed_line(line);
        });
        if added {
            let n = self.trace.with(|t| t.rows.len());
            self.sample_count.set(n);
        }
    }

    pub fn pause(&self) {
        if self.state.get() == SimState::Running {
            self.state.set(SimState::Paused);
        }
    }

    pub fn resume(&self) {
        if self.state.get() == SimState::Paused {
            self.state.set(SimState::Running);
        }
    }

    /// The process exited (clean or killed) — settle into Finished unless reset.
    pub fn finish(&self) {
        if self.state.get() != SimState::Idle {
            self.state.set(SimState::Finished);
        }
    }

    /// Clear the trace and return to Idle.
    pub fn reset(&self) {
        self.trace.set(SimTrace::new());
        self.sample_count.set(0);
        self.state.set(SimState::Idle);
    }

    pub fn signal_count(&self) -> usize {
        self.trace.with(SimTrace::signal_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::SchemaKind;

    fn vm() -> MflowLinkViewModel {
        MflowLinkViewModel::new(FlowchartDocument::empty("sim", SchemaKind::SignalFlow))
    }

    #[test]
    fn start_runs_and_clears() {
        let vm = vm();
        vm.feed_line("t,a");
        vm.feed_line("0.0,1.0");
        assert_eq!(vm.sample_count.get(), 1);
        vm.start();
        assert_eq!(vm.state.get(), SimState::Running);
        assert_eq!(vm.sample_count.get(), 0); // reset by start
    }

    #[test]
    fn feed_line_updates_trace_and_counter() {
        let vm = vm();
        vm.start();
        vm.feed_line("t,src,scope");
        assert_eq!(vm.sample_count.get(), 0); // header only
        vm.feed_line("0.0,1.0,2.0");
        vm.feed_line("0.1,3.0,4.0");
        assert_eq!(vm.sample_count.get(), 2);
        assert_eq!(vm.signal_count(), 2);
    }

    #[test]
    fn pause_resume_transitions() {
        let vm = vm();
        vm.start();
        vm.pause();
        assert_eq!(vm.state.get(), SimState::Paused);
        vm.resume();
        assert_eq!(vm.state.get(), SimState::Running);
        // pause only applies while running
        vm.finish();
        vm.pause();
        assert_eq!(vm.state.get(), SimState::Finished);
    }

    #[test]
    fn finish_from_idle_stays_idle() {
        let vm = vm();
        vm.finish();
        assert_eq!(vm.state.get(), SimState::Idle);
        vm.start();
        vm.finish();
        assert_eq!(vm.state.get(), SimState::Finished);
    }

    #[test]
    fn reset_clears_everything() {
        let vm = vm();
        vm.start();
        vm.feed_line("t,a");
        vm.feed_line("0.0,9.0");
        vm.reset();
        assert_eq!(vm.state.get(), SimState::Idle);
        assert_eq!(vm.sample_count.get(), 0);
        assert_eq!(vm.signal_count(), 0);
    }
}
