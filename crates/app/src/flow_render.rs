//! Cairo rendering + hit-testing for a flowchart document. Draws each node as
//! its kind's shape, routes edges orthogonally between port anchors, and marks
//! selection / breakpoints / the paused node. A pan + zoom viewport transforms
//! world → screen. Geometry reads the tested core model (`NodeKind` shapes,
//! ports, anchors); this only paints.

use std::collections::{BTreeMap, BTreeSet};

use gtk::cairo;

use matforge_core::models::flowchart::{
    FlowNode, FlowPosition, FlowchartDocument, NodeKind, NodeShape, PortAnchor, StateDecomposition,
};
use matforge_core::models::BreakpointConfig;
use matforge_core::theme::Rgb;

/// Pan offset + zoom factor.
#[derive(Clone, Copy)]
pub struct Viewport {
    pub pan: (f64, f64),
    pub zoom: f64,
}

fn node_rect(node: &FlowNode) -> (f64, f64, f64, f64) {
    let size = node.ui.size.unwrap_or_else(|| node.kind.default_size());
    (
        node.ui.position.x,
        node.ui.position.y,
        size.width,
        size.height,
    )
}

fn port_point(node: &FlowNode, port: &str) -> (f64, f64) {
    let (x, y, w, h) = node_rect(node);
    match node.kind.port_anchor(port) {
        Some(PortAnchor::Top) => (x + w / 2.0, y),
        Some(PortAnchor::Bottom) => (x + w / 2.0, y + h),
        Some(PortAnchor::Left) => (x, y + h / 2.0),
        Some(PortAnchor::Right) => (x + w, y + h / 2.0),
        None => (x + w / 2.0, y + h),
    }
}

