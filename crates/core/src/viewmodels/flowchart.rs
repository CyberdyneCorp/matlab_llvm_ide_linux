//! Per-tab flowchart editor view model. Mirrors `FlowchartViewModel`: holds the
//! in-memory document, selection, viewport (pan/zoom), per-node breakpoints, the
//! paused-node marker, and an undo/redo history. Editing operations act on the
//! entry flow (index 0). Snapshots the document before each mutation for undo.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use crate::models::flowchart::{
    EdgeEndpoint, EdgeKind, FlowEdge, FlowNode, FlowPosition, FlowUi, FlowchartDocument, NodeData,
    NodeKind, SchemaKind,
};
use crate::models::BreakpointConfig;
use crate::observable::Property;
use crate::services::flowchart_codec;

pub const ZOOM_MIN: f64 = 0.4;
pub const ZOOM_MAX: f64 = 2.5;

pub struct FlowchartViewModel {
    pub document: Property<FlowchartDocument>,
    pub selected_id: Property<Option<String>>,
    pub is_dirty: Property<bool>,
    pub zoom: Property<f64>,
    pub pan: Property<(f64, f64)>,
    pub node_breakpoints: Property<BTreeMap<String, BreakpointConfig>>,
    pub execution_node: Property<Option<String>>,
    undo_stack: RefCell<Vec<FlowchartDocument>>,
    redo_stack: RefCell<Vec<FlowchartDocument>>,
    seq: Cell<u64>,
}

impl FlowchartViewModel {
    pub fn from_document(document: FlowchartDocument) -> FlowchartViewModel {
        FlowchartViewModel {
            document: Property::new(document),
            selected_id: Property::new(None),
            is_dirty: Property::new(false),
            zoom: Property::new(1.0),
            pan: Property::new((0.0, 0.0)),
            node_breakpoints: Property::new(BTreeMap::new()),
            execution_node: Property::new(None),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            seq: Cell::new(0),
        }
    }

    pub fn empty(name: &str, kind: SchemaKind) -> FlowchartViewModel {
        FlowchartViewModel::from_document(FlowchartDocument::empty(name, kind))
    }

    fn next_seq(&self) -> u64 {
        let n = self.seq.get() + 1;
        self.seq.set(n);
        n
    }

    fn push_undo(&self) {
        self.undo_stack.borrow_mut().push(self.document.get());
        self.redo_stack.borrow_mut().clear();
    }

