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
        FlowEdge { id: id.to_string(), kind, from, to, label: None, waypoints: None, data: None }
    }
}

/// Edge-level data bag — chart transitions carry a typed `params` map.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct EdgeData {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<BTreeMap<String, ParamValue>>,
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
        EdgeEndpoint { node: node.to_string(), port: port.to_string() }
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
        assert_eq!(serde_json::to_string(&EdgeKind::Control).unwrap(), "\"control\"");
        assert_eq!(
            serde_json::from_str::<EdgeKind>("\"transition\"").unwrap(),
            EdgeKind::Transition
        );
    }

    #[test]
    fn edge_omits_none_fields() {
        let e = FlowEdge::new("e1", EdgeKind::Control, EdgeEndpoint::new("a", "out"), EdgeEndpoint::new("b", "in"));
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
}
