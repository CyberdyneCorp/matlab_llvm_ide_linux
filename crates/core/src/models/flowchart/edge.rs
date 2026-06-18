//! Flowchart edges and the canvas clipboard payload. Ported from
//! `FlowchartModels.swift`; `label`/`waypoints`/`data` are IDE-only and
//! round-tripped verbatim (the compiler ignores unknown fields).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::node::{FlowNode, FlowPosition, ParamValue};

/// Schema §4 edge `{ id, kind, from, to }` plus IDE-only annotations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
    pub id: String,
    pub kind: EdgeKind,
    pub from: EdgeEndpoint,
    pub to: EdgeEndpoint,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub waypoints: Option<Vec<FlowPosition>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<EdgeData>,
}

impl FlowEdge {
    pub fn new(id: &str, kind: EdgeKind, from: EdgeEndpoint, to: EdgeEndpoint) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            kind,
            from,
            to,
            label: None,
            waypoints: None,
            data: None,
        }
    }
}

/// Edge-level data bag — chart transitions carry a typed `params` map.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct EdgeData {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<BTreeMap<String, ParamValue>>,
}

/// The structured parts of a Stateflow transition label, in canonical order
/// `event[guard]{condAction}/transAction` — each part optional. Used by the
/// state-transition table editor to decompose / recompose the edge `label`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransitionLabel {
    pub event: Option<String>,
    pub guard: Option<String>,
    pub cond_action: Option<String>,
    pub trans_action: Option<String>,
}

impl TransitionLabel {
    /// Parse a transition label. The bracketed `[guard]` and braced
    /// `{condAction}` sections are pulled out first (so a `/` inside them is not
    /// mistaken for the transition-action separator); what remains splits on the
    /// first `/` into the event and the transition action.
    pub fn parse(label: &str) -> TransitionLabel {
        let (guard, rest) = take_braced(label, '[', ']');
        let (cond_action, rest) = take_braced(&rest, '{', '}');
        let (event, trans_action) = match rest.split_once('/') {
            Some((e, t)) => (e.to_string(), Some(t.to_string())),
            None => (rest, None),
        };
        let norm = |s: &str| {
            let t = s.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        TransitionLabel {
            event: norm(&event),
            guard: guard.as_deref().and_then(norm),
            cond_action: cond_action.as_deref().and_then(norm),
            trans_action: trans_action.as_deref().and_then(norm),
        }
    }

    /// Recompose the canonical label string (empty when every part is unset).
    pub fn format(&self) -> String {
        let mut out = String::new();
        if let Some(e) = &self.event {
            out.push_str(e);
        }
        if let Some(g) = &self.guard {
            out.push('[');
            out.push_str(g);
            out.push(']');
        }
        if let Some(c) = &self.cond_action {
            out.push('{');
            out.push_str(c);
            out.push('}');
        }
        if let Some(t) = &self.trans_action {
            out.push('/');
            out.push_str(t);
        }
        out
    }

    /// True when no part is set (so the edge needs no label).
    pub fn is_empty(&self) -> bool {
        self.event.is_none()
            && self.guard.is_none()
            && self.cond_action.is_none()
            && self.trans_action.is_none()
    }
}

/// Pull the first `open..close` section out of `s`, returning its inner text and
/// `s` with that section removed. Returns `(None, s)` when unbalanced/absent.
fn take_braced(s: &str, open: char, close: char) -> (Option<String>, String) {
    let Some(start) = s.find(open) else {
        return (None, s.to_string());
    };
    let after = &s[start + open.len_utf8()..];
    let Some(len) = after.find(close) else {
        return (None, s.to_string());
    };
    let inner = after[..len].to_string();
    let mut rem = String::with_capacity(s.len());
    rem.push_str(&s[..start]);
    rem.push_str(&after[len + close.len_utf8()..]);
    (Some(inner), rem)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Control,
    Data,
    Transition,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EdgeEndpoint {
    pub node: String,
    pub port: String,
}

impl EdgeEndpoint {
    pub fn new(node: &str, port: &str) -> EdgeEndpoint {
        EdgeEndpoint {
            node: node.to_string(),
            port: port.to_string(),
        }
    }
}

/// In-memory clipboard for Ctrl+C / Ctrl+V on the canvas: the copied nodes,
/// their internal edges, and the selection's top-left anchor for paste offset.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowchartClipboard {
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    pub anchor: FlowPosition,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_kind_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&EdgeKind::Control).unwrap(),
            "\"control\""
        );
        assert_eq!(
            serde_json::from_str::<EdgeKind>("\"transition\"").unwrap(),
            EdgeKind::Transition
        );
    }

    #[test]
    fn edge_omits_none_fields() {
        let e = FlowEdge::new(
            "e1",
            EdgeKind::Control,
            EdgeEndpoint::new("a", "out"),
            EdgeEndpoint::new("b", "in"),
        );
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("label"));
        assert!(!json.contains("waypoints"));
        assert!(json.contains("\"from\""));
    }

    #[test]
    fn endpoint_roundtrips() {
        let ep = EdgeEndpoint::new("n1", "true");
        let json = serde_json::to_string(&ep).unwrap();
        let back: EdgeEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(ep, back);
    }

    #[test]
    fn transition_label_parse_and_format_round_trip() {
        // Full label: event[guard]{condAction}/transAction.
        let l = TransitionLabel::parse("E[x > 0]{y = 1}/z = z + 1");
        assert_eq!(l.event.as_deref(), Some("E"));
        assert_eq!(l.guard.as_deref(), Some("x > 0"));
        assert_eq!(l.cond_action.as_deref(), Some("y = 1"));
        assert_eq!(l.trans_action.as_deref(), Some("z = z + 1"));
        assert_eq!(l.format(), "E[x > 0]{y = 1}/z = z + 1");

        // Guard only, and a '/' living inside the condition action is preserved.
        let g = TransitionLabel::parse("[a && b]");
        assert_eq!(g.guard.as_deref(), Some("a && b"));
        assert!(g.event.is_none() && g.trans_action.is_none());
        let slash = TransitionLabel::parse("{p = a/b}");
        assert_eq!(slash.cond_action.as_deref(), Some("p = a/b"));
        assert!(slash.trans_action.is_none());

        // Empty / whitespace label is empty.
        assert!(TransitionLabel::parse("   ").is_empty());
        assert_eq!(TransitionLabel::default().format(), "");
    }
}
