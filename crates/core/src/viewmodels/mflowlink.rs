//! Standalone mflowLink (signal-flow simulation) view model. Holds the opened
//! signal-flow document, the transport state, and the live [`SimTrace`] fed from
//! `matlabc -simulate` output. The GTK window subscribes to `trace`/`state` and
//! renders scope tiles; it owns the subprocess and calls the verb methods here.

use std::collections::BTreeMap;

use crate::models::flowchart::FlowchartDocument;
use crate::observable::Property;
use crate::services::sim_dap::{SimEvent, SimRequest};
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
    /// When paused on a MATLAB Function source-line breakpoint: `(block_id,
    /// line)` parsed from the stop description (`<block_id>:<line>`). `None`
    /// otherwise.
    pub source_stop: Property<Option<(String, i64)>>,
    /// Body locals `(name, value)` captured at the current source-line stop,
    /// fetched via the DAP `scopes`/`variables` round-trip (#385).
    pub locals: Property<Vec<(String, String)>>,
}

/// Parse a `<block_id>:<line>` source-line stop description into its parts.
/// Returns `None` for any other description (signal breakpoints, `stopTime
/// reached`, …), which is how a source-line pause is told apart.
fn parse_source_loc(desc: &str) -> Option<(String, i64)> {
    let idx = desc.rfind(':')?;
    let line: i64 = desc[idx + 1..].trim().parse().ok()?;
    let block = desc[..idx].trim();
    (!block.is_empty() && line > 0).then(|| (block.to_string(), line))
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
            source_stop: Property::new(None),
            locals: Property::new(Vec::new()),
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
                // Set the step first: the clock label is bound to `sim_time` and
                // reads `major_step` in the same callback, so it must be current.
                self.major_step.set(*major_step);
                self.sim_time.set(*t);
            }
            SimEvent::ActiveBlock { node_id } => {
                self.active_block.set(Some(node_id.clone()));
            }
            SimEvent::Stopped {
                reason,
                description,
            } => {
                // The runtime paused (entry / breakpoint / step / pause /
                // crossing) or reached the end of the run. `"stopTime reached"`
                // is completion → Finished (so To Workspace export + the UI
                // settle); anything else is an interactive Paused.
                if self.state.get() != SimState::Idle {
                    if description.as_deref() == Some("stopTime reached") {
                        self.state.set(SimState::Finished);
                    } else {
                        self.state.set(SimState::Paused);
                    }
                    self.stop_reason
                        .set(Some(description.clone().unwrap_or_else(|| reason.clone())));
                    // A `breakpoint` or `step` stop whose description is
                    // `<block_id>:<line>` is a MATLAB Function source-line pause
                    // (#354) — the first hit is `breakpoint`, each statement step
                    // is `step` (#386). Record it so the UI can fetch + show body
                    // locals; any other stop (`function returned`, major steps,
                    // signal breakpoints) clears it.
                    let src = matches!(reason.as_str(), "breakpoint" | "step")
                        .then(|| description.as_deref().and_then(parse_source_loc))
                        .flatten();
                    if src.is_some() {
                        self.source_stop.set(src);
                    } else {
                        self.source_stop.set(None);
                        self.locals.set(Vec::new());
                    }
                }
            }
            SimEvent::Signal { block_id, t, value } => {
                self.live_signals.update(|m| {
                    let series = m.entry(block_id.clone()).or_default();
                    // A sample at or before the last one means the run rewound
                    // (step-back / reset): drop the now-invalid future samples
                    // so the scope trace stays monotonic in time.
                    if series.last().is_some_and(|&(lt, _)| *t <= lt) {
                        series.retain(|&(st, _)| st < *t);
                    }
                    series.push((*t, *value));
                });
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

    /// The step verb the transport's Step button sends for a live session: a
    /// statement step (DAP `next`) when paused inside a MATLAB Function body,
    /// else one major (solver) step — matching the compiler's #386 routing.
    pub fn live_step_request(&self) -> SimRequest {
        if self.source_stop.get().is_some() {
            SimRequest::StepStatement
        } else {
            SimRequest::StepMajor
        }
    }

    /// True while paused inside a MATLAB Function body, where Step Out (finish
    /// the body) is meaningful.
    pub fn can_step_out(&self) -> bool {
        self.source_stop.get().is_some()
    }

    /// Advance the playback cursor by one sample (clamped to the trace length).
    pub fn step(&self) {
        let total = self.total_samples();
        self.cursor.update(|c| *c = (*c + 1).min(total));
    }

    /// Move the playback cursor back one sample (clamped at the start).
    pub fn step_back(&self) {
        self.cursor.update(|c| *c = c.saturating_sub(1));
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
        // Leaving the pause clears any source-line stop + its locals.
        self.source_stop.set(None);
        self.locals.set(Vec::new());
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
        self.source_stop.set(None);
        self.locals.set(Vec::new());
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
    fn cursor_steps_forward_and_back_clamped() {
        let vm = vm();
        vm.start();
        vm.feed_line("t,scope");
        for r in ["0.0,1.0", "0.1,2.0", "0.2,3.0"] {
            vm.feed_line(r);
        }
        vm.set_cursor(0);
        vm.step();
        vm.step();
        assert_eq!(vm.cursor.get(), 2);
        vm.step_back();
        assert_eq!(vm.cursor.get(), 1);
        vm.step_back();
        vm.step_back(); // clamps at 0, no underflow
        assert_eq!(vm.cursor.get(), 0);
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
            description: None,
        });
        assert_eq!(vm.state.get(), SimState::Paused);

        // Reaching the end of the run (the `stopTime reached` description)
        // completes it — Finished, not Paused — so a live run settles like a
        // one-shot and triggers the To Workspace export.
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "step".into(),
            description: Some("stopTime reached".into()),
        });
        assert_eq!(vm.state.get(), SimState::Finished);
        assert_eq!(vm.stop_reason.get().as_deref(), Some("stopTime reached"));

        // Reset clears the live state back to Idle.
        vm.reset();
        assert!(!vm.live.get());
        assert_eq!(vm.sim_time.get(), 0.0);
        assert!(vm.active_block.get().is_none());
    }

    #[test]
    fn source_line_breakpoint_stop_is_recorded() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        // A `breakpoint` stop with a `<block>:<line>` description is a MATLAB
        // Function source-line pause (#354).
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "breakpoint".into(),
            description: Some("fcn:3".into()),
        });
        assert_eq!(vm.state.get(), SimState::Paused);
        assert_eq!(vm.source_stop.get(), Some(("fcn".into(), 3)));

        // A signal breakpoint (no `:line` form) is NOT a source-line stop.
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "breakpoint".into(),
            description: Some("sat_1 abs(value) > 1e3 (=2000)".into()),
        });
        assert_eq!(vm.source_stop.get(), None);

        // Resume clears the source stop + locals.
        vm.source_stop.set(Some(("fcn".into(), 3)));
        vm.locals.set(vec![("a".into(), "4".into())]);
        vm.resume();
        assert_eq!(vm.source_stop.get(), None);
        assert!(vm.locals.get().is_empty());
    }

    #[test]
    fn statement_step_tracks_source_line_and_locals() {
        use crate::services::sim_dap::{SimEvent, SimRequest};
        let vm = vm();
        vm.start_live();

        // Outside a function body, Step is a major step and Step Out is disabled.
        assert_eq!(vm.live_step_request(), SimRequest::StepMajor);
        assert!(!vm.can_step_out());

        // Hit a body breakpoint: now Step is a statement step + Step Out is live.
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "breakpoint".into(),
            description: Some("fcn:3".into()),
        });
        assert_eq!(vm.source_stop.get(), Some(("fcn".into(), 3)));
        assert_eq!(vm.live_step_request(), SimRequest::StepStatement);
        assert!(vm.can_step_out());

        // A `step` stop (compiler #386) advances the source-line marker; the UI
        // re-fetches locals off `source_stop` being set.
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "step".into(),
            description: Some("fcn:4".into()),
        });
        assert_eq!(vm.source_stop.get(), Some(("fcn".into(), 4)));
        assert_eq!(vm.live_step_request(), SimRequest::StepStatement);

        // The body returning clears the marker + locals and drops back to
        // major-step granularity.
        vm.locals.set(vec![("b".into(), "8".into())]);
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "step".into(),
            description: Some("function returned".into()),
        });
        assert_eq!(vm.source_stop.get(), None);
        assert!(vm.locals.get().is_empty());
        assert_eq!(vm.live_step_request(), SimRequest::StepMajor);
        assert!(!vm.can_step_out());
    }

    #[test]
    fn stopped_event_ignored_when_idle() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.on_sim_event(&SimEvent::Stopped {
            reason: "entry".into(),
            description: None,
        });
        assert_eq!(vm.state.get(), SimState::Idle);
    }

    #[test]
    fn live_signal_samples_accumulate_into_scopes() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        let sig = |b: &str, t, v| SimEvent::Signal {
            block_id: b.into(),
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
    fn step_counter_is_current_when_the_clock_updates() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        // The clock label is bound to `sim_time` and reads `major_step` in the
        // same callback; the step must already be set when the clock fires.
        let captured = std::rc::Rc::new(std::cell::Cell::new(-1i64));
        let c = captured.clone();
        let step = vm.major_step.clone();
        vm.sim_time.subscribe(move |_| c.set(step.get()));
        vm.on_sim_event(&SimEvent::Time {
            t: 0.05,
            major_step: 5,
        });
        assert_eq!(captured.get(), 5);
    }

    #[test]
    fn stepping_back_truncates_the_live_trace() {
        use crate::services::sim_dap::SimEvent;
        let vm = vm();
        vm.start_live();
        let sig = |t, v| SimEvent::Signal {
            block_id: "scope".into(),
            t,
            value: v,
        };
        // Step forward to t = 0.0, 0.1, 0.2.
        vm.on_sim_event(&sig(0.0, 0.0));
        vm.on_sim_event(&sig(0.1, 1.0));
        vm.on_sim_event(&sig(0.2, 2.0));
        assert_eq!(vm.scope_series(0).0, vec![0.0, 0.1, 0.2]);
        // Step back to t = 0.1: the future sample (t = 0.2) is dropped, not
        // appended out of order, so the trace stays monotonic.
        vm.on_sim_event(&sig(0.1, 1.0));
        assert_eq!(vm.scope_series(0).0, vec![0.0, 0.1]);
        assert_eq!(vm.scope_series(0).1, vec![0.0, 1.0]);
        // A sample back at t = 0.0 collapses the trace to a single point.
        vm.on_sim_event(&sig(0.0, 0.0));
        assert_eq!(vm.scope_series(0).0, vec![0.0]);
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
            description: None,
        });
        assert_eq!(vm.stop_reason.get().as_deref(), Some("breakpoint"));
        assert_eq!(vm.state.get(), SimState::Paused);
        vm.reset();
        assert!(vm.snapshots.get().is_empty());
        assert!(vm.stop_reason.get().is_none());
    }
}
