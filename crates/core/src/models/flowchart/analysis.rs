//! Edit-time static analysis of flow diagrams: algebraic-loop detection for
//! signal flow (mirroring the compiler's lowering rule, `mflow_link_roadmap.md`
//! §6.3/§6.4 — a cycle of data wires in which no block breaks direct feedthrough
//! is an algebraic loop the solver cannot resolve without iterating) and
//! mStateflow hierarchy queries (parent/child links, reparent-cycle checks,
//! compound autosizing, and decomposition/history lints).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::document::Flow;
use super::edge::EdgeKind;
use super::node::{FlowPosition, FlowSize, StateDecomposition};

/// The set of block ids lying on an algebraic loop within `flow`.
///
/// Builds a directed graph over the flow's signal wires (every edge that is not
/// a chart `transition`), drops the outgoing edges of loop-breaker blocks
/// (Integrator / Unit Delay / ZOH / …, whose output does not depend on the
/// current-step input), and reports every node that can still reach itself —
/// i.e. every node on a remaining cycle.
pub fn algebraic_loop_nodes(flow: &Flow) -> BTreeSet<String> {
    let n = flow.nodes.len();
    let idx: HashMap<&str, usize> = flow
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();
    let breaker: Vec<bool> = flow
        .nodes
        .iter()
        .map(|node| node.kind.breaks_algebraic_loop())
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in &flow.edges {
        if edge.kind == EdgeKind::Transition {
            continue; // chart transitions are not signal wires
        }
        let (Some(&u), Some(&v)) = (
            idx.get(edge.from.node.as_str()),
            idx.get(edge.to.node.as_str()),
        ) else {
            continue;
        };
        if breaker[u] {
            continue; // a loop-breaker's output is decoupled from its input
        }
        adj[u].push(v);
    }

    let mut on_cycle = BTreeSet::new();
    for start in 0..n {
        let mut stack: Vec<usize> = adj[start].clone();
        let mut seen = vec![false; n];
        while let Some(node) = stack.pop() {
            if node == start {
                on_cycle.insert(flow.nodes[start].id.clone());
                break;
            }
            if seen[node] {
                continue;
            }
            seen[node] = true;
            stack.extend(adj[node].iter().copied());
        }
    }
    on_cycle
}

/// Direct child node ids of the compound state `parent`, in document order.
pub fn children_ids(flow: &Flow, parent: &str) -> Vec<String> {
    flow.nodes
        .iter()
        .filter(|n| n.parent.as_deref() == Some(parent))
        .map(|n| n.id.clone())
        .collect()
}

/// All transitive descendants of `id` (children, grandchildren, …).
pub fn descendants(flow: &Flow, id: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        for child in flow
            .nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(&cur))
        {
            if out.insert(child.id.clone()) {
                stack.push(child.id.clone());
            }
        }
    }
    out
}

/// Whether `node` is `ancestor` itself or nested anywhere beneath it — the test
/// a reparent must fail to avoid building a cycle.
pub fn is_descendant(flow: &Flow, ancestor: &str, node: &str) -> bool {
    if ancestor == node {
        return true;
    }
    // Walk `node`'s parent chain upward (cap at node count to defend against a
    // pre-existing cycle in malformed input).
    let parent_of = |id: &str| {
        flow.nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.parent.clone())
    };
    let mut cur = parent_of(node);
    for _ in 0..flow.nodes.len() {
        match cur {
            Some(p) if p == ancestor => return true,
            Some(p) => cur = parent_of(&p),
            None => return false,
        }
    }
    false
}

/// The decomposition of a state node by id (defaults to `Or` when unset or for
/// a non-existent node).
fn decomposition_of(flow: &Flow, id: &str) -> StateDecomposition {
    flow.nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.data.decomposition)
        .unwrap_or_default()
}

/// Nesting depth of each node (root states = 0), indexed parallel to
/// `flow.nodes`. A broken parent link or cycle clamps at the node count.
fn depths(flow: &Flow) -> Vec<usize> {
    let parent_of = |id: &str| {
        flow.nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.parent.clone())
    };
    flow.nodes
        .iter()
        .map(|node| {
            let mut depth = 0;
            let mut cur = node.parent.clone();
            while let Some(p) = cur {
                depth += 1;
                if depth > flow.nodes.len() {
                    break;
                }
                cur = parent_of(&p);
            }
            depth
        })
        .collect()
}