/// Draw the whole document.
#[allow(clippy::too_many_arguments)]
pub fn draw_document(
    ctx: &cairo::Context,
    w: f64,
    h: f64,
    doc: &FlowchartDocument,
    flow_index: usize,
    vp: Viewport,
    selected: Option<&str>,
    selected_edge: Option<&str>,
    breakpoints: &BTreeMap<String, BreakpointConfig>,
    exec_node: Option<&str>,
    algebraic: &BTreeSet<String>,
    lint: &BTreeSet<String>,
    hierarchy_lint: &BTreeSet<String>,
    active: &BTreeSet<String>,
) {
    set_rgb(ctx, crate::theme_css::current().editor_bg);
    ctx.rectangle(0.0, 0.0, w, h);
    ctx.fill().ok();

    ctx.save().ok();
    ctx.translate(vp.pan.0, vp.pan.1);
    ctx.scale(vp.zoom, vp.zoom);

    let Some(flow) = doc.flows.get(flow_index) else {
        ctx.restore().ok();
        return;
    };
    let by_id: BTreeMap<&str, &FlowNode> = flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Edges first (under nodes).
    for edge in &flow.edges {
        let (Some(from), Some(to)) = (
            by_id.get(edge.from.node.as_str()),
            by_id.get(edge.to.node.as_str()),
        ) else {
            continue;
        };
        let start = port_point(from, &edge.from.port);
        let end = port_point(to, &edge.to.port);
        let is_selected = selected_edge == Some(edge.id.as_str());
        draw_edge(
            ctx,
            from.kind.port_anchor(&edge.from.port),
            start,
            end,
            is_selected,
        );
    }

    // Hierarchy: which states have children, each parent's decomposition, and
    // each node's nesting depth (so compound containers paint behind children).
    let has_children: BTreeSet<&str> = flow
        .nodes
        .iter()
        .filter_map(|n| n.parent.as_deref())
        .collect();
    let decomp_of = |id: &str| -> StateDecomposition {
        flow.nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.data.decomposition)
            .unwrap_or_default()
    };
    let depth_of = |node: &FlowNode| -> usize {
        let mut depth = 0;
        let mut cur = node.parent.clone();
        while let Some(p) = cur {
            depth += 1;
            if depth > flow.nodes.len() {
                break;
            }
            cur = flow
                .nodes
                .iter()
                .find(|n| n.id == p)
                .and_then(|n| n.parent.clone());
        }
        depth
    };
    let mut draw_order: Vec<&FlowNode> = flow.nodes.iter().collect();
    draw_order.sort_by_key(|n| depth_of(n)); // stable: shallowest (containers) first

    // Nodes.
    for node in draw_order {
        let (x, y, nw, nh) = node_rect(node);
        let is_compound = has_children.contains(node.id.as_str());
        let is_sel = selected == Some(node.id.as_str());
        let is_exec = exec_node == Some(node.id.as_str());
        let accent = node.kind.category().accent();

        // Body.
        draw_shape(ctx, node.kind.shape(), x, y, nw, nh);
        set_rgb(ctx, crate::theme_css::current().card);
        ctx.fill_preserve().ok();
        ctx.set_line_width(if is_sel { 2.5 } else { 1.3 });
        set_rgb(
            ctx,
            if is_sel {
                crate::theme_css::current().blue
            } else {
                accent
            },
        );
        ctx.stroke().ok();

        // Live active-state halo (mStateflow): a solid green ring on every state
        // currently active during a chart run.
        if active.contains(&node.id) {
            draw_shape(ctx, node.kind.shape(), x - 2.0, y - 2.0, nw + 4.0, nh + 4.0);
            set_rgb(ctx, crate::theme_css::current().green);
            ctx.set_line_width(2.5);
            ctx.stroke().ok();
        }

        if is_exec {
            draw_shape(ctx, node.kind.shape(), x - 2.0, y - 2.0, nw + 4.0, nh + 4.0);
            set_rgb(ctx, crate::theme_css::current().yellow);
            ctx.set_line_width(2.0);
            ctx.stroke().ok();
        }

        // Algebraic-loop warning: an amber dashed halo on every block whose
        // output feeds back into its own input without a state block to break
        // the loop.
        if algebraic.contains(&node.id) {
            draw_shape(ctx, node.kind.shape(), x - 3.0, y - 3.0, nw + 6.0, nh + 6.0);
            set_rgb(ctx, crate::theme_css::current().amber);
            ctx.set_line_width(2.0);
            ctx.set_dash(&[4.0, 3.0], 0.0);
            ctx.stroke().ok();
            ctx.set_dash(&[], 0.0);
        }

        // Action lint warning: a red dashed halo on a state whose entry/during/
        // exit/on-event code has unbalanced brackets.
        if lint.contains(&node.id) {
            draw_shape(ctx, node.kind.shape(), x - 3.0, y - 3.0, nw + 6.0, nh + 6.0);
            set_rgb(ctx, crate::theme_css::current().red);
            ctx.set_line_width(2.0);
            ctx.set_dash(&[3.0, 3.0], 0.0);
            ctx.stroke().ok();
            ctx.set_dash(&[], 0.0);
        }

        // Hierarchy lint warning (history-on-AND, exec-order collision): a red
        // dashed halo, mirroring the algebraic-loop one.
        if hierarchy_lint.contains(&node.id) {
            draw_shape(ctx, node.kind.shape(), x - 3.0, y - 3.0, nw + 6.0, nh + 6.0);
            set_rgb(ctx, crate::theme_css::current().red);
            ctx.set_line_width(2.0);
            ctx.set_dash(&[3.0, 3.0], 0.0);
            ctx.stroke().ok();
            ctx.set_dash(&[], 0.0);
        }

        // Compound state: a header divider under the title, an extra dashed
        // outline when AND-decomposed (parallel), and a history "H" badge.
        if is_compound {
            let is_and = decomp_of(&node.id) == StateDecomposition::And;
            set_rgb(ctx, crate::theme_css::current().border);
            ctx.set_line_width(1.0);
            ctx.move_to(x, y + 24.0);
            ctx.line_to(x + nw, y + 24.0);
            ctx.stroke().ok();
            if is_and {
                draw_shape(ctx, node.kind.shape(), x + 3.0, y + 3.0, nw - 6.0, nh - 6.0);
                set_rgb(ctx, node.kind.category().accent());
                ctx.set_line_width(1.0);
                ctx.set_dash(&[3.0, 3.0], 0.0);
                ctx.stroke().ok();
                ctx.set_dash(&[], 0.0);
            }
            if node.data.has_history == Some(true) {
                set_rgb(ctx, crate::theme_css::current().blue);
                ctx.arc(x + nw - 12.0, y + 12.0, 7.0, 0.0, std::f64::consts::TAU);
                ctx.stroke().ok();
                ctx.set_font_size(10.0);
                ctx.move_to(x + nw - 15.0, y + 15.5);
                ctx.show_text("H").ok();
            }
        }

        // Execution-order badge for an AND parent's child.
        if node
            .parent
            .as_deref()
            .is_some_and(|p| decomp_of(p) == StateDecomposition::And)
        {
            if let Some(order) = node.data.execution_order {
                set_rgb(ctx, node.kind.category().accent());
                ctx.arc(x + 10.0, y + 10.0, 8.0, 0.0, std::f64::consts::TAU);
                ctx.fill().ok();
                set_rgb(ctx, crate::theme_css::current().card);
                ctx.set_font_size(10.0);
                ctx.move_to(x + 7.0, y + 13.5);
                ctx.show_text(&order.to_string()).ok();
            }
        }

        // Port markers — small dots at each port so they're visible targets
        // when drawing a wire by drag (inputs muted, outputs accent-colored).
        for port in &node.ports.inputs {
            let (px, py) = port_point(node, &port.id);
            set_rgb(ctx, crate::theme_css::current().text_secondary);
            ctx.arc(px, py, 2.5, 0.0, std::f64::consts::TAU);
            ctx.fill().ok();
        }
        for port in &node.ports.outputs {
            let (px, py) = port_point(node, &port.id);
            set_rgb(ctx, accent);
            ctx.arc(px, py, 2.5, 0.0, std::f64::consts::TAU);
            ctx.fill().ok();
        }

        // Label.
        set_rgb(ctx, crate::theme_css::current().text_primary);
        ctx.select_font_face(
            "sans-serif",
            cairo::FontSlant::Normal,
            cairo::FontWeight::Normal,
        );
        ctx.set_font_size(12.0);
        let label = node_label(node);
        let ext = ctx.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
        // Compound containers title in the header band; leaves center the label.
        let label_y = if is_compound {
            y + 16.0
        } else {
            y + nh / 2.0 + 4.0
        };
        ctx.move_to(x + (nw - ext) / 2.0, label_y);
        ctx.show_text(&label).ok();

        // Breakpoint dot.
        if breakpoints.contains_key(&node.id) {
            set_rgb(ctx, crate::theme_css::current().red);
            ctx.arc(x + 8.0, y + 8.0, 4.0, 0.0, std::f64::consts::TAU);
            ctx.fill().ok();
        }
    }

    ctx.restore().ok();
}

