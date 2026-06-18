//! Standalone mflowLink (signal-flow simulation) view model. Holds the opened
//! signal-flow document, the transport state, and the live [`SimTrace`] fed from
//! `matlabc -simulate` output. The GTK window subscribes to `trace`/`state` and
//! renders scope tiles; it owns the subprocess and calls the verb methods here.

use std::collections::BTreeMap;

use crate::models::flowchart::FlowchartDocument;
use crate::observable::Property;
use crate::services::sim_dap::SimEvent;
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
    /// Playback position: how many samples the scopes currently show. Follows
    /// the live trace while running; afterwards Play/Step/Rewind scrub it.
    pub cursor: Property<usize>,
    /// True while driven by a live `--sim-dap` session (vs one-shot CSV replay).
    pub live: Property<bool>,
    /// Latest simulation clock (live mode), from `simulationTime` events.
    pub sim_time: Property<f64>,
    /// Latest major-step index (live mode).
    pub major_step: Property<i64>,
    /// The block currently being evaluated (drives the active-block halo).
    pub active_block: Property<Option<String>>,
    /// Live mode: per-edge `(t, value)` samples from `signalSample` events,
    /// keyed by edge id (insertion-ordered by `BTreeMap`). Feeds the scopes.
    pub live_signals: Property<BTreeMap<String, Vec<(f64, f64)>>>,
    /// Snapshot-ring entries `(major_step, depth)` from `snapshotTaken` events.
    pub snapshots: Property<Vec<(i64, i64)>>,
    /// Why the live run last paused (`stopped` reason), for the transport row.
    pub stop_reason: Property<Option<String>>,
}

impl MflowLinkViewModel {
    pub fn new(document: FlowchartDocument) -> MflowLinkViewModel {
        MflowLinkViewModel {
            document: Property::new(document),
            trace: Property::new(SimTrace::new()),
            state: Property::new(SimState::Idle),
            sample_count: Property::new(0),
            cursor: Property::new(0),
            live: Property::new(false),
            sim_time: Property::new(0.0),
            major_step: Property::new(0),
            active_block: Property::new(None),
            live_signals: Property::new(BTreeMap::new()),
            snapshots: Property::new(Vec::new()),
            stop_reason: Property::new(None),
        }
    }

    /// Begin a live `--sim-dap` session: clears any prior trace and arms the
    /// transport. Events arrive via [`on_sim_event`](Self::on_sim_event).
    pub fn start_live(&self) {
        self.reset();
        self.live.set(true);
        self.state.set(SimState::Running);
    }

    /// Fold one live simulation event into the transport/inspection state.
    pub fn on_sim_event(&self, ev: &SimEvent) {
        match ev {
            SimEvent::Time { t, major_step } => {
                self.sim_time.set(*t);
                self.major_step.set(*major_step);
            }
            SimEvent::ActiveBlock { node_id } => {
                self.active_block.set(Some(node_id.clone()));
            }
            SimEvent::Stopped { reason } => {
                // The runtime paused (entry / breakpoint / step / pause /
                // crossing). Settle into Paused unless we never started.
                if self.state.get() != SimState::Idle {
                    self.state.set(SimState::Paused);
                    self.stop_reason.set(Some(reason.clone()));
                }
            }
            SimEvent::Signal { edge_id, t, value } => {
                self.live_signals
                    .update(|m| m.entry(edge_id.clone()).or_default().push((*t, *value)));
                // Drive the scope redraw subscription (shared with CSV mode).
                self.sample_count.update(|c| *c += 1);
            }
            SimEvent::Snapshot { major_step, depth } => {
                self.snapshots.update(|s| s.push((*major_step, *depth)));
            }
            // Zero-crossings are surfaced by a later slice.
            SimEvent::ZeroCrossing { .. } => {}
        }
    }

    pub fn total_samples(&self) -> usize {
        self.trace.with(|t| t.rows.len())
    }

    /// Advance the playback cursor by one sample (clamped to the trace length).
    pub fn step(&self) {
        let total = self.total_samples();
        self.cursor.update(|c| *c = (*c + 1).min(total));
    }

    /// Move the playback cursor to `n` (clamped).
    pub fn set_cursor(&self, n: usize) {
        let total = self.total_samples();
        self.cursor.set(n.min(total));
    }

    /// Rewind playback to the start (keeps the collected trace).
    pub fn rewind(&self) {
        self.cursor.set(0);
    }

    /// True once the cursor has reached the end of the trace.
    pub fn at_end(&self) -> bool {
        self.cursor.get() >= self.total_samples()
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
            // While collecting, the cursor tracks the live edge so scopes fill
            // in real time; after Finished it can be scrubbed independently.
            if self.state.get() == SimState::Running {
                self.cursor.set(n);
            }
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
        self.cursor.set(0);
        self.state.set(SimState::Idle);
        self.live.set(false);
        self.sim_time.set(0.0);
        self.major_step.set(0);
        self.active_block.set(None);
        self.live_signals.set(BTreeMap::new());
        self.snapshots.set(Vec::new());
        self.stop_reason.set(None);
    }