/// Resize every compound state to wrap its children (a padded box with a header
/// band), processing the deepest compounds first so nested boxes settle before
/// their parents measure them. Mutates node positions/sizes in place.
pub fn autosize_compounds(flow: &mut Flow) {
    const PAD: f64 = 22.0;
    const HEADER: f64 = 26.0;
    let depth = depths(flow);
    let mut order: Vec<usize> = (0..flow.nodes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(depth[i]));
    for i in order {
        let id = flow.nodes[i].id.clone();
        let kids: Vec<(f64, f64, f64, f64)> = flow
            .nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(id.as_str()))
            .map(|n| n.rect())
            .collect();
        if kids.is_empty() {
            continue;
        }
        let minx = kids.iter().map(|r| r.0).fold(f64::INFINITY, f64::min);
        let miny = kids.iter().map(|r| r.1).fold(f64::INFINITY, f64::min);
        let maxx = kids
            .iter()
            .map(|r| r.0 + r.2)
            .fold(f64::NEG_INFINITY, f64::max);
        let maxy = kids
            .iter()
            .map(|r| r.1 + r.3)
            .fold(f64::NEG_INFINITY, f64::max);
        let node = &mut flow.nodes[i];
        node.ui.position = FlowPosition {
            x: minx - PAD,
            y: miny - PAD - HEADER,
        };
        node.ui.size = Some(FlowSize {
            width: (maxx - minx) + 2.0 * PAD,
            height: (maxy - miny) + 2.0 * PAD + HEADER,
        });
    }
}

