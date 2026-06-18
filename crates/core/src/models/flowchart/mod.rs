//! Flowchart (`.mflow`) document model — the `matforge.flowchart` schema as
//! Rust value types. Split across submodules but re-exported flat so callers
//! use `models::flowchart::FlowNode` etc.

mod analysis;
mod document;
mod edge;
mod node;
mod palette;

pub use analysis::{
    algebraic_loop_nodes, autosize_compounds, children_ids, descendants, format_on_event,
    hierarchy_errors, is_descendant, lint_action, parse_on_event,
};
pub use document::{
    AlgebraicLoopMethod, ChartSymbol, ChartSymbols, Flow, FlowKind, FlowLayout, FlowSignature,
    FlowchartDocument, FlowchartMetadata, FlowchartSettings, SchemaKind, SnapshotConfig,
    SnapshotFields, SolverAlgorithm, SolverConfig, SolverType,
};
pub use edge::{EdgeData, EdgeEndpoint, EdgeKind, FlowEdge, FlowchartClipboard};
pub use node::{
    FlowNode, FlowPort, FlowPorts, FlowPosition, FlowSize, FlowUi, NodeData, NodeKind, NodeShape,
    ParamValue, PortAnchor, StateDecomposition,
};
pub use palette::{library_blocks, NodeCategory, ParamConstraint, SignalFlowParamSpec};
