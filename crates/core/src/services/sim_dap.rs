//! Client-side model of the mflowLink simulation DAP
//! (`matlabc -simulate --sim-dap`). Builds the simulation-control request frames
//! on top of the generic [`DapClient`](super::dap::DapClient) and parses the
//! simulation event stream into typed [`SimEvent`]s.
//!
//! Protocol: compiler `docs/mflow_link_roadmap.md` §10. Pure + tested; the
//! process plumbing (spawn, stdio pump) lives in the app.

use serde_json::{json, Value};

use super::dap::{DapClient, DapMessage};

/// A time breakpoint: pause when the simulation clock reaches `t`, optionally
/// gated by a condition expression.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeBreakpoint {
    pub t: f64,
    pub condition: Option<String>,
}

/// A signal breakpoint on an edge: pause when `condition` (over the edge value)
/// holds — e.g. `"abs(value) > 1e3"`.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalBreakpoint {
    /// The block whose output signal is watched (the simulator keys signal
    /// breakpoints by the source block of the wire).
    pub block_id: String,
    pub condition: Option<String>,
}

/// A simulation-control request the IDE sends to the sim-DAP server.
#[derive(Clone, Debug, PartialEq)]
pub enum SimRequest {
    /// Advance one major (solver) step.
    StepMajor,
    /// Advance one statement inside a MATLAB Function body (DAP `next`). The
    /// server replays the body one statement at a time while paused inside it
    /// (compiler #386); outside a body it falls through to one major step.
    StepStatement,
    /// Finish the current MATLAB Function body (DAP `stepOut`, compiler #386),
    /// returning the transport to major-step granularity.
    StepOut,
    /// Advance one block within the current major step.
    StepBlock,
    /// Restore the previous major step from the snapshot ring.
    StepBackMajor,
    /// Step one block backwards within the current major step.
    StepBackBlock,
    /// Resume free-running until a breakpoint or `stopTime`.
    Continue,
    /// Pause a free-running simulation at the next major step.
    Pause,
    /// Reset to `startTime` and clear the snapshot ring.
    ResetSimulation,
    /// Tune the solver while paused (any subset of fields).
    ConfigureSolver {
        rel_tol: Option<f64>,
        max_step: Option<f64>,
    },
    /// Replace the set of time breakpoints.
    SetTimeBreakpoints(Vec<TimeBreakpoint>),
    /// Replace the set of signal breakpoints. `source` is the model path the
    /// adapter keys edge ids against.
    SetSignalBreakpoints {
        source: String,
        breakpoints: Vec<SignalBreakpoint>,
    },
}

impl SimRequest {
    /// The DAP `command` and `arguments` for this request.
    pub fn command_and_args(&self) -> (&'static str, Option<Value>) {
        // Stepping verbs carry the single simulation thread id.
        let thread = || Some(json!({ "threadId": 1 }));
        match self {
            SimRequest::StepMajor => ("stepMajor", thread()),
            SimRequest::StepStatement => ("next", thread()),
            SimRequest::StepOut => ("stepOut", thread()),
            SimRequest::StepBlock => ("stepBlock", thread()),
            SimRequest::StepBackMajor => ("stepBackMajor", thread()),
            SimRequest::StepBackBlock => ("stepBackBlock", thread()),
            SimRequest::Continue => ("continue", thread()),
            SimRequest::Pause => ("pause", thread()),
            SimRequest::ResetSimulation => ("resetSimulation", None),
            SimRequest::ConfigureSolver { rel_tol, max_step } => {
                let mut a = serde_json::Map::new();
                if let Some(r) = rel_tol {
                    a.insert("relTol".into(), json!(r));
                }
                if let Some(m) = max_step {
                    a.insert("maxStep".into(), json!(m));
                }
                ("configureSolver", Some(Value::Object(a)))
            }
            SimRequest::SetTimeBreakpoints(times) => {
                let arr: Vec<Value> = times
                    .iter()
                    .map(|b| match &b.condition {
                        Some(c) => json!({ "t": b.t, "condition": c }),
                        None => json!({ "t": b.t }),
                    })
                    .collect();
                ("setTimeBreakpoints", Some(json!({ "times": arr })))
            }
            SimRequest::SetSignalBreakpoints {
                source,
                breakpoints,
            } => {
                let arr: Vec<Value> = breakpoints
                    .iter()
                    .map(|b| match &b.condition {
                        Some(c) => json!({ "blockId": b.block_id, "condition": c }),
                        None => json!({ "blockId": b.block_id }),
                    })
                    .collect();
                (
                    "setSignalBreakpoints",
                    Some(json!({ "source": { "path": source }, "breakpoints": arr })),
                )
            }
        }
    }
}

