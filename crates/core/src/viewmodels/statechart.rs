//! Standalone mStateflow (state-machine) view model. Holds the chart document,
//! the transport state, the streamed event log from `matlabc -emit-trace`, and
//! the set of currently-active states (for highlighting the chart). The GTK
//! window owns the subprocess and calls the verb methods here.

use std::cell::Cell;
use std::collections::BTreeSet;

use crate::models::flowchart::{state_tree, FlowchartDocument, StateTreeNode};
use crate::observable::Property;
use crate::services::chart_trace::{parse_chart_event, ChartEvent};
use crate::viewmodels::mflowlink::SimState;

/// One chart event tagged with the super-step index ("sim time") it occurred in.
#[derive(Clone, Debug, PartialEq)]
pub struct LoggedEvent {
    pub step: i64,
    pub event: ChartEvent,
}

pub struct StateChartViewModel {
    pub document: Property<FlowchartDocument>,
    pub events: Property<Vec<LoggedEvent>>,
    pub active_states: Property<BTreeSet<String>>,
    /// State the user clicked in the event log to reveal on the canvas.
    pub revealed: Property<Option<String>>,
    pub state: Property<SimState>,
    /// True while driven by a live `--sim-dap` chart session (vs one-shot
    /// `-emit-trace`).
    pub live: Property<bool>,
    /// Current super-step index, advanced by `superStep*` events and stamped
    /// onto every logged event.
    step: Cell<i64>,
}

impl StateChartViewModel {
    pub fn new(document: FlowchartDocument) -> StateChartViewModel {
        StateChartViewModel {
            document: Property::new(document),
            events: Property::new(Vec::new()),
            active_states: Property::new(BTreeSet::new()),
            revealed: Property::new(None),
            state: Property::new(SimState::Idle),
            live: Property::new(false),
            step: Cell::new(0),
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
        // Advance the super-step "clock" so each event carries a sim-time index.
        match &event {
            ChartEvent::SuperStepBegin { iteration }
            | ChartEvent::SuperStepEnd { iteration, .. } => {
                self.step.set(*iteration);
            }
            _ => {}
        }
        let step = self.step.get();
        self.events.update(|e| e.push(LoggedEvent { step, event }));
    }

    /// Reveal a state on the canvas (clicked in the event log). `None` clears it.
    pub fn reveal(&self, id: Option<&str>) {
        self.revealed.set(id.map(str::to_string));
    }

    /// The chart's state hierarchy as a tree (for the active-state pane).
    pub fn state_tree(&self) -> Vec<StateTreeNode> {
        self.document
            .with(|d| d.flows.first().map(state_tree).unwrap_or_default())
    }

    /// The event log as CSV (`step,kind,detail`), including a header row.
    pub fn events_csv(&self) -> String {
        let mut out = String::from("step,kind,detail\n");
        self.events.with(|events| {
            for logged in events {
                let (kind, detail) = logged.event.csv_fields();
                // detail never contains a comma we need to quote (ids/kinds only).
                out.push_str(&format!("{},{},{}\n", logged.step, kind, detail));
            }
        });
        out
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
        self.revealed.set(None);
        self.state.set(SimState::Idle);
        self.live.set(false);
        self.step.set(0);
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

    #[test]
    fn events_carry_super_step_index_and_export_csv() {
        let vm = vm();
        vm.start();
        vm.feed_line(r#"{"kind":"superStepBegin","iteration":0}"#);
        vm.feed_line(r#"{"kind":"stateEnter","id":"A"}"#);
        vm.feed_line(r#"{"kind":"superStepBegin","iteration":1}"#);
        vm.feed_line(r#"{"kind":"transitionFired","id":"t","src":"A","dst":"B"}"#);
        let steps: Vec<i64> = vm.events.with(|e| e.iter().map(|l| l.step).collect());
        assert_eq!(steps, vec![0, 0, 1, 1]); // enter A tagged step 0, transition step 1

        let csv = vm.events_csv();
        assert!(csv.starts_with("step,kind,detail\n"));
        assert!(csv.contains("0,stateEnter,A\n"));
        assert!(csv.contains("1,transitionFired,t:A->B\n"));

        // Reveal round-trips and clears.
        vm.reveal(Some("B"));
        assert_eq!(vm.revealed.get().as_deref(), Some("B"));
        vm.reveal(None);
        assert!(vm.revealed.get().is_none());
    }
}