/// The manhattan routing for an edge as four world-space points
/// `[start, bend1, bend2, end]`. Shared by the renderer and the hit-test so a
/// click follows exactly the drawn line.
fn edge_polyline(
    from_anchor: Option<PortAnchor>,
    start: (f64, f64),
    end: (f64, f64),
) -> [(f64, f64); 4] {
    let horizontal = matches!(
        from_anchor,
        Some(PortAnchor::Left) | Some(PortAnchor::Right)
    );
    if horizontal {
        let mid_x = (start.0 + end.0) / 2.0;
        [start, (mid_x, start.1), (mid_x, end.1), end]
    } else {
        let mid_y = (start.1 + end.1) / 2.0;
        [start, (start.0, mid_y), (end.0, mid_y), end]
    }
}

/// Distance from point `p` to the line segment `a`–`b`.
fn point_segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// The id of the edge whose routed line passes within `tol` world units of
/// `world`, if any. Topmost (last-drawn) edge wins on overlap.
pub fn edge_hit_test(
    doc: &FlowchartDocument,
    flow_index: usize,
    world: FlowPosition,
    tol: f64,
) -> Option<String> {
    let flow = doc.flows.get(flow_index)?;
    let by_id: BTreeMap<&str, &FlowNode> = flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let p = (world.x, world.y);
    let mut hit = None;
    for edge in &flow.edges {
        let (Some(from), Some(to)) = (
            by_id.get(edge.from.node.as_str()),
            by_id.get(edge.to.node.as_str()),
        ) else {
            continue;
        };
        let pts = edge_polyline(
            from.kind.port_anchor(&edge.from.port),
            port_point(from, &edge.from.port),
            port_point(to, &edge.to.port),
        );
        let near = pts
            .windows(2)
            .any(|s| point_segment_distance(p, s[0], s[1]) <= tol);
        if near {
            hit = Some(edge.id.clone());
        }
    }
    hit
}

