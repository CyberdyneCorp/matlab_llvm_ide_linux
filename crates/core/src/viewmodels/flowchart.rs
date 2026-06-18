//! Per-tab flowchart editor view model. Mirrors `FlowchartViewModel`: holds the
//! in-memory document, selection, viewport (pan/zoom), per-node breakpoints, the
//! paused-node marker, and an undo/redo history. Editing operations act on the
//! entry flow (index 0). Snapshots the document before each mutation for undo.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use crate::models::flowchart::{
    EdgeEndpoint, EdgeKind, Flow, FlowEdge, FlowKind, FlowNode, FlowPort, FlowPorts, FlowPosition,
    FlowSignature, FlowUi, FlowchartDocument, NodeData, NodeKind, SchemaKind, SolverConfig,
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
    /// Navigation breadcrumb of `document.flows` indices; the last entry is the
    /// flow currently shown/edited. Root charts stay at `[0]`; entering a
    /// subsystem pushes its sub-flow index.
    pub nav_stack: Property<Vec<usize>>,
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
            nav_stack: Property::new(vec![0]),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            seq: Cell::new(0),
        }
    }

    /// Index into `document.flows` of the flow currently shown/edited.
    pub fn current_flow_index(&self) -> usize {
        self.nav_stack.with(|s| s.last().copied().unwrap_or(0))
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
        let idx = self.current_flow_index();
        self.document.update(|d| {
            if let Some(flow) = d.flows.get_mut(idx) {
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
        let idx = self.current_flow_index();
        self.document.update(|d| {
            if let Some(flow) = d.flows.get_mut(idx) {
                if let Some(node) = flow.nodes.iter_mut().find(|n| n.id == id) {
                    node.ui.position = FlowPosition { x, y };
                }
            }
        });
        self.is_dirty.set(true);
    }

    /// Auto-arrange the diagram into clean layers (one undo step). `horizontal`
    /// flows left→right (signal-flow / Simulink-style); otherwise top→down
    /// (control-flow fluxogramas / state charts).
    pub fn auto_layout(&self, horizontal: bool) {
        let idx = self.current_flow_index();
        let placed = self.document.with(|d| {
            d.flows
                .get(idx)
                .map(|f| arrange(f, horizontal))
                .unwrap_or_default()
        });
        if placed.is_empty() {
            return;
        }
        self.push_undo();
        self.document.update(|d| {
            if let Some(flow) = d.flows.get_mut(idx) {
                let map: std::collections::HashMap<&str, FlowPosition> =
                    placed.iter().map(|(id, p)| (id.as_str(), *p)).collect();
                for node in &mut flow.nodes {
                    if let Some(p) = map.get(node.id.as_str()) {
                        node.ui.position = *p;
                    }
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
        let idx = self.current_flow_index();
        self.document.update(|d| {
            if let Some(flow) = d.flows.get_mut(idx) {
                if let Some(node) = flow.nodes.iter_mut().find(|n| n.id == id) {
                    f(node);
                }
            }
        });
        self.is_dirty.set(true);
    }

    /// The current-flow node with `id`, cloned (for the inspector).
    pub fn node(&self, id: &str) -> Option<FlowNode> {
        let idx = self.current_flow_index();
        self.document.with(|d| {
            d.flows
                .get(idx)
                .and_then(|f| f.nodes.iter().find(|n| n.id == id).cloned())
        })
    }

    /// Block ids sitting on an algebraic loop in the current flow — drives the
    /// inspector note and the canvas warning outline. Empty unless this is a
    /// signal-flow document.
    pub fn algebraic_loop_nodes(&self) -> std::collections::BTreeSet<String> {
        let idx = self.current_flow_index();
        self.document.with(|d| {
            if d.schema_kind() != SchemaKind::SignalFlow {
                return std::collections::BTreeSet::new();
            }
            d.flows
                .get(idx)
                .map(crate::models::flowchart::algebraic_loop_nodes)
                .unwrap_or_default()
        })
    }

    /// The document's solver settings, or the variable-step default if unset.
    pub fn solver_config(&self) -> SolverConfig {
        self.document.with(|d| {
            d.settings
                .as_ref()
                .and_then(|s| s.solver.clone())
                .unwrap_or_else(SolverConfig::default_variable_step)
        })
    }

    /// Replace the solver settings (one undo step) and mark the document dirty.
    pub fn set_solver_config(&self, cfg: SolverConfig) {
        self.push_undo();
        self.document.update(|d| {
            d.settings.get_or_insert_with(Default::default).solver = Some(cfg);
        });
        self.is_dirty.set(true);
    }

    /// Delete a node and any edge that touches it.
    pub fn delete_node(&self, id: &str) {
        self.push_undo();
        let idx = self.current_flow_index();
        self.document.update(|d| {
            if let Some(flow) = d.flows.get_mut(idx) {
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

    /// Whether a new wire from `from_node:from_port` to `to_node:to_port` is
    /// allowed: no self-connection, and the destination input port must be free
    /// (each input port takes a single source). The source port may fan out.
    pub fn can_add_edge(
        &self,
        from_node: &str,
        _from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> bool {
        if from_node == to_node {
            return false;
        }
        let idx = self.current_flow_index();
        self.document.with(|d| {
            d.flows.get(idx).is_some_and(|flow| {
                !flow
                    .edges
                    .iter()
                    .any(|e| e.to.node == to_node && e.to.port == to_port)
            })
        })
    }

    /// Connect two ports with a control edge; returns the new edge id.
    pub fn add_edge(
        &self,
        from_node: &str,
        from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> String {
        self.push_undo();
        let id = format!("e{}", self.next_seq());
        let edge = FlowEdge::new(
            &id,
            EdgeKind::Control,
            EdgeEndpoint::new(from_node, from_port),
            EdgeEndpoint::new(to_node, to_port),
        );
        let idx = self.current_flow_index();
        self.document.update(|d| {
            if let Some(flow) = d.flows.get_mut(idx) {
                flow.edges.push(edge);
            }
        });
        self.is_dirty.set(true);
        id
    }

    /// Enter the sub-flow of a subsystem node (resolved by its `data.flow_id`).
    /// Returns false when the node has no linked sub-flow in the document.
    pub fn enter_subflow(&self, node_id: &str) -> bool {
        let Some(flow_id) = self.node(node_id).and_then(|n| n.data.flow_id) else {
            return false;
        };
        let found = self
            .document
            .with(|d| d.flows.iter().position(|f| f.id == flow_id));
        if let Some(idx) = found {
            self.nav_stack.update(|s| s.push(idx));
            self.selected_id.set(None);
            true
        } else {
            false
        }
    }

    /// Navigate up one level (no-op at the root).
    pub fn nav_back(&self) {
        self.nav_stack.update(|s| {
            if s.len() > 1 {
                s.pop();
            }
        });
        self.selected_id.set(None);
    }

    /// Jump to a breadcrumb depth (0 = root chart).
    pub fn nav_to_depth(&self, depth: usize) {
        self.nav_stack.update(|s| {
            if depth < s.len() {
                s.truncate(depth + 1);
            }
        });
        self.selected_id.set(None);
    }

    /// Breadcrumb of flow names from the root to the current flow.
    pub fn breadcrumb(&self) -> Vec<String> {
        self.document.with(|d| {
            self.nav_stack.with(|s| {
                s.iter()
                    .map(|&i| {
                        d.flows
                            .get(i)
                            .map(|f| f.name.clone())
                            .unwrap_or_else(|| format!("flow {i}"))
                    })
                    .collect()
            })
        })
    }

    /// Extract the given nodes from the current flow into a new subsystem: the
    /// nodes (and their internal wires) move to a fresh sub-flow, boundary wires
    /// are rerouted through inport/outport blocks, and a `signal_subsystem` node
    /// linking that sub-flow replaces them. Returns the new node's id. One undo
    /// step.
    pub fn extract_to_subsystem(&self, ids: &[String]) -> Option<String> {
        if ids.is_empty() {
            return None;
        }
        let idx = self.current_flow_index();
        let set: std::collections::BTreeSet<String> = ids.iter().cloned().collect();
        let sub_flow_id = format!("flow_sub{}", self.next_seq());
        let sub_node_id = format!("n{}", self.next_seq());
        self.push_undo();
        let mut result = None;
        self.document.update(|d| {
            let extracted = match d.flows.get_mut(idx) {
                Some(flow) => extract_subsystem(flow, &set, &sub_flow_id, &sub_node_id),
                None => None,
            };
            if let Some((subflow, node_id)) = extracted {
                d.flows.push(subflow);
                result = Some(node_id);
            }
        });
        if result.is_some() {
            self.is_dirty.set(true);
            self.selected_id.set(result.clone());
        } else {
            // Nothing extracted — drop the speculative undo snapshot.
            self.undo_stack.borrow_mut().pop();
        }
        result
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

    /// A structural execution order for the visual step: a depth-first walk from
    /// the Start node, following each node's outgoing edges in order and visiting
    /// every node once (so loops are walked a single time). Any nodes not reached
    /// from Start are appended in document order, so nothing is skipped. This is
    /// purely structural — no value evaluation, so branches are all explored.
    pub fn execution_order(&self) -> Vec<String> {
        use std::collections::{HashMap, HashSet};
        let idx = self.current_flow_index();
        self.document.with(|doc| {
            let Some(flow) = doc.flows.get(idx) else {
                return Vec::new();
            };
            let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
            for e in &flow.edges {
                adj.entry(e.from.node.as_str())
                    .or_default()
                    .push(e.to.node.as_str());
            }
            let start = flow
                .nodes
                .iter()
                .find(|n| n.kind == NodeKind::Start)
                .or_else(|| flow.nodes.first());
            let mut order: Vec<String> = Vec::new();
            let mut visited: HashSet<&str> = HashSet::new();
            if let Some(s) = start {
                let mut stack: Vec<&str> = vec![s.id.as_str()];
                while let Some(id) = stack.pop() {
                    if !visited.insert(id) {
                        continue;
                    }
                    order.push(id.to_string());
                    if let Some(children) = adj.get(id) {
                        // Reverse so the first outgoing edge is stepped first.
                        for &c in children.iter().rev() {
                            if !visited.contains(c) {
                                stack.push(c);
                            }
                        }
                    }
                }
            }
            for n in &flow.nodes {
                if !visited.contains(n.id.as_str()) {
                    order.push(n.id.clone());
                }
            }
            order
        })
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

    /// Node count in the current flow (for tests / status).
    pub fn node_count(&self) -> usize {
        let idx = self.current_flow_index();
        self.document
            .with(|d| d.flows.get(idx).map(|f| f.nodes.len()).unwrap_or(0))
    }
    pub fn edge_count(&self) -> usize {
        let idx = self.current_flow_index();
        self.document
            .with(|d| d.flows.get(idx).map(|f| f.edges.len()).unwrap_or(0))
    }
}

// Node footprint + spacing used by the auto-layout (`arrange`).
const LAY_NODE_W: f64 = 150.0;
const LAY_NODE_H: f64 = 60.0;
const LAY_LAYER_GAP: f64 = 70.0;
const LAY_SIBLING_GAP: f64 = 36.0;
const LAY_MARGIN: f64 = 40.0;

/// Compute clean layered positions for the entry flow. Layers come from a
/// longest-path pass (back edges dropped so loops/cycles don't blow up), then
/// each layer is centered on the cross axis. `horizontal` flows left→right;
/// otherwise top→down. Pure — the caller applies the result.
/// Move `ids` out of `flow` into a new sub-flow, rerouting boundary wires through
/// inport / outport blocks and replacing them in `flow` with a `signal_subsystem`
/// node (ports matching the boundary). Returns `(sub_flow, subsystem_node_id)`,
/// or `None` when no listed node is present. `flow` is mutated in place.
fn extract_subsystem(
    flow: &mut Flow,
    ids: &std::collections::BTreeSet<String>,
    sub_flow_id: &str,
    sub_node_id: &str,
) -> Option<(Flow, String)> {
    let inside: Vec<FlowNode> = flow
        .nodes
        .iter()
        .filter(|n| ids.contains(&n.id))
        .cloned()
        .collect();
    if inside.is_empty() {
        return None;
    }

    // Centroid + top-left of the extracted block, to place the new node + ports.
    let (mut cx, mut cy) = (0.0, 0.0);
    let (mut minx, mut miny) = (f64::INFINITY, f64::INFINITY);
    for n in &inside {
        let size = n.ui.size.unwrap_or_else(|| n.kind.default_size());
        cx += n.ui.position.x + size.width / 2.0;
        cy += n.ui.position.y + size.height / 2.0;
        minx = minx.min(n.ui.position.x);
        miny = miny.min(n.ui.position.y);
    }
    cx /= inside.len() as f64;
    cy /= inside.len() as f64;

    let mut sub_nodes = inside.clone();
    let mut sub_edges: Vec<FlowEdge> = Vec::new();
    let mut parent_edges: Vec<FlowEdge> = Vec::new();
    let mut in_ports: Vec<FlowPort> = Vec::new();
    let mut out_ports: Vec<FlowPort> = Vec::new();
    let port_node = |id: &str, kind: NodeKind, label: &str, x: f64, y: f64| {
        FlowNode::new(
            id,
            kind,
            label,
            kind.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x, y }),
        )
    };

    for e in &flow.edges {
        match (ids.contains(&e.from.node), ids.contains(&e.to.node)) {
            (true, true) => sub_edges.push(e.clone()),
            (false, false) => parent_edges.push(e.clone()),
            (false, true) => {
                // Entering wire → a subsystem input + an inport inside the sub-flow.
                let k = in_ports.len();
                let port = format!("in{}", k + 1);
                let ip_id = format!("{sub_node_id}_ip{k}");
                sub_nodes.push(port_node(
                    &ip_id,
                    NodeKind::SignalInport,
                    &port,
                    minx - 140.0,
                    miny + k as f64 * 70.0,
                ));
                sub_edges.push(FlowEdge::new(
                    &format!("{ip_id}_e"),
                    EdgeKind::Data,
                    EdgeEndpoint::new(&ip_id, "out"),
                    EdgeEndpoint::new(&e.to.node, &e.to.port),
                ));
                parent_edges.push(FlowEdge::new(
                    &e.id,
                    EdgeKind::Data,
                    e.from.clone(),
                    EdgeEndpoint::new(sub_node_id, &port),
                ));
                in_ports.push(FlowPort::new(&port));
            }
            (true, false) => {
                // Leaving wire → a subsystem output + an outport inside the sub-flow.
                let m = out_ports.len();
                let port = format!("out{}", m + 1);
                let op_id = format!("{sub_node_id}_op{m}");
                sub_nodes.push(port_node(
                    &op_id,
                    NodeKind::SignalOutport,
                    &port,
                    minx + 280.0,
                    miny + m as f64 * 70.0,
                ));
                sub_edges.push(FlowEdge::new(
                    &format!("{op_id}_e"),
                    EdgeKind::Data,
                    e.from.clone(),
                    EdgeEndpoint::new(&op_id, "in"),
                ));
                parent_edges.push(FlowEdge::new(
                    &e.id,
                    EdgeKind::Data,
                    EdgeEndpoint::new(sub_node_id, &port),
                    e.to.clone(),
                ));
                out_ports.push(FlowPort::new(&port));
            }
        }
    }

    let sub_node = FlowNode::new(
        sub_node_id,
        NodeKind::SignalSubsystem,
        "Subsystem",
        FlowPorts {
            inputs: in_ports,
            outputs: out_ports,
        },
        NodeData {
            flow_id: Some(sub_flow_id.to_string()),
            ..NodeData::default()
        },
        FlowUi::at(FlowPosition {
            x: cx - 60.0,
            y: cy - 30.0,
        }),
    );

    flow.nodes.retain(|n| !ids.contains(&n.id));
    flow.nodes.push(sub_node);
    flow.edges = parent_edges;

    let subflow = Flow::new(
        sub_flow_id,
        FlowKind::Function,
        "Subsystem",
        FlowSignature::default(),
        sub_nodes,
        sub_edges,
        flow.layout.clone(),
    );
    Some((subflow, sub_node_id.to_string()))
}

pub fn arrange(
    flow: &crate::models::flowchart::Flow,
    horizontal: bool,
) -> Vec<(String, FlowPosition)> {
    use std::collections::HashMap;
    let n = flow.nodes.len();
    if n == 0 {
        return Vec::new();
    }
    let idx: HashMap<&str, usize> = flow
        .nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| (nd.id.as_str(), i))
        .collect();
    let edges: Vec<(usize, usize)> = flow
        .edges
        .iter()
        .filter_map(|e| {
            Some((
                *idx.get(e.from.node.as_str())?,
                *idx.get(e.to.node.as_str())?,
            ))
        })
        .collect();

    let forward = arrange_forward_edges(n, &edges);
    let mut layer = vec![0usize; n];
    for _ in 0..=n {
        let mut changed = false;
        for &(a, b) in &forward {
            if layer[b] < layer[a] + 1 {
                layer[b] = layer[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        by_layer[l].push(i); // preserves node insertion order within a layer
    }

    let cross_step = if horizontal { LAY_NODE_H } else { LAY_NODE_W } + LAY_SIBLING_GAP;
    let main_step = if horizontal { LAY_NODE_W } else { LAY_NODE_H } + LAY_LAYER_GAP;
    let extent = |count: usize| (count as f64) * cross_step - LAY_SIBLING_GAP;
    let max_extent = by_layer
        .iter()
        .map(|ids| extent(ids.len()))
        .fold(0.0, f64::max);

    let mut out = vec![FlowPosition { x: 0.0, y: 0.0 }; n];
    for (l, ids) in by_layer.iter().enumerate() {
        let main = LAY_MARGIN + l as f64 * main_step;
        let mut cross = LAY_MARGIN + (max_extent - extent(ids.len())) / 2.0;
        for &i in ids {
            out[i] = if horizontal {
                FlowPosition { x: main, y: cross }
            } else {
                FlowPosition { x: cross, y: main }
            };
            cross += cross_step;
        }
    }
    flow.nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| (nd.id.clone(), out[i]))
        .collect()
}

/// Edges with cycle-forming back edges removed (iterative DFS, gray = on stack).
fn arrange_forward_edges(n: usize, edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, &(a, _)) in edges.iter().enumerate() {
        if a < n {
            adj[a].push(i);
        }
    }
    let mut color = vec![0u8; n]; // 0 unvisited, 1 on stack, 2 done
    let mut keep = vec![true; edges.len()];
    for root in 0..n {
        if color[root] != 0 {
            continue;
        }
        color[root] = 1;
        let mut stack = vec![(root, 0usize)];
        while let Some(&(u, ai)) = stack.last() {
            if ai < adj[u].len() {
                stack.last_mut().unwrap().1 += 1;
                let ei = adj[u][ai];
                let v = edges[ei].1;
                match color[v] {
                    1 => keep[ei] = false, // points at an ancestor → back edge
                    0 => {
                        color[v] = 1;
                        stack.push((v, 0));
                    }
                    _ => {}
                }
            } else {
                color[u] = 2;
                stack.pop();
            }
        }
    }
    edges
        .iter()
        .copied()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, e)| e)
        .collect()
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
    fn auto_layout_stacks_top_down_and_is_undoable() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        let a = vm.add_node(NodeKind::Assignment, 999.0, 999.0);
        vm.add_edge("main_start", "out", &a, "in");
        vm.add_edge(&a, "out", "main_end", "in");
        vm.auto_layout(false); // top-down

        let pos = |id: &str| {
            vm.document.with(|d| {
                d.flows[0]
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .unwrap()
                    .ui
                    .position
            })
        };
        // start above a above end; all share a column (top-down layering).
        assert!(pos("main_start").y < pos(&a).y);
        assert!(pos(&a).y < pos("main_end").y);
        assert_eq!(pos("main_start").x, pos(&a).x);
        // One undo restores the pre-layout position.
        assert!(vm.can_undo());
        vm.undo();
        assert_eq!(pos(&a), super::FlowPosition { x: 999.0, y: 999.0 });
    }

    #[test]
    fn auto_layout_horizontal_flows_left_to_right() {
        let vm = FlowchartViewModel::empty("S", SchemaKind::SignalFlow);
        let src = vm.add_node(NodeKind::SignalConstant, 0.0, 0.0);
        let gain = vm.add_node(NodeKind::SignalGain, 0.0, 0.0);
        let sink = vm.add_node(NodeKind::SignalScope, 0.0, 0.0);
        vm.add_edge(&src, "out", &gain, "in");
        vm.add_edge(&gain, "out", &sink, "in");
        vm.auto_layout(true); // left-to-right

        let pos = |id: &str| {
            vm.document.with(|d| {
                d.flows[0]
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .unwrap()
                    .ui
                    .position
            })
        };
        assert!(pos(&src).x < pos(&gain).x && pos(&gain).x < pos(&sink).x);
    }

    #[test]
    fn arrange_terminates_on_cycles() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        let n = vm.add_node(NodeKind::Assignment, 0.0, 0.0);
        vm.add_edge("main_start", "out", &n, "in");
        vm.add_edge(&n, "out", "main_start", "in"); // cycle
        let placed = vm
            .document
            .with(|d| super::arrange(d.flows.first().unwrap(), false));
        assert_eq!(placed.len(), vm.node_count()); // every node placed, no hang
    }

    #[test]
    fn execution_order_starts_at_start_and_covers_all() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        let a = vm.add_node(NodeKind::Assignment, 0.0, 0.0);
        vm.add_edge("main_start", "out", &a, "in");
        vm.add_edge(&a, "out", "main_end", "in");
        let order = vm.execution_order();
        assert_eq!(order.first().map(String::as_str), Some("main_start"));
        // Every node appears exactly once.
        assert_eq!(order.len(), vm.node_count());
        let set: std::collections::HashSet<_> = order.iter().cloned().collect();
        assert_eq!(set.len(), order.len());
        assert!(order.contains(&a));
        assert!(order.contains(&"main_end".to_string()));
    }

    #[test]
    fn execution_order_terminates_on_cycles() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        let n = vm.add_node(NodeKind::Assignment, 0.0, 0.0);
        vm.add_edge("main_start", "out", &n, "in");
        vm.add_edge(&n, "out", "main_start", "in"); // back edge → cycle
        let order = vm.execution_order();
        // Despite the cycle, each node is visited exactly once.
        assert_eq!(order.len(), vm.node_count());
        let set: std::collections::HashSet<_> = order.iter().cloned().collect();
        assert_eq!(set.len(), order.len());
        assert_eq!(order[0], "main_start");
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
            d.flows[0]
                .nodes
                .iter()
                .find(|n| n.id == "main_end")
                .unwrap()
                .ui
                .position
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
    fn can_add_edge_rejects_self_loop_and_occupied_input() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::SignalFlow);
        let a = vm.add_node(NodeKind::SignalConstant, 0.0, 0.0);
        let b = vm.add_node(NodeKind::SignalGain, 200.0, 0.0);
        let sum = vm.add_node(NodeKind::SignalSum, 400.0, 0.0);
        // A fresh connection is allowed; a self-connection is not.
        assert!(vm.can_add_edge(&a, "out", &b, "in"));
        assert!(!vm.can_add_edge(&b, "out", &b, "in"));
        // Once an input port is wired it won't take a second source…
        vm.add_edge(&a, "out", &b, "in");
        assert!(!vm.can_add_edge(&sum, "out", &b, "in"));
        // …but a different input port on the same block is still free, and a
        // source may fan out to multiple destinations.
        assert!(vm.can_add_edge(&a, "out", &sum, "in2"));
        assert!(vm.can_add_edge(&a, "out", &sum, "in1"));
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
            d.flows[0]
                .nodes
                .iter()
                .find(|n| n.id == "main_end")
                .unwrap()
                .ui
                .position
        });
        assert_eq!((pos.x, pos.y), (310.0, 320.0));
        assert!(vm.is_dirty.get());
        // A single undo restores the pre-drag position.
        vm.undo();
        let pos = vm.document.with(|d| {
            d.flows[0]
                .nodes
                .iter()
                .find(|n| n.id == "main_end")
                .unwrap()
                .ui
                .position
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

    #[test]
    fn solver_config_defaults_then_round_trips() {
        use crate::models::flowchart::{SolverAlgorithm, SolverType};
        let vm = FlowchartViewModel::empty("S", SchemaKind::SignalFlow);
        // Unset → variable-step ode45 default.
        let def = vm.solver_config();
        assert_eq!(def.solver_type, Some(SolverType::VariableStep));
        assert_eq!(def.algorithm, Some(SolverAlgorithm::Ode45));

        // Set a fixed-step Euler config; it persists and re-reads.
        let mut cfg = def.clone();
        cfg.solver_type = Some(SolverType::FixedStep);
        cfg.algorithm = Some(SolverAlgorithm::Euler);
        cfg.stop_time = Some(42.0);
        vm.set_solver_config(cfg.clone());
        assert_eq!(vm.solver_config(), cfg);
        assert!(vm.is_dirty.get());
        // It round-trips through the codec (lands in settings.solver).
        let json = vm.encode().unwrap();
        assert!(json.contains("\"stopTime\""));
        // Undo restores the previous (default) solver.
        vm.undo();
        assert_eq!(vm.solver_config().algorithm, Some(SolverAlgorithm::Ode45));
    }

    #[test]
    fn extract_to_subsystem_and_navigate() {
        let vm = FlowchartViewModel::empty("Model", SchemaKind::SignalFlow);
        let a = vm.add_node(NodeKind::SignalGain, 0.0, 0.0);
        let b = vm.add_node(NodeKind::SignalGain, 200.0, 0.0);
        let c = vm.add_node(NodeKind::SignalGain, 400.0, 0.0);
        vm.add_edge(&a, "out", &b, "in"); // entering B
        vm.add_edge(&b, "out", &c, "in"); // leaving B
        assert_eq!(vm.node_count(), 3);

        // Extract B → A, C and a subsystem node remain at the root.
        let sub = vm.extract_to_subsystem(std::slice::from_ref(&b)).unwrap();
        assert_eq!(vm.node_count(), 3); // A, C, subsystem
        assert!(vm.node(&b).is_none()); // B moved out of the root flow
        let sub_node = vm.node(&sub).unwrap();
        assert_eq!(sub_node.kind, NodeKind::SignalSubsystem);
        assert_eq!(sub_node.ports.inputs.len(), 1);
        assert_eq!(sub_node.ports.outputs.len(), 1);
        assert!(sub_node.data.flow_id.is_some());

        // Enter the subsystem: B plus its boundary inport + outport.
        assert!(vm.enter_subflow(&sub));
        assert_eq!(vm.breadcrumb().len(), 2);
        assert_eq!(vm.node_count(), 3); // B + inport + outport
        assert!(vm.node(&b).is_some());

        // Back to the root.
        vm.nav_back();
        assert_eq!(vm.current_flow_index(), 0);
        assert!(vm.node(&a).is_some());

        // Undo reverses the whole extraction.
        vm.undo();
        assert!(vm.node(&b).is_some());
        assert!(vm.node(&sub).is_none());
    }
}