/// State nodes with a hierarchy lint problem, each mapped to its message:
/// a history junction on an AND (parallel) state, or two AND siblings sharing an
/// execution order.
pub fn hierarchy_errors(flow: &Flow) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    // History-on-AND.
    for node in &flow.nodes {
        if node.data.has_history == Some(true)
            && node.data.decomposition == Some(StateDecomposition::And)
        {
            out.insert(
                node.id.clone(),
                "history junction is not allowed on an AND (parallel) state".to_string(),
            );
        }
    }
    // Execution-order collisions among AND-decomposed siblings.
    let and_parents: BTreeSet<String> = flow
        .nodes
        .iter()
        .filter(|n| {
            n.parent.is_some()
                && decomposition_of(flow, n.parent.as_deref().unwrap()) == StateDecomposition::And
        })
        .filter_map(|n| n.parent.clone())
        .collect();
    for parent in &and_parents {
        let mut by_order: HashMap<u32, Vec<String>> = HashMap::new();
        for child in flow
            .nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(parent.as_str()))
        {
            if let Some(order) = child.data.execution_order {
                by_order.entry(order).or_default().push(child.id.clone());
            }
        }
        for (order, ids) in by_order {
            if ids.len() > 1 {
                for id in ids {
                    out.entry(id).or_insert_with(|| {
                        format!("execution order {order} is used by more than one parallel state")
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::{
        EdgeEndpoint, FlowEdge, FlowKind, FlowNode, FlowPosition, FlowSignature, FlowUi, NodeData,
        NodeKind,
    };

    fn node(id: &str, kind: NodeKind) -> FlowNode {
        FlowNode::new(
            id,
            kind,
            kind.display_name(),
            kind.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x: 0.0, y: 0.0 }),
        )
    }
    fn wire(id: &str, from: &str, to: &str) -> FlowEdge {
        FlowEdge::new(
            id,
            EdgeKind::Data,
            EdgeEndpoint::new(from, "out"),
            EdgeEndpoint::new(to, "in"),
        )
    }
    fn flow_of(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> Flow {
        Flow::new(
            "main",
            FlowKind::Program,
            "main",
            FlowSignature::default(),
            nodes,
            edges,
            None,
        )
    }

    #[test]
    fn direct_feedthrough_cycle_is_flagged() {
        // sum -> gain -> (back to) sum
        let flow = flow_of(
            vec![
                node("sum", NodeKind::SignalSum),
                node("gain", NodeKind::SignalGain),
            ],
            vec![wire("e1", "sum", "gain"), wire("e2", "gain", "sum")],
        );
        let loops = algebraic_loop_nodes(&flow);
        assert!(loops.contains("sum"));
        assert!(loops.contains("gain"));
    }

    #[test]
    fn integrator_breaks_the_loop() {
        // sum -> integrator -> (back to) sum: integrator decouples input/output
        let flow = flow_of(
            vec![
                node("sum", NodeKind::SignalSum),
                node("i", NodeKind::SignalIntegrator),
            ],
            vec![wire("e1", "sum", "i"), wire("e2", "i", "sum")],
        );
        assert!(algebraic_loop_nodes(&flow).is_empty());
    }

    #[test]
    fn acyclic_diagram_has_no_loop() {
        let flow = flow_of(
            vec![
                node("src", NodeKind::SignalConstant),
                node("g", NodeKind::SignalGain),
                node("scope", NodeKind::SignalScope),
            ],
            vec![wire("e1", "src", "g"), wire("e2", "g", "scope")],
        );
        assert!(algebraic_loop_nodes(&flow).is_empty());
    }

    #[test]
    fn self_loop_on_feedthrough_block_is_flagged() {
        let flow = flow_of(
            vec![node("g", NodeKind::SignalGain)],
            vec![wire("e1", "g", "g")],
        );
        assert!(algebraic_loop_nodes(&flow).contains("g"));
    }

    fn state(id: &str, parent: Option<&str>) -> FlowNode {
        let mut n = node(id, NodeKind::State);
        n.parent = parent.map(|s| s.to_string());
        n
    }

    #[test]
    fn descendants_and_cycle_check() {
        // root → mid → leaf
        let flow = flow_of(
            vec![
                state("root", None),
                state("mid", Some("root")),
                state("leaf", Some("mid")),
                state("other", None),
            ],
            vec![],
        );
        let d = descendants(&flow, "root");
        assert!(d.contains("mid") && d.contains("leaf"));
        assert!(!d.contains("other"));
        assert_eq!(children_ids(&flow, "root"), vec!["mid".to_string()]);

        // leaf is nested under root → reparenting root into leaf would cycle.
        assert!(is_descendant(&flow, "root", "leaf"));
        assert!(is_descendant(&flow, "root", "root")); // self counts
        assert!(!is_descendant(&flow, "leaf", "root"));
        assert!(!is_descendant(&flow, "root", "other"));
    }

    #[test]
    fn autosize_wraps_children_with_padding() {
        let mut child_a = state("a", Some("box"));
        child_a.ui.position = FlowPosition { x: 100.0, y: 100.0 };
        child_a.ui.size = Some(FlowSize {
            width: 40.0,
            height: 30.0,
        });
        let mut child_b = state("b", Some("box"));
        child_b.ui.position = FlowPosition { x: 200.0, y: 160.0 };
        child_b.ui.size = Some(FlowSize {
            width: 40.0,
            height: 30.0,
        });
        let mut flow = flow_of(vec![state("box", None), child_a, child_b], vec![]);
        autosize_compounds(&mut flow);
        let (x, y, w, h) = flow.nodes[0].rect();
        // bbox of children: x 100..240, y 100..190 → padded by 22 (and 26 header on top).
        assert_eq!(x, 100.0 - 22.0);
        assert_eq!(y, 100.0 - 22.0 - 26.0);
        assert_eq!(w, 140.0 + 44.0);
        assert_eq!(h, 90.0 + 44.0 + 26.0);
    }

    #[test]
    fn hierarchy_errors_flags_history_on_and_and_order_collisions() {
        let mut and_box = state("p", None);
        and_box.data.decomposition = Some(StateDecomposition::And);
        and_box.data.has_history = Some(true); // illegal on AND

        let mut s1 = state("s1", Some("p"));
        s1.data.execution_order = Some(1);
        let mut s2 = state("s2", Some("p"));
        s2.data.execution_order = Some(1); // collides with s1
        let mut s3 = state("s3", Some("p"));
        s3.data.execution_order = Some(2); // unique → fine

        let flow = flow_of(vec![and_box, s1, s2, s3], vec![]);
        let errs = hierarchy_errors(&flow);
        assert!(errs.get("p").unwrap().contains("history"));
        assert!(errs.contains_key("s1"));
        assert!(errs.contains_key("s2"));
        assert!(!errs.contains_key("s3"));
    }
}