fn draw_edge(
    ctx: &cairo::Context,
    from_anchor: Option<PortAnchor>,
    start: (f64, f64),
    end: (f64, f64),
    selected: bool,
) {
    let tokens = crate::theme_css::current();
    let color = if selected {
        tokens.blue
    } else {
        tokens.text_secondary
    };
    set_rgb(ctx, color);
    ctx.set_line_width(if selected { 2.6 } else { 1.4 });
    let pts = edge_polyline(from_anchor, start, end);
    ctx.move_to(pts[0].0, pts[0].1);
    for q in &pts[1..] {
        ctx.line_to(q.0, q.1);
    }
    ctx.stroke().ok();
    // Arrowhead.
    set_rgb(ctx, color);
    ctx.arc(
        end.0,
        end.1,
        if selected { 3.5 } else { 2.5 },
        0.0,
        std::f64::consts::TAU,
    );
    ctx.fill().ok();
}

fn draw_shape(ctx: &cairo::Context, shape: NodeShape, x: f64, y: f64, w: f64, h: f64) {
    ctx.new_path();
    match shape {
        NodeShape::Rectangle => ctx.rectangle(x, y, w, h),
        NodeShape::RoundedRect => rounded_rect(ctx, x, y, w, h, 8.0),
        NodeShape::Ellipse => {
            ctx.save().ok();
            ctx.translate(x + w / 2.0, y + h / 2.0);
            ctx.scale(w / 2.0, h / 2.0);
            ctx.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
            ctx.restore().ok();
        }
        NodeShape::Diamond => {
            ctx.move_to(x + w / 2.0, y);
            ctx.line_to(x + w, y + h / 2.0);
            ctx.line_to(x + w / 2.0, y + h);
            ctx.line_to(x, y + h / 2.0);
            ctx.close_path();
        }
        NodeShape::Parallelogram => {
            let s = w * 0.18;
            ctx.move_to(x + s, y);
            ctx.line_to(x + w, y);
            ctx.line_to(x + w - s, y + h);
            ctx.line_to(x, y + h);
            ctx.close_path();
        }
        NodeShape::Hexagon => {
            let s = w * 0.16;
            ctx.move_to(x + s, y);
            ctx.line_to(x + w - s, y);
            ctx.line_to(x + w, y + h / 2.0);
            ctx.line_to(x + w - s, y + h);
            ctx.line_to(x + s, y + h);
            ctx.line_to(x, y + h / 2.0);
            ctx.close_path();
        }
    }
}

fn rounded_rect(ctx: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let deg = std::f64::consts::PI / 180.0;
    ctx.new_sub_path();
    ctx.arc(x + w - r, y + r, r, -90.0 * deg, 0.0);
    ctx.arc(x + w - r, y + h - r, r, 0.0, 90.0 * deg);
    ctx.arc(x + r, y + h - r, r, 90.0 * deg, 180.0 * deg);
    ctx.arc(x + r, y + r, r, 180.0 * deg, 270.0 * deg);
    ctx.close_path();
}