    /// Add a node of `kind` at canvas `(x, y)` to the entry flow, select it,
    /// and return its generated id.
    pub fn add_node(&self, kind: NodeKind, x: f64, y: f64) -> String {
        self.push_undo();
        let id = format!("n{}", self.next_seq());
        let node = FlowNode::new(
            &id,
            kind,
            kind.display_name(),
            kind.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x, y }),
        );
        self.document.update(|d| {
            if let Some(flow) = d.flows.first_mut() {
                flow.nodes.push(node);
            }
        });
        self.is_dirty.set(true);
        self.select(Some(id.clone()));
        id
    }

    /// Move a node (drag commit). Snapshots for undo.
    pub fn move_node(&self, id: &str, x: f64, y: f64) {
        self.push_undo();
        self.set_node_position(id, x, y);
    }

    /// Take an undo snapshot of the current document — call once at the start of
    /// an interactive gesture (e.g. a canvas drag) whose individual steps use
    /// [`set_node_position`](Self::set_node_position).
    pub fn begin_edit(&self) {
        self.push_undo();
    }

    /// Set a node's position without snapshotting undo (for smooth drags).
    pub fn set_node_position(&self, id: &str, x: f64, y: f64) {
        self.document.update(|d| {
            if let Some(flow) = d.flows.first_mut() {
                if let Some(node) = flow.nodes.iter_mut().find(|n| n.id == id) {
                    node.ui.position = FlowPosition { x, y };
                }
            }
        });
        self.is_dirty.set(true);
    }

    /// Apply an in-place edit to a node's mutable fields (label / data) without
    /// snapshotting undo — for inspector field edits during typing. Marks the
    /// document dirty. Pair with [`begin_edit`](Self::begin_edit) if a single
    /// undo step per editing session is wanted.
    pub fn edit_node(&self, id: &str, f: impl FnOnce(&mut FlowNode)) {
        self.document.update(|d| {
            if let Some(flow) = d.flows.first_mut() {
                if let Some(node) = flow.nodes.iter_mut().find(|n| n.id == id) {
                    f(node);
                }
            }
        });
        self.is_dirty.set(true);
    }

    /// The entry-flow node with `id`, cloned (for the inspector).
    pub fn node(&self, id: &str) -> Option<FlowNode> {
        self.document
            .with(|d| d.flows.first().and_then(|f| f.nodes.iter().find(|n| n.id == id).cloned()))
    }

    /// Delete a node and any edge that touches it.
    pub fn delete_node(&self, id: &str) {
        self.push_undo();
        self.document.update(|d| {
            if let Some(flow) = d.flows.first_mut() {
                flow.nodes.retain(|n| n.id != id);
                flow.edges.retain(|e| e.from.node != id && e.to.node != id);
            }
        });
        if self.selected_id.get().as_deref() == Some(id) {
            self.selected_id.set(None);
        }
        self.node_breakpoints.update(|b| {
            b.remove(id);
        });
        self.is_dirty.set(true);
    }

    pub fn delete_selected(&self) {
        if let Some(id) = self.selected_id.get() {
            self.delete_node(&id);
        }
    }

    /// Connect two ports with a control edge; returns the new edge id.
    pub fn add_edge(&self, from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> String {
        self.push_undo();
        let id = format!("e{}", self.next_seq());
        let edge = FlowEdge::new(
            &id,
            EdgeKind::Control,
            EdgeEndpoint::new(from_node, from_port),
            EdgeEndpoint::new(to_node, to_port),
        );
        self.document.update(|d| {
            if let Some(flow) = d.flows.first_mut() {
                flow.edges.push(edge);
            }
        });
        self.is_dirty.set(true);
        id
    }

    pub fn select(&self, id: Option<String>) {
        self.selected_id.set(id);
    }

    pub fn set_zoom(&self, zoom: f64) {
        self.zoom.set(zoom.clamp(ZOOM_MIN, ZOOM_MAX));
    }

    pub fn set_pan(&self, x: f64, y: f64) {
        self.pan.set((x, y));
    }

    /// Toggle a breakpoint on a node, but only for executable kinds.
    pub fn toggle_breakpoint(&self, node_id: &str) -> bool {
        let kind = self.document.with(|d| {
            d.flows
                .first()
                .and_then(|f| f.nodes.iter().find(|n| n.id == node_id))
                .map(|n| n.kind)
        });
        if !kind.map(NodeKind::is_executable).unwrap_or(false) {
            return false;
        }
        let mut now_set = false;
        self.node_breakpoints.update(|b| {
            if b.remove(node_id).is_none() {
                b.insert(node_id.to_string(), BreakpointConfig::plain());
                now_set = true;
            }
        });
        now_set
    }

    pub fn set_execution_node(&self, id: Option<String>) {
        self.execution_node.set(id);
    }

    pub fn undo(&self) {
        if let Some(prev) = self.undo_stack.borrow_mut().pop() {
            self.redo_stack.borrow_mut().push(self.document.get());
            self.document.set(prev);
            self.is_dirty.set(true);
        }
    }

    pub fn redo(&self) {
        if let Some(next) = self.redo_stack.borrow_mut().pop() {
            self.undo_stack.borrow_mut().push(self.document.get());
            self.document.set(next);
            self.is_dirty.set(true);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.borrow().is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.borrow().is_empty()
    }

    /// Serialize the current document to `.mflow` JSON and mark it saved.
    pub fn encode(&self) -> Result<String, flowchart_codec::FlowchartCodecError> {
        let json = self.document.with(flowchart_codec::encode_string)?;
        self.is_dirty.set(false);
        Ok(json)
    }

    /// Node count in the entry flow (for tests / status).
    pub fn node_count(&self) -> usize {
        self.document.with(|d| d.flows.first().map(|f| f.nodes.len()).unwrap_or(0))
    }
    pub fn edge_count(&self) -> usize {
        self.document.with(|d| d.flows.first().map(|f| f.edges.len()).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_control_flow_starts_with_two_nodes() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        assert_eq!(vm.node_count(), 2);
        assert_eq!(vm.edge_count(), 1);
    }

    #[test]
    fn add_node_selects_and_marks_dirty() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::SignalFlow);
        let id = vm.add_node(NodeKind::SignalGain, 100.0, 80.0);
        assert_eq!(vm.node_count(), 1);
        assert_eq!(vm.selected_id.get(), Some(id));
        assert!(vm.is_dirty.get());
        assert!(vm.can_undo());
    }

    #[test]
    fn delete_node_removes_connected_edges() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        // entry flow already has main_start --edge--> main_end
        vm.delete_node("main_start");
        assert_eq!(vm.node_count(), 1);
        assert_eq!(vm.edge_count(), 0); // edge removed with the node
    }

    #[test]
    fn move_node_updates_position() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        vm.move_node("main_end", 500.0, 333.0);
        let pos = vm.document.with(|d| {
            d.flows[0].nodes.iter().find(|n| n.id == "main_end").unwrap().ui.position
        });
        assert_eq!((pos.x, pos.y), (500.0, 333.0));
    }

    #[test]
    fn add_edge_appends() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::SignalFlow);
        let a = vm.add_node(NodeKind::SignalConstant, 0.0, 0.0);
        let b = vm.add_node(NodeKind::SignalScope, 200.0, 0.0);
        vm.add_edge(&a, "out", &b, "in");
        assert_eq!(vm.edge_count(), 1);
    }

    #[test]
    fn zoom_is_clamped() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        vm.set_zoom(10.0);
        assert_eq!(vm.zoom.get(), ZOOM_MAX);
        vm.set_zoom(0.01);
        assert_eq!(vm.zoom.get(), ZOOM_MIN);
    }

    #[test]
    fn breakpoint_only_on_executable_nodes() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        // start node is non-executable
        assert!(!vm.toggle_breakpoint("main_start"));
        let id = vm.add_node(NodeKind::Assignment, 10.0, 10.0);
        assert!(vm.toggle_breakpoint(&id));
        assert!(vm.node_breakpoints.get().contains_key(&id));
        assert!(!vm.toggle_breakpoint(&id)); // toggled back off
        assert!(vm.node_breakpoints.get().is_empty());
    }

    #[test]
    fn undo_redo_round_trip() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::SignalFlow);
        vm.add_node(NodeKind::SignalGain, 0.0, 0.0);
        assert_eq!(vm.node_count(), 1);
        vm.undo();
        assert_eq!(vm.node_count(), 0);
        assert!(vm.can_redo());
        vm.redo();
        assert_eq!(vm.node_count(), 1);
    }

    #[test]
    fn new_edit_clears_redo() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::SignalFlow);
        vm.add_node(NodeKind::SignalGain, 0.0, 0.0);
        vm.undo();
        assert!(vm.can_redo());
        vm.add_node(NodeKind::SignalSum, 0.0, 0.0); // new edit invalidates redo
        assert!(!vm.can_redo());
    }

    #[test]
    fn encode_round_trips_and_clears_dirty() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        vm.add_node(NodeKind::Assignment, 1.0, 2.0);
        let json = vm.encode().unwrap();
        assert!(!vm.is_dirty.get());
        let back = flowchart_codec::decode_str(&json).unwrap();
        assert_eq!(back, vm.document.get());
    }

    #[test]
    fn begin_edit_then_drag_is_one_undo_step() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        // Simulate a drag: one snapshot, several position updates.
        vm.begin_edit();
        vm.set_node_position("main_end", 300.0, 300.0);
        vm.set_node_position("main_end", 310.0, 320.0);
        let pos = vm.document.with(|d| {
            d.flows[0].nodes.iter().find(|n| n.id == "main_end").unwrap().ui.position
        });
        assert_eq!((pos.x, pos.y), (310.0, 320.0));
        assert!(vm.is_dirty.get());
        // A single undo restores the pre-drag position.
        vm.undo();
        let pos = vm.document.with(|d| {
            d.flows[0].nodes.iter().find(|n| n.id == "main_end").unwrap().ui.position
        });
        assert_eq!((pos.x, pos.y), (240.0, 220.0));
    }

    #[test]
    fn edit_node_updates_fields_and_marks_dirty() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        let id = vm.add_node(NodeKind::Assignment, 0.0, 0.0);
        vm.is_dirty.set(false);
        vm.edit_node(&id, |n| {
            n.label = "y = 2*x".into();
            n.data.lhs = Some("y".into());
            n.data.rhs = Some("2*x".into());
        });
        assert!(vm.is_dirty.get());
        let node = vm.node(&id).unwrap();
        assert_eq!(node.label, "y = 2*x");
        assert_eq!(node.data.lhs.as_deref(), Some("y"));
        assert_eq!(node.data.rhs.as_deref(), Some("2*x"));
        // edit_node adds no undo step of its own: a single undo removes the
        // whole node added above (back to the 2-node template).
        assert_eq!(vm.node_count(), 3);
        vm.undo();
        assert_eq!(vm.node_count(), 2);
    }

    #[test]
    fn node_lookup_returns_clone_or_none() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        assert!(vm.node("main_start").is_some());
        assert!(vm.node("missing").is_none());
    }

    #[test]
    fn execution_node_marker() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        vm.set_execution_node(Some("main_start".into()));
        assert_eq!(vm.execution_node.get().as_deref(), Some("main_start"));
        vm.set_execution_node(None);
        assert!(vm.execution_node.get().is_none());
    }
}
