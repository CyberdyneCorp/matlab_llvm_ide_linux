//! Edit-time static analysis of flow diagrams: algebraic-loop detection for
//! signal flow (mirroring the compiler's lowering rule, `mflow_link_roadmap.md`
//! §6.3/§6.4 — a cycle of data wires in which no block breaks direct feedthrough
//! is an algebraic loop the solver cannot resolve without iterating) and a
//! lightweight structural lint for mStateflow state-action snippets.

use std::collections::{BTreeMap, BTreeSet, HashMap};

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

/// First structural problem in a MATLAB action snippet, or `None` when it is
/// balanced. This is a lint, not a full parser: it only checks that `()`, `[]`,
/// and `{}` nest correctly, skipping `%` comments and `"…"` / `'…'` string
/// literals, and treating `'` as the transpose operator (not a string opener)
/// when it directly follows a value. That heuristic avoids false positives on
/// the common authoring typos (a dropped closing paren) without a real parser.
pub fn lint_action(src: &str) -> Option<String> {
    let mut stack: Vec<char> = Vec::new();
    let mut in_string: Option<char> = None;
    // Whether the previous significant char can be followed by a transpose `'`.
    let mut prev_value = false;
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(delim) = in_string {
            // Strings don't span lines in MATLAB; be lenient and end at EOL.
            if c == delim || c == '\n' {
                in_string = None;
                prev_value = c == delim;
            }
            continue;
        }
        match c {
            '%' => {
                while chars.peek().is_some_and(|&n| n != '\n') {
                    chars.next();
                }
                prev_value = false;
            }
            '"' => in_string = Some('"'),
            '\'' if !prev_value => in_string = Some('\''),
            '\'' => {} // transpose operator — leaves prev_value true
            '(' | '[' | '{' => {
                stack.push(c);
                prev_value = false;
            }
            ')' | ']' | '}' => {
                let open = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                match stack.pop() {
                    Some(o) if o == open => {}
                    Some(o) => {
                        let want = match o {
                            '(' => ')',
                            '[' => ']',
                            _ => '}',
                        };
                        return Some(format!("mismatched '{c}' — expected '{want}'"));
                    }
                    None => return Some(format!("unexpected '{c}'")),
                }
                prev_value = true;
            }
            c if c.is_alphanumeric() || c == '_' || c == '.' => prev_value = true,
            _ => prev_value = false, // whitespace and operators
        }
    }
    stack.last().map(|open| {
        let want = match open {
            '(' => ')',
            '[' => ']',
            _ => '}',
        };
        format!("unclosed '{open}' — missing '{want}'")
    })
}

/// Parse an on-event editor body (one `EVENT: action` per line) into the
/// canonical event→action map. Blank lines and lines without a colon are
/// skipped; a leading `on ` keyword is tolerated (`on Tick: x = x + 1`).
pub fn parse_on_event(src: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in src.lines() {
        let Some((head, body)) = line.split_once(':') else {
            continue;
        };
        let event = head
            .trim()
            .strip_prefix("on ")
            .unwrap_or(head.trim())
            .trim();
        if event.is_empty() {
            continue;
        }
        map.insert(event.to_string(), body.trim().to_string());
    }
    map
}

/// Render the event→action map as canonical `EVENT: action` lines, one per
/// event, in sorted (deterministic) order.
pub fn format_on_event(map: &BTreeMap<String, String>) -> String {
    map.iter()
        .map(|(event, action)| format!("{event}: {action}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// State nodes in `flow` whose action snippets fail [`lint_action`], each mapped
/// to its first error message. Checks entry/during/exit and every on-event body.
pub fn state_action_errors(flow: &Flow) -> BTreeMap<String, String> {
    use crate::models::flowchart::NodeKind;
    let mut out = BTreeMap::new();
    for node in &flow.nodes {
        if node.kind != NodeKind::State {
            continue;
        }
        let d = &node.data;
        let bodies = [&d.entry_action, &d.during_action, &d.exit_action];
        let direct = bodies.into_iter().flatten();
        let on_event = d.on_event_actions.iter().flat_map(|m| m.values());
        if let Some(err) = direct.chain(on_event).find_map(|s| lint_action(s)) {
            out.insert(node.id.clone(), err);
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

    #[test]
    fn lint_accepts_balanced_actions() {
        assert_eq!(lint_action("x = x + 1;"), None);
        assert_eq!(lint_action("y = sin(a(1)) + b;"), None);
        assert_eq!(lint_action("m = [1 2 3]';"), None); // transpose after ]
        assert_eq!(lint_action("v = x'; w = (a + b);"), None); // transpose, then parens
        assert_eq!(lint_action("s = 'it (is) fine';"), None); // brackets inside a string
        assert_eq!(lint_action("c = obj.method(x){1};"), None);
        assert_eq!(lint_action(""), None);
    }

    #[test]
    fn lint_flags_unbalanced_brackets() {
        assert!(lint_action("y = sin(x").unwrap().contains("unclosed '('"));
        assert!(lint_action("y = a(1) + b)")
            .unwrap()
            .contains("unexpected ')'"));
        assert!(lint_action("q = f(a, [1 2);")
            .unwrap()
            .contains("mismatched ')'"));
        // A '%' comment must not let a typo hide.
        assert_eq!(lint_action("x = 1; % a (note"), None);
    }

    #[test]
    fn on_event_round_trips_through_canonical_form() {
        let text = "Tick: x = x + 1\non Reset : y = 0\n\nbad line without colon";
        let map = parse_on_event(text);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("Tick").unwrap(), "x = x + 1");
        assert_eq!(map.get("Reset").unwrap(), "y = 0"); // `on ` prefix stripped
                                                        // Canonical, sorted output re-parses to the same map.
        let canon = format_on_event(&map);
        assert_eq!(canon, "Reset: y = 0\nTick: x = x + 1");
        assert_eq!(parse_on_event(&canon), map);
    }

    #[test]
    fn state_action_errors_reports_only_broken_states() {
        let mut good = node("s_ok", NodeKind::State);
        good.data.entry_action = Some("count = count + 1;".into());
        let mut bad = node("s_bad", NodeKind::State);
        bad.data.during_action = Some("y = gain * (x + 1".into());
        let mut bad_event = node("s_evt", NodeKind::State);
        bad_event.data.on_event_actions =
            Some(BTreeMap::from([("E".to_string(), "z = a[".to_string())]));
        // A non-state node with bracket noise is ignored.
        let other = node("g", NodeKind::SignalGain);
        let flow = flow_of(vec![good, bad, bad_event, other], vec![]);
        let errs = state_action_errors(&flow);
        assert_eq!(errs.len(), 2);
        assert!(errs.contains_key("s_bad"));
        assert!(errs.contains_key("s_evt"));
        assert!(!errs.contains_key("s_ok"));
    }
}
