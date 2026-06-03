//! Parser for `matlabc -emit-trace` output. The state-chart interpreter streams
//! one JSON event per line as it runs:
//!
//! ```json
//! {"kind":"superStepBegin","iteration":0}
//! {"kind":"stateEnter","id":"Charge"}
//! {"kind":"transitionFired","id":"t1","src":"Charge","dst":"Discharge"}
//! {"kind":"superStepEnd","iteration":2,"quiescent":true}
//! ```
//!
//! [`parse_chart_event`] turns one line into a [`ChartEvent`] for the mStateflow
//! window's event log + active-state highlighting. Unknown / non-JSON lines
//! yield `None`.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChartEvent {
    StateEnter { id: String },
    StateExit { id: String },
    TransitionFired { id: String, src: String, dst: String },
    SuperStepBegin { iteration: i64 },
    SuperStepEnd { iteration: i64, quiescent: bool },
    Other { kind: String },
}

impl ChartEvent {
    /// A one-line, human-readable description for the event log.
    pub fn summary(&self) -> String {
        match self {
            ChartEvent::StateEnter { id } => format!("→ enter {id}"),
            ChartEvent::StateExit { id } => format!("← exit {id}"),
            ChartEvent::TransitionFired { src, dst, .. } => format!("⇒ {src} → {dst}"),
            ChartEvent::SuperStepBegin { iteration } => format!("· super-step begin (i={iteration})"),
            ChartEvent::SuperStepEnd { iteration, quiescent } => {
                format!("· super-step end (i={iteration}{})", if *quiescent { ", quiescent" } else { "" })
            }
            ChartEvent::Other { kind } => format!("· {kind}"),
        }
    }

    /// The state id this event makes active, if any.
    pub fn entered_state(&self) -> Option<&str> {
        match self {
            ChartEvent::StateEnter { id } => Some(id),
            _ => None,
        }
    }

    /// The state id this event deactivates, if any.
    pub fn exited_state(&self) -> Option<&str> {
        match self {
            ChartEvent::StateExit { id } => Some(id),
            _ => None,
        }
    }
}

/// Parse one trace line. Returns `None` for blank / non-JSON / malformed lines.
pub fn parse_chart_event(line: &str) -> Option<ChartEvent> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let kind = v.get("kind")?.as_str()?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let i = |k: &str| v.get(k).and_then(Value::as_i64);
    Some(match kind {
        "stateEnter" => ChartEvent::StateEnter { id: s("id")? },
        "stateExit" => ChartEvent::StateExit { id: s("id")? },
        "transitionFired" => ChartEvent::TransitionFired {
            id: s("id").unwrap_or_default(),
            src: s("src").unwrap_or_default(),
            dst: s("dst").unwrap_or_default(),
        },
        "superStepBegin" => ChartEvent::SuperStepBegin { iteration: i("iteration").unwrap_or(0) },
        "superStepEnd" => ChartEvent::SuperStepEnd {
            iteration: i("iteration").unwrap_or(0),
            quiescent: v.get("quiescent").and_then(Value::as_bool).unwrap_or(false),
        },
        other => ChartEvent::Other { kind: other.to_string() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_state_events() {
        assert_eq!(
            parse_chart_event(r#"{"kind":"stateEnter","id":"Charge"}"#),
            Some(ChartEvent::StateEnter { id: "Charge".into() })
        );
        assert_eq!(
            parse_chart_event(r#"{"kind":"stateExit","id":"Charge"}"#),
            Some(ChartEvent::StateExit { id: "Charge".into() })
        );
    }

    #[test]
    fn parses_transition_and_steps() {
        let t = parse_chart_event(
            r#"{"kind":"transitionFired","id":"t1","src":"Charge","dst":"Discharge"}"#,
        )
        .unwrap();
        assert_eq!(t, ChartEvent::TransitionFired { id: "t1".into(), src: "Charge".into(), dst: "Discharge".into() });
        assert_eq!(
            parse_chart_event(r#"{"kind":"superStepEnd","iteration":2,"quiescent":true}"#),
            Some(ChartEvent::SuperStepEnd { iteration: 2, quiescent: true })
        );
    }

    #[test]
    fn entered_exited_helpers() {
        assert_eq!(ChartEvent::StateEnter { id: "A".into() }.entered_state(), Some("A"));
        assert_eq!(ChartEvent::StateExit { id: "A".into() }.exited_state(), Some("A"));
        assert_eq!(ChartEvent::SuperStepBegin { iteration: 0 }.entered_state(), None);
    }

    #[test]
    fn summary_is_human_readable() {
        assert_eq!(ChartEvent::StateEnter { id: "X".into() }.summary(), "→ enter X");
        let t = ChartEvent::TransitionFired { id: "t".into(), src: "A".into(), dst: "B".into() };
        assert_eq!(t.summary(), "⇒ A → B");
    }

    #[test]
    fn ignores_non_json_and_banners() {
        assert!(parse_chart_event("ChartModel entry=battery").is_none());
        assert!(parse_chart_event("").is_none());
        assert!(parse_chart_event("{not json}").is_none());
    }

    #[test]
    fn unknown_kind_falls_back_to_other() {
        assert_eq!(
            parse_chart_event(r#"{"kind":"customEvent","foo":1}"#),
            Some(ChartEvent::Other { kind: "customEvent".into() })
        );
    }
}