/// Build the framed request bytes for `req`, advancing `client`'s seq counter.
pub fn build_request(client: &mut DapClient, req: &SimRequest) -> String {
    let (cmd, args) = req.command_and_args();
    client.request(cmd, args)
}

/// A typed simulation event streamed up from the sim-DAP server.
#[derive(Clone, Debug, PartialEq)]
pub enum SimEvent {
    /// The simulation clock advanced.
    Time { t: f64, major_step: i64 },
    /// The block currently being evaluated (drives the active-block halo).
    ActiveBlock { node_id: String },
    /// A logged signal sample on a block (feeds the live scopes).
    Signal {
        block_id: String,
        t: f64,
        value: f64,
    },
    /// A zero-crossing was located on a block.
    ZeroCrossing { block_id: String, t: f64 },
    /// A snapshot was pushed to the step-back ring.
    Snapshot { major_step: i64, depth: i64 },
    /// The run stopped (entry / breakpoint / step / pause / crossing). The
    /// `description` carries the human detail — notably `"stopTime reached"` at
    /// the end of a run, which the viewmodel treats as completion.
    Stopped {
        reason: String,
        description: Option<String>,
    },
}

/// The block identifier from a signal/sample body: `blockId` (current) or the
/// legacy `edgeId` spelling.
fn block_or_edge_id(body: &Value) -> Option<String> {
    body.get("blockId")
        .or_else(|| body.get("edgeId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The block identifier from a zero-crossing body: `blockId` (current) or the
/// legacy `nodeId` spelling.
fn block_or_node_id(body: &Value) -> Option<String> {
    body.get("blockId")
        .or_else(|| body.get("nodeId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Interpret a decoded [`DapMessage`] as a [`SimEvent`], if it is one.
pub fn parse_sim_event(msg: &DapMessage) -> Option<SimEvent> {
    let DapMessage::Event { event, body } = msg else {
        return None;
    };
    match event.as_str() {
        "simulationTime" => Some(SimEvent::Time {
            t: body.get("t")?.as_f64()?,
            major_step: body.get("majorStep").and_then(Value::as_i64).unwrap_or(0),
        }),
        "simulationActiveBlock" => Some(SimEvent::ActiveBlock {
            node_id: body.get("nodeId")?.as_str()?.to_string(),
        }),
        "signalSample" => Some(SimEvent::Signal {
            // The simulator keys samples by `blockId`; accept the legacy
            // `edgeId` spelling too so old streams still parse.
            block_id: block_or_edge_id(body)?,
            t: body.get("t")?.as_f64()?,
            value: body.get("value")?.as_f64()?,
        }),
        "zeroCrossing" => Some(SimEvent::ZeroCrossing {
            block_id: block_or_node_id(body)?,
            t: body.get("t")?.as_f64()?,
        }),
        "snapshotTaken" => Some(SimEvent::Snapshot {
            major_step: body.get("majorStep")?.as_i64()?,
            depth: body.get("depth").and_then(Value::as_i64).unwrap_or(0),
        }),
        "stopped" => Some(SimEvent::Stopped {
            reason: body
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description: body
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::dap::parse_message;
    use super::*;

    fn frame_command(req: &SimRequest) -> (String, Value) {
        let mut c = DapClient::new();
        let frame = build_request(&mut c, req);
        // Strip the Content-Length header to get the JSON body.
        let body = frame.split("\r\n\r\n").nth(1).unwrap().to_string();
        let v: Value = serde_json::from_str(&body).unwrap();
        (
            v["command"].as_str().unwrap().to_string(),
            v["arguments"].clone(),
        )
    }

    #[test]
    fn stepping_requests_carry_thread_id() {
        let (cmd, args) = frame_command(&SimRequest::StepMajor);
        assert_eq!(cmd, "stepMajor");
        assert_eq!(args["threadId"], 1);
        assert_eq!(frame_command(&SimRequest::StepBackMajor).0, "stepBackMajor");
        assert_eq!(frame_command(&SimRequest::Continue).0, "continue");
    }

    #[test]
    fn statement_step_uses_standard_dap_verbs() {
        // Function-body statement stepping (compiler #386) rides the standard DAP
        // `next`/`stepOut` verbs — distinct from the custom `stepMajor`.
        let (next, next_args) = frame_command(&SimRequest::StepStatement);
        assert_eq!(next, "next");
        assert_eq!(next_args["threadId"], 1);
        let (out, out_args) = frame_command(&SimRequest::StepOut);
        assert_eq!(out, "stepOut");
        assert_eq!(out_args["threadId"], 1);
    }

    #[test]
    fn configure_solver_includes_only_set_fields() {
        let (cmd, args) = frame_command(&SimRequest::ConfigureSolver {
            rel_tol: Some(1e-4),
            max_step: None,
        });
        assert_eq!(cmd, "configureSolver");
        assert_eq!(args["relTol"], 1e-4);
        assert!(args.get("maxStep").is_none());
    }

    #[test]
    fn time_breakpoints_serialize_with_optional_condition() {
        let (cmd, args) = frame_command(&SimRequest::SetTimeBreakpoints(vec![
            TimeBreakpoint {
                t: 5.0,
                condition: None,
            },
            TimeBreakpoint {
                t: 7.5,
                condition: Some("x > 0".into()),
            },
        ]));
        assert_eq!(cmd, "setTimeBreakpoints");
        assert_eq!(args["times"][0]["t"], 5.0);
        assert_eq!(args["times"][1]["condition"], "x > 0");
    }

    #[test]
    fn signal_breakpoints_serialize_source_and_block() {
        let (cmd, args) = frame_command(&SimRequest::SetSignalBreakpoints {
            source: "model.mflow".into(),
            breakpoints: vec![SignalBreakpoint {
                block_id: "fcn".into(),
                condition: Some("abs(value) > 1e3".into()),
            }],
        });
        assert_eq!(cmd, "setSignalBreakpoints");
        assert_eq!(args["source"]["path"], "model.mflow");
        // The simulator keys signal breakpoints by the source block id.
        assert_eq!(args["breakpoints"][0]["blockId"], "fcn");
    }

    fn event(json_body: &str) -> SimEvent {
        parse_sim_event(&parse_message(json_body).unwrap()).unwrap()
    }

    #[test]
    fn parses_every_simulation_event() {
        assert_eq!(
            event(
                r#"{"type":"event","event":"simulationTime","body":{"t":1.25,"majorStep":1234}}"#
            ),
            SimEvent::Time {
                t: 1.25,
                major_step: 1234
            }
        );
        assert_eq!(
            event(r#"{"type":"event","event":"simulationActiveBlock","body":{"nodeId":"gain_1"}}"#),
            SimEvent::ActiveBlock {
                node_id: "gain_1".into()
            }
        );
        assert_eq!(
            event(
                r#"{"type":"event","event":"signalSample","body":{"blockId":"scope_1","t":1.2,"value":0.42}}"#
            ),
            SimEvent::Signal {
                block_id: "scope_1".into(),
                t: 1.2,
                value: 0.42
            }
        );
        assert_eq!(
            event(r#"{"type":"event","event":"zeroCrossing","body":{"blockId":"sat_1","t":1.2}}"#),
            SimEvent::ZeroCrossing {
                block_id: "sat_1".into(),
                t: 1.2
            }
        );
        assert_eq!(
            event(
                r#"{"type":"event","event":"snapshotTaken","body":{"majorStep":1234,"depth":47}}"#
            ),
            SimEvent::Snapshot {
                major_step: 1234,
                depth: 47
            }
        );
        assert_eq!(
            event(r#"{"type":"event","event":"stopped","body":{"reason":"breakpoint"}}"#),
            SimEvent::Stopped {
                reason: "breakpoint".into(),
                description: None,
            }
        );
        // End-of-run carries the `stopTime reached` description (matlabc sim DAP).
        assert_eq!(
            event(
                r#"{"type":"event","event":"stopped","body":{"reason":"step","description":"stopTime reached"}}"#
            ),
            SimEvent::Stopped {
                reason: "step".into(),
                description: Some("stopTime reached".into()),
            }
        );
    }

    #[test]
    fn signal_sample_reads_block_id_keeping_legacy_edge_id() {
        // Regression: the simulator emits `signalSample` keyed by `blockId`. The
        // parser previously required `edgeId`, so every sample was dropped and
        // the live scope stayed empty. Both spellings must now parse.
        let block = event(
            r#"{"type":"event","event":"signalSample","body":{"blockId":"scope_1","t":2.0,"value":1.5}}"#,
        );
        let legacy = event(
            r#"{"type":"event","event":"signalSample","body":{"edgeId":"scope_1","t":2.0,"value":1.5}}"#,
        );
        let expected = SimEvent::Signal {
            block_id: "scope_1".into(),
            t: 2.0,
            value: 1.5,
        };
        assert_eq!(block, expected);
        assert_eq!(legacy, expected);
    }

    #[test]
    fn ignores_non_simulation_messages() {
        // A normal DAP response is not a sim event.
        let resp = parse_message(
            r#"{"type":"response","request_seq":1,"command":"stepMajor","success":true,"body":{}}"#,
        )
        .unwrap();
        assert!(parse_sim_event(&resp).is_none());
        // An unrelated event is ignored.
        let other =
            parse_message(r#"{"type":"event","event":"output","body":{"output":"hi"}}"#).unwrap();
        assert!(parse_sim_event(&other).is_none());
    }
}