/// World-space point for a screen click under the viewport transform.
pub fn screen_to_world(vp: Viewport, sx: f64, sy: f64) -> FlowPosition {
    FlowPosition {
        x: (sx - vp.pan.0) / vp.zoom,
        y: (sy - vp.pan.1) / vp.zoom,
    }
}

/// Topmost node id containing the world point, if any.
pub fn hit_test(doc: &FlowchartDocument, flow_index: usize, world: FlowPosition) -> Option<String> {
    let flow = doc.flows.get(flow_index)?;
    for node in flow.nodes.iter().rev() {
        let (x, y, w, h) = node_rect(node);
        if world.x >= x && world.x <= x + w && world.y >= y && world.y <= y + h {
            return Some(node.id.clone());
        }
    }
    None
}

/// The text drawn on a node body: the explicit label, else the most useful
/// per-kind text (state name / assignment / expression …), else the kind name.
fn node_label(node: &FlowNode) -> String {
    use matforge_core::models::flowchart::NodeKind::*;
    if !node.label.is_empty() {
        return node.label.clone();
    }
    let d = &node.data;
    let some = |o: &Option<String>| o.clone().filter(|s| !s.is_empty());
    match node.kind {
        // State-chart nodes are keyed by their id (= the state / chart name).
        k if k.is_state_chart() => some(&d.name).unwrap_or_else(|| node.id.clone()),
        Assignment => match (some(&d.lhs), some(&d.rhs)) {
            (Some(l), Some(r)) => format!("{l} = {r}"),
            _ => node.kind.display_name().to_string(),
        },
        Expression | Display => {
            some(&d.expression).unwrap_or_else(|| node.kind.display_name().to_string())
        }
        Constant | Variable => {
            some(&d.name).unwrap_or_else(|| node.kind.display_name().to_string())
        }
        IfBlock | WhileLoop => {
            some(&d.cond).unwrap_or_else(|| node.kind.display_name().to_string())
        }
        FunctionCall => some(&d.callee).unwrap_or_else(|| node.kind.display_name().to_string()),
        _ => node.kind.display_name().to_string(),
    }
}

/// Bounding box `(min_x, min_y, max_x, max_y)` of all nodes in the entry flow,
/// in world coordinates. `None` for an empty flow. Used for zoom-to-fit.
pub fn content_bounds(doc: &FlowchartDocument, flow_index: usize) -> Option<(f64, f64, f64, f64)> {
    let flow = doc.flows.get(flow_index)?;
    let mut it = flow.nodes.iter();
    let first = it.next()?;
    let (x, y, w, h) = node_rect(first);
    let mut b = (x, y, x + w, y + h);
    for node in it {
        let (x, y, w, h) = node_rect(node);
        b.0 = b.0.min(x);
        b.1 = b.1.min(y);
        b.2 = b.2.max(x + w);
        b.3 = b.3.max(y + h);
    }
    Some(b)
}

/// World-space position of a node's port (for the edge-drag rubber band).
pub fn port_world(
    doc: &FlowchartDocument,
    flow_index: usize,
    node_id: &str,
    port: &str,
) -> Option<(f64, f64)> {
    let flow = doc.flows.get(flow_index)?;
    let node = flow.nodes.iter().find(|n| n.id == node_id)?;
    Some(port_point(node, port))
}

/// Nearest *output* port within `radius` world-units of `world`, as
/// `(node_id, port_id)`. Used to start an edge drag from a port stub.
pub fn output_port_hit(
    doc: &FlowchartDocument,
    flow_index: usize,
    world: FlowPosition,
    radius: f64,
) -> Option<(String, String)> {
    let flow = doc.flows.get(flow_index)?;
    let mut best: Option<(f64, String, String)> = None;
    for node in &flow.nodes {
        for p in &node.ports.outputs {
            let (px, py) = port_point(node, &p.id);
            let d = ((px - world.x).powi(2) + (py - world.y).powi(2)).sqrt();
            if d <= radius && best.as_ref().map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, node.id.clone(), p.id.clone()));
            }
        }
    }
    best.map(|(_, n, p)| (n, p))
}

