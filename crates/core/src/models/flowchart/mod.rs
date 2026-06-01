//! Flowchart (`.mflow`) document model — the `matforge.flowchart` schema as
//! Rust value types. Split across submodules but re-exported flat so callers
//! use `models::flowchart::FlowNode` etc.

mod document;
mod edge;
mod node;
mod palette;

pub use document::{
    AlgebraicLoopMethod, ChartSymbol, ChartSymbols, Flow, FlowKind, FlowLayout, FlowSignature,
    FlowchartDocument, FlowchartMetadata, FlowchartSettings, SchemaKind, SnapshotConfig,
    SnapshotFields, SolverAlgorithm, SolverConfig, SolverType,
};
pub use edge::{EdgeData, EdgeEndpoint, EdgeKind, FlowEdge, FlowchartClipboard};
pub use node::{
    FlowNode, FlowPort, FlowPorts, FlowPosition, FlowSize, FlowUi, NodeData, NodeKind, NodeShape,
    ParamValue, PortAnchor,
};
pub use palette::{NodeCategory, SignalFlowParamSpec};
