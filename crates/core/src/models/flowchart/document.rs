//! Document-level `.mflow` types: the root container, settings, dialect,
//! solver/snapshot config, flows, and chart symbol tables. Ported from
//! `FlowchartModels.swift`; serde key names match the `matforge.flowchart`
//! JSON schema 1:1 so `FlowchartCodec` round-trips byte-stably.

use serde::{Deserialize, Serialize};

use super::edge::FlowEdge;
use super::node::FlowNode;

/// Top-level on-disk container. One `.mflow` file = one document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowchartDocument {
    pub schema: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub entry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub settings: Option<FlowchartSettings>,
    pub flows: Vec<Flow>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub metadata: Option<FlowchartMetadata>,
    /// Free-form authorship comments (`_comment` in JSON); round-tripped verbatim.
    #[serde(rename = "_comment", skip_serializing_if = "Option::is_none", default)]
    pub comment: Option<Vec<String>>,
}

impl FlowchartDocument {
    pub const CURRENT_SCHEMA: &'static str = "matforge.flowchart";
    pub const CURRENT_VERSION: &'static str = "0.2.0";

    /// Effective dialect — `settings.kind` resolved through the control-flow
    /// default (both `nil` and explicit `control_flow` read as control-flow).
    pub fn schema_kind(&self) -> SchemaKind {
        self.settings.as_ref().and_then(|s| s.kind).unwrap_or(SchemaKind::ControlFlow)
    }

    /// Fresh empty document for the given dialect.
    pub fn empty(name: &str, kind: SchemaKind) -> FlowchartDocument {
        match kind {
            SchemaKind::ControlFlow => Self::empty_control_flow(name),
            SchemaKind::SignalFlow => Self::empty_signal_flow(name),
            SchemaKind::StateChart => Self::empty_state_chart(name),
        }
    }

    fn base(name: &str, settings: FlowchartSettings, flows: Vec<Flow>) -> FlowchartDocument {
        FlowchartDocument {
            schema: Self::CURRENT_SCHEMA.to_string(),
            version: Self::CURRENT_VERSION.to_string(),
            entry: Some("main".to_string()),
            settings: Some(settings),
            flows,
            // Deterministic id (no RNG in core); the IDE can re-stamp on save.
            id: Some(format!("project_{}", name.to_lowercase().replace(' ', "_"))),
            name: Some(name.to_string()),
            metadata: Some(FlowchartMetadata {
                created_by: Some("MatForge IDE".to_string()),
                description: None,
            }),
            comment: None,
        }
    }

    /// Historical control-flow document: Start → End by one control edge.
    fn empty_control_flow(name: &str) -> FlowchartDocument {
        use super::edge::{EdgeEndpoint, EdgeKind};
        use super::node::{FlowUi, NodeData, NodeKind, FlowPosition};
        let start = FlowNode::new(
            "main_start",
            NodeKind::Start,
            "Start",
            NodeKind::Start.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x: 240.0, y: 60.0 }),
        );
        let end = FlowNode::new(
            "main_end",
            NodeKind::End,
            "End",
            NodeKind::End.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x: 240.0, y: 220.0 }),
        );
        let edge = FlowEdge::new(
            "e_initial",
            EdgeKind::Control,
            EdgeEndpoint::new("main_start", "out"),
            EdgeEndpoint::new("main_end", "in"),
        );
        let flow = Flow::new(
            "flow_main",
            FlowKind::Program,
            "main",
            FlowSignature::default(),
            vec![start, end],
            vec![edge],
            Some(FlowLayout { direction: Some("TB".into()), zoom: Some(1.0) }),
        );
        Self::base(name, FlowchartSettings::control_flow(), vec![flow])
    }

    fn empty_state_chart(name: &str) -> FlowchartDocument {
        let flow = Flow::new(
            "flow_main",
            FlowKind::Program,
            "main",
            FlowSignature::default(),
            vec![],
            vec![],
            Some(FlowLayout { direction: Some("LR".into()), zoom: Some(1.0) }),
        );
        let mut settings = FlowchartSettings::control_flow();
        settings.kind = Some(SchemaKind::StateChart);
        Self::base(name, settings, vec![flow])
    }

    fn empty_signal_flow(name: &str) -> FlowchartDocument {
        let flow = Flow::new(
            "flow_main",
            FlowKind::Program,
            "main",
            FlowSignature::default(),
            vec![],
            vec![],
            Some(FlowLayout { direction: Some("LR".into()), zoom: Some(1.0) }),
        );
        let mut settings = FlowchartSettings::control_flow();
        settings.kind = Some(SchemaKind::SignalFlow);
        settings.solver = Some(SolverConfig::default_variable_step());
        settings.snapshot = Some(SnapshotConfig {
            enabled: Some(true),
            depth: Some(256),
            fields: Some(SnapshotFields::States),
        });
        Self::base(name, settings, vec![flow])
    }
}

