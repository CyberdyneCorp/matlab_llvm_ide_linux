//! Pure value types for the IDE — no I/O, no GTK. Ported from the macOS
//! reference's `Models/` directory. Submodules group by domain; everything is
//! re-exported flat so callers use `models::EditorTab`, `models::NodeKind`, etc.

pub mod flowchart;
pub mod ids;

mod compiler;
mod console;
mod debug;
mod editor;
mod plot;
mod project;
mod status;
mod workspace;

pub use compiler::{CompilerTarget, NumericMode, OptimizationProfile};
pub use console::{ConsoleLevel, ConsoleMessage, ConsoleTab};
pub use debug::{
    DapEvaluation, DapStackFrame, DapVariable, DataAccess, DataBreakpoint, ExceptionFilter,
    FunctionBreakpoint,
};
pub use editor::{BreakpointConfig, EditorTab, TabKind};
pub use ids::next_id;
pub use plot::{PlotFigure, PlotKind, PlotView, SurfaceCamera};
pub use project::{NodeFileKind, ProjectNode};
pub use status::{
    CenterLayoutMode, ExplorerAction, SearchMode, SearchScope, StatusBarState,
};
pub use workspace::{
    DType, InspectionColumn, InspectionField, InspectionMethod, MatrixView, WorkspaceVariable,
};

// Flowchart types are also surfaced at the models root for convenience.
pub use flowchart::{
    EdgeKind, Flow, FlowEdge, FlowNode, FlowchartDocument, NodeCategory, NodeKind, NodeShape,
    SchemaKind,
};
