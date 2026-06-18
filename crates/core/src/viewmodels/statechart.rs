//! Standalone mStateflow (state-machine) view model. Holds the chart document,
//! the transport state, the streamed event log from `matlabc -emit-trace`, and
//! the set of currently-active states (for highlighting the chart). The GTK
//! window owns the subprocess and calls the verb methods here.

use std::collections::BTreeSet;

use crate::models::flowchart::FlowchartDocument;
use crate::observable::Property;
use crate::services::chart_trace::{parse_chart_event, ChartEvent};
use crate::viewmodels::mflowlink::SimState;

pub struct StateChartViewModel {
    pub document: Property<FlowchartDocument>,
    pub events: Property<Vec<ChartEvent>>,
    pub active_states: Property<BTreeSet<String>>,
    pub state: Property<SimState>,
    /// True while driven by a live `--sim-dap` chart session (vs one-shot
    /// `-emit-trace`).
    pub live: Property<bool>,
}

impl StateChartViewModel {
    pub fn new(document: FlowchartDocument) -> StateChartViewModel {
        StateChartViewModel {
            document: Property::new(document),
            events: Property::new(Vec::new()),
            active_states: Property::new(BTreeSet::new()),
            state: Property::new(SimState::Idle),
            live: Property::new(false),
        }
    }

    pub fn start(&self) {
        self.reset();
        self.state.set(SimState::Running);
    }

    /// Begin a live `--sim-dap` chart session: clears prior state and arms the
    /// transport. Events arrive via [`apply_event`](Self::apply_event).
    pub fn start_live(&self) {
        self.reset();
        self.live.set(true);
        self.state.set(SimState::Running);
    }

    /// Fold one chart event into the active-state set + event log. Shared by the
    /// one-shot `-emit-trace` path and the live `stateChart/*` DAP path.
    pub fn apply_event(&self, event: ChartEvent) {
        if let Some(id) = event.entered_state() {
            let id = id.to_string();
            self.active_states.update(|s| {
                s.insert(id);
            });
        }
        if let Some(id) = event.exited_state() {
            self.active_states.update(|s| {
                s.remove(id);
            });
        }
        self.events.update(|e| e.push(event));
    }

    /// Feed one `-emit-trace` line (one-shot path).
    pub fn feed_line(&self, line: &str) {
        if let Some(event) = parse_chart_event(line) {
            self.apply_event(event);
        }
    }

    pub fn finish(&self) {
        if self.state.get() != SimState::Idle {
            self.state.set(SimState::Finished);
        }
    }

    pub fn reset(&self) {
        self.events.set(Vec::new());
        self.active_states.set(BTreeSet::new());
        self.state.set(SimState::Idle);
        self.live.set(false);
    }

    pub fn event_count(&self) -> usize {
        self.events.with(Vec::len)
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.active_states.with(|s| s.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::SchemaKind;

    fn vm() -> StateChartViewModel {
        StateChartViewModel::new(FlowchartDocument::empty("chart", SchemaKind::StateChart))
    }

    #[test]
    fn enter_exit_tracks_active_states() {
        let vm = vm();
        vm.start();
        vm.feed_line(r#"{"kind":"stateEnter","id":"Charge"}"#);
        assert!(vm.is_active("Charge"));
        vm.feed_line(r#"{"kind":"stateExit","id":"Charge"}"#);
        vm.feed_line(r#"{"kind":"stateEnter","id":"Discharge"}"#);
        assert!(!vm.is_active("Charge"));
        assert!(vm.is_active("Discharge"));
        assert_eq!(vm.event_count(), 3);
    }

    #[test]
    fn transition_event_is_logged_without_changing_active_set() {
        let vm = vm();
        vm.start();
        vm.feed_line(r#"{"kind":"stateEnter","id":"A"}"#);
        vm.feed_line(r#"{"kind":"transitionFired","id":"t","src":"A","dst":"B"}"#);
        assert!(vm.is_active("A"));
        assert_eq!(vm.event_count(), 2);
    }

    #[test]
    fn live_session_applies_dap_chart_events() {
        use crate::services::chart_trace::ChartEvent;
        let vm = vm();
        vm.start_live();
        assert!(vm.live.get());
        assert_eq!(vm.state.get(), SimState::Running);
        // apply_event drives the same active-set logic as the -emit-trace path.
        vm.apply_event(ChartEvent::StateEnter { id: "Off".into() });
        assert!(vm.is_active("Off"));
        vm.apply_event(ChartEvent::StateExit { id: "Off".into() });
        vm.apply_event(ChartEvent::StateEnter { id: "On".into() });
        assert!(!vm.is_active("Off"));
        assert!(vm.is_active("On"));
        assert_eq!(vm.event_count(), 3);
        vm.reset();
        assert!(!vm.live.get());
        assert!(vm.active_states.get().is_empty());
    }

    #[test]
    fn non_event_lines_are_ignored() {
        let vm = vm();
        vm.start();
        vm.feed_line("ChartModel entry=battery");
        vm.feed_line("");
        assert_eq!(vm.event_count(), 0);
    }

    #[test]
    fn reset_clears_log_and_active() {
        let vm = vm();
        vm.start();
        vm.feed_line(r#"{"kind":"stateEnter","id":"A"}"#);
        vm.reset();
        assert_eq!(vm.event_count(), 0);
        assert!(!vm.is_active("A"));
        assert_eq!(vm.state.get(), SimState::Idle);
    }

    #[test]
    fn finish_only_from_active() {
        let vm = vm();
        vm.finish();
        assert_eq!(vm.state.get(), SimState::Idle);
        vm.start();
        vm.finish();
        assert_eq!(vm.state.get(), SimState::Finished);
    }
}
