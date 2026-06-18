//! Edit-time static analysis of signal-flow diagrams. Currently: algebraic-loop
//! detection, mirroring the compiler's lowering rule (`mflow_link_roadmap.md`
//! §6.3/§6.4) — a cycle of data wires in which no block breaks direct
//! feedthrough is an algebraic loop the solver cannot resolve without iterating.

use std::collections::{BTreeSet, HashMap};

use super::document::Flow;
use super::edge::EdgeKind;

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
}