/// Input port of `node_id` closest to `world` (the drop target's landing port).
/// Falls back to `"in"` when the node declares no input ports.
pub fn nearest_input_port(
    doc: &FlowchartDocument,
    flow_index: usize,
    node_id: &str,
    world: FlowPosition,
) -> Option<String> {
    let flow = doc.flows.get(flow_index)?;
    let node = flow.nodes.iter().find(|n| n.id == node_id)?;
    if node.ports.inputs.is_empty() {
        return None;
    }
    node.ports
        .inputs
        .iter()
        .map(|p| {
            let (px, py) = port_point(node, &p.id);
            let d = (px - world.x).powi(2) + (py - world.y).powi(2);
            (d, p.id.clone())
        })
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, id)| id)
}

/// Palette kinds offered for a given document dialect.
pub fn palette_kinds(doc: &FlowchartDocument) -> Vec<NodeKind> {
    use matforge_core::models::flowchart::SchemaKind;
    match doc.schema_kind() {
        SchemaKind::SignalFlow => vec![
            NodeKind::SignalConstant,
            NodeKind::SignalSine,
            NodeKind::SignalGain,
            NodeKind::SignalSum,
            NodeKind::SignalIntegrator,
            NodeKind::SignalScope,
        ],
        SchemaKind::StateChart => vec![NodeKind::State, NodeKind::JunctionConnective],
        SchemaKind::ControlFlow => vec![
            NodeKind::Assignment,
            NodeKind::IfBlock,
            NodeKind::ForLoop,
            NodeKind::WhileLoop,
            NodeKind::Display,
            NodeKind::FunctionCall,
        ],
    }
}

fn set_rgb(ctx: &cairo::Context, c: Rgb) {
    let (r, g, b) = c.to_unit();
    ctx.set_source_rgb(r, g, b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use matforge_core::models::flowchart::SchemaKind;
    use matforge_core::viewmodels::FlowchartViewModel;

    #[test]
    fn point_segment_distance_handles_endpoints_and_interior() {
        let a = (0.0, 0.0);
        let b = (10.0, 0.0);
        // Perpendicular drop onto the interior.
        assert!((point_segment_distance((5.0, 3.0), a, b) - 3.0).abs() < 1e-9);
        // Past an endpoint clamps to the endpoint.
        assert!((point_segment_distance((-4.0, 0.0), a, b) - 4.0).abs() < 1e-9);
        // Degenerate (zero-length) segment is the point distance.
        assert!((point_segment_distance((3.0, 4.0), a, a) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn edge_hit_test_picks_the_routed_line_and_misses_empty_space() {
        let vm = FlowchartViewModel::empty("D", SchemaKind::ControlFlow);
        vm.move_node("main_start", 0.0, 0.0);
        vm.move_node("main_end", 240.0, 120.0);
        vm.document.with(|doc| {
            let flow = &doc.flows[0];
            let edge = &flow.edges[0];
            let from = flow.nodes.iter().find(|n| n.id == edge.from.node).unwrap();
            let to = flow.nodes.iter().find(|n| n.id == edge.to.node).unwrap();
            let start = port_point(from, &edge.from.port);
            let end = port_point(to, &edge.to.port);
            let pts = edge_polyline(from.kind.port_anchor(&edge.from.port), start, end);
            // A point on the middle of the first routed segment is a hit.
            let mid = ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0);
            let hit = edge_hit_test(doc, 0, FlowPosition { x: mid.0, y: mid.1 }, 3.0);
            assert_eq!(hit.as_deref(), Some(edge.id.as_str()));
            // A point far from any routed segment misses.
            let miss = edge_hit_test(
                doc,
                0,
                FlowPosition {
                    x: start.0,
                    y: start.1 + 400.0,
                },
                3.0,
            );
            assert!(miss.is_none());
        });
    }
}