    /// Number of plotted signals — live edges in live mode, else trace columns.
    pub fn signal_count(&self) -> usize {
        if self.live.get() {
            self.live_signals.with(BTreeMap::len)
        } else {
            self.trace.with(SimTrace::signal_count)
        }
    }

    /// Name of scope signal `i` (edge id in live mode, else trace column).
    pub fn scope_name(&self, i: usize) -> Option<String> {
        if self.live.get() {
            self.live_signals.with(|m| m.keys().nth(i).cloned())
        } else {
            self.trace.with(|t| t.signal_name(i).map(str::to_string))
        }
    }

    /// `(time, value)` series for scope signal `i`.
    pub fn scope_series(&self, i: usize) -> (Vec<f64>, Vec<f64>) {
        if self.live.get() {
            self.live_signals.with(|m| {
                m.values()
                    .nth(i)
                    .map(|s| s.iter().copied().unzip())
                    .unwrap_or_default()
            })
        } else {
            self.trace.with(|t| t.series(i))
        }
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
        assert_eq!(vm.cursor.get(), 0);
    }

    #[test]
    fn cursor_follows_live_then_scrubs() {
        let vm = vm();
        vm.start();
        vm.feed_line("t,a");
        vm.feed_line("0.0,1.0");
        vm.feed_line("0.1,2.0");
        vm.feed_line("0.2,3.0");
        // While running, the cursor tracks the live edge.
        assert_eq!(vm.cursor.get(), 3);
        assert_eq!(vm.total_samples(), 3);
        vm.finish();
        // Rewind + step scrubs independently, clamped at the end.
        vm.rewind();
        assert_eq!(vm.cursor.get(), 0);
        vm.step();
        vm.step();
        assert_eq!(vm.cursor.get(), 2);
        assert!(!vm.at_end());
        vm.step();
        vm.step(); // clamps
        assert_eq!(vm.cursor.get(), 3);
        assert!(vm.at_end());
        vm.set_cursor(99);
        assert_eq!(vm.cursor.get(), 3);
    }

    #[test]
    fn live_session_folds_events_into_state() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        assert!(vm.live.get());
        assert_eq!(vm.state.get(), SimState::Running);

        vm.on_sim_event(&SimEvent::Time {
            t: 1.25,
            major_step: 7,
        });
        assert_eq!(vm.sim_time.get(), 1.25);
        assert_eq!(vm.major_step.get(), 7);

        vm.on_sim_event(&SimEvent::ActiveBlock {
            node_id: "gain_1".into(),
        });
        assert_eq!(vm.active_block.get().as_deref(), Some("gain_1"));

        // A stopped event (breakpoint / step / entry) pauses the transport.
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "breakpoint".into(),
        });
        assert_eq!(vm.state.get(), SimState::Paused);

        // Reset clears the live state back to Idle.
        vm.reset();
        assert!(!vm.live.get());
        assert_eq!(vm.sim_time.get(), 0.0);
        assert!(vm.active_block.get().is_none());
    }

    #[test]
    fn stopped_event_ignored_when_idle() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "entry".into(),
        });
        assert_eq!(vm.state.get(), SimState::Idle);
    }

    #[test]
    fn live_signal_samples_accumulate_into_scopes() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        let sig = |e: &str, t, v| SimEvent::Signal {
            edge_id: e.into(),
            t,
            value: v,
        };
        vm.on_sim_event(&sig("scope", 0.0, 1.0));
        vm.on_sim_event(&sig("src", 0.0, 5.0));
        vm.on_sim_event(&sig("scope", 0.1, 2.0));
        // Two distinct edges → two scope signals (BTreeMap order: scope, src).
        assert_eq!(vm.signal_count(), 2);
        assert_eq!(vm.scope_name(0).as_deref(), Some("scope"));
        let (xs, ys) = vm.scope_series(0);
        assert_eq!(xs, vec![0.0, 0.1]);
        assert_eq!(ys, vec![1.0, 2.0]);
        // sample_count is bumped so the scope panel redraws.
        assert_eq!(vm.sample_count.get(), 3);
        // Reset clears the live buffer.
        vm.reset();
        assert_eq!(vm.signal_count(), 0);
    }

    #[test]
    fn snapshots_and_stop_reason_track_events() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        vm.on_sim_event(&SimEvent::Snapshot {
            major_step: 10,
            depth: 1,
        });
        vm.on_sim_event(&SimEvent::Snapshot {
            major_step: 20,
            depth: 2,
        });
        assert_eq!(vm.snapshots.get(), vec![(10, 1), (20, 2)]);
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "breakpoint".into(),
        });
        assert_eq!(vm.stop_reason.get().as_deref(), Some("breakpoint"));
        assert_eq!(vm.state.get(), SimState::Paused);
        vm.reset();
        assert!(vm.snapshots.get().is_empty());
        assert!(vm.stop_reason.get().is_none());
    }
}