/// IDE-private metadata; the compiler ignores this object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowchartMetadata {
    #[serde(rename = "createdBy", skip_serializing_if = "Option::is_none", default)]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}

/// Per-document defaults the compiler reads. All fields optional.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct FlowchartSettings {
    #[serde(rename = "columnMajor", skip_serializing_if = "Option::is_none", default)]
    pub column_major: Option<bool>,
    #[serde(rename = "defaultNumericType", skip_serializing_if = "Option::is_none", default)]
    pub default_numeric_type: Option<String>,
    #[serde(rename = "sourceLanguage", skip_serializing_if = "Option::is_none", default)]
    pub source_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub kind: Option<SchemaKind>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub solver: Option<SolverConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub snapshot: Option<SnapshotConfig>,
}

impl FlowchartSettings {
    fn control_flow() -> FlowchartSettings {
        FlowchartSettings {
            column_major: Some(true),
            default_numeric_type: Some("double".into()),
            source_language: Some("matforge".into()),
            kind: None,
            solver: None,
            snapshot: None,
        }
    }
}

/// Document dialect — picks which palette and inspector the IDE shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SchemaKind {
    #[serde(rename = "control_flow")]
    ControlFlow,
    #[serde(rename = "signal_flow")]
    SignalFlow,
    #[serde(rename = "state_chart")]
    StateChart,
}

impl SchemaKind {
    pub const ALL: [SchemaKind; 3] =
        [SchemaKind::ControlFlow, SchemaKind::SignalFlow, SchemaKind::StateChart];
}

/// Solver config for signal-flow documents. All fields optional.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct SolverConfig {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub solver_type: Option<SolverType>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub algorithm: Option<SolverAlgorithm>,
    #[serde(rename = "startTime", skip_serializing_if = "Option::is_none", default)]
    pub start_time: Option<f64>,
    #[serde(rename = "stopTime", skip_serializing_if = "Option::is_none", default)]
    pub stop_time: Option<f64>,
    #[serde(rename = "maxStep", skip_serializing_if = "Option::is_none", default)]
    pub max_step: Option<String>,
    #[serde(rename = "minStep", skip_serializing_if = "Option::is_none", default)]
    pub min_step: Option<String>,
    #[serde(rename = "relTol", skip_serializing_if = "Option::is_none", default)]
    pub rel_tol: Option<f64>,
    #[serde(rename = "absTol", skip_serializing_if = "Option::is_none", default)]
    pub abs_tol: Option<f64>,
    #[serde(rename = "zeroCrossing", skip_serializing_if = "Option::is_none", default)]
    pub zero_crossing: Option<bool>,
    #[serde(rename = "algebraicLoopMethod", skip_serializing_if = "Option::is_none", default)]
    pub algebraic_loop_method: Option<AlgebraicLoopMethod>,
}

impl SolverConfig {
    fn default_variable_step() -> SolverConfig {
        SolverConfig {
            solver_type: Some(SolverType::VariableStep),
            algorithm: Some(SolverAlgorithm::Ode45),
            start_time: Some(0.0),
            stop_time: Some(10.0),
            max_step: Some("auto".into()),
            min_step: Some("auto".into()),
            rel_tol: Some(1e-3),
            abs_tol: Some(1e-6),
            zero_crossing: Some(true),
            algebraic_loop_method: Some(AlgebraicLoopMethod::TrustRegion),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolverType {
    #[serde(rename = "fixed_step")]
    FixedStep,
    #[serde(rename = "variable_step")]
    VariableStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SolverAlgorithm {
    Ode45,
    Ode23,
    Ode15s,
    Euler,
    Heun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlgebraicLoopMethod {
    #[serde(rename = "trust_region")]
    TrustRegion,
    #[serde(rename = "newton")]
    Newton,
    #[serde(rename = "off")]
    Off,
}

/// Snapshot-ring config for step-back debugging.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct SnapshotConfig {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub depth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fields: Option<SnapshotFields>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotFields {
    #[serde(rename = "states")]
    States,
    #[serde(rename = "states+inputs")]
    StatesPlusInputs,
    #[serde(rename = "all")]
    All,
}

/// One executable diagram inside a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub id: String,
    pub kind: FlowKind,
    pub name: String,
    #[serde(default)]
    pub signature: FlowSignature,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub layout: Option<FlowLayout>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub solver: Option<SolverConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub symbols: Option<ChartSymbols>,
}

impl Flow {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        kind: FlowKind,
        name: &str,
        signature: FlowSignature,
        nodes: Vec<FlowNode>,
        edges: Vec<FlowEdge>,
        layout: Option<FlowLayout>,
    ) -> Flow {
        Flow {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            signature,
            nodes,
            edges,
            layout,
            solver: None,
            symbols: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowKind {
    Program,
    Function,
    Library,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FlowSignature {
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowLayout {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub zoom: Option<f64>,
}

/// Chart-level symbol tables (mStateflow).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct ChartSymbols {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<Vec<ChartSymbol>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub events: Option<Vec<ChartSymbol>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub messages: Option<Vec<ChartSymbol>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartSymbol {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none", default)]
    pub symbol_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub units: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub initial: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_control_flow_has_start_end_and_edge() {
        let doc = FlowchartDocument::empty("Demo", SchemaKind::ControlFlow);
        assert_eq!(doc.schema_kind(), SchemaKind::ControlFlow);
        assert_eq!(doc.flows.len(), 1);
        let flow = &doc.flows[0];
        assert_eq!(flow.nodes.len(), 2);
        assert_eq!(flow.edges.len(), 1);
        assert_eq!(flow.edges[0].from.node, "main_start");
        assert_eq!(flow.edges[0].to.node, "main_end");
    }

    #[test]
    fn empty_signal_flow_sets_solver_and_kind() {
        let doc = FlowchartDocument::empty("S", SchemaKind::SignalFlow);
        assert_eq!(doc.schema_kind(), SchemaKind::SignalFlow);
        let settings = doc.settings.unwrap();
        assert!(settings.solver.is_some());
        assert!(settings.snapshot.is_some());
        assert_eq!(doc.flows[0].nodes.len(), 0);
    }

    #[test]
    fn empty_state_chart_is_blank() {
        let doc = FlowchartDocument::empty("C", SchemaKind::StateChart);
        assert_eq!(doc.schema_kind(), SchemaKind::StateChart);
        assert!(doc.flows[0].nodes.is_empty());
    }

    #[test]
    fn schema_kind_defaults_to_control_flow_when_absent() {
        let doc = FlowchartDocument {
            schema: "x".into(),
            version: "0.1.0".into(),
            entry: None,
            settings: None,
            flows: vec![],
            id: None,
            name: None,
            metadata: None,
            comment: None,
        };
        assert_eq!(doc.schema_kind(), SchemaKind::ControlFlow);
    }

    #[test]
    fn schema_kind_serde_uses_snake_case() {
        let json = serde_json::to_string(&SchemaKind::SignalFlow).unwrap();
        assert_eq!(json, "\"signal_flow\"");
        let back: SchemaKind = serde_json::from_str("\"state_chart\"").unwrap();
        assert_eq!(back, SchemaKind::StateChart);
    }
}
