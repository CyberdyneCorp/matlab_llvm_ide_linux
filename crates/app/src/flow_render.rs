//! Cairo rendering + hit-testing for a flowchart document. Draws each node as
//! its kind's shape, routes edges orthogonally between port anchors, and marks
//! selection / breakpoints / the paused node. A pan + zoom viewport transforms
//! world → screen. Geometry reads the tested core model (`NodeKind` shapes,
//! ports, anchors); this only paints.

use std::collections::{BTreeMap, BTreeSet};

use gtk::cairo;

use matforge_core::models::flowchart::{
    FlowEdge, FlowNode, FlowPosition, FlowchartDocument, NodeKind, NodeShape, PortAnchor,
    StateDecomposition,
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
    let Some(anchor) = node.kind.port_anchor(port) else {
        return (x + w / 2.0, y + h);
    };
    // Spread ports that share a face evenly along it so multiple inputs/outputs
    // (e.g. an integrator's in / reset / init) don't collapse onto one point.
    let same_face: Vec<&str> = node
        .ports
        .inputs
        .iter()
        .chain(node.ports.outputs.iter())
        .map(|p| p.id.as_str())
        .filter(|id| node.kind.port_anchor(id) == Some(anchor))
        .collect();
    let count = same_face.len().max(1);
    let index = same_face.iter().position(|id| *id == port).unwrap_or(0);
    let frac = (index as f64 + 1.0) / (count as f64 + 1.0);
    match anchor {
        PortAnchor::Top => (x + w * frac, y),
        PortAnchor::Bottom => (x + w * frac, y + h),
        PortAnchor::Left => (x, y + h * frac),
        PortAnchor::Right => (x + w, y + h * frac),
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
    // Edges first (under nodes), then the fan-out junction dots.
    let (routes, junctions) = route_flow(&flow.nodes, &flow.edges);
    for (id, pts) in &routes {
        let is_selected = selected_edge == Some(id.as_str());
        draw_polyline(ctx, pts, is_selected);
    }
    set_rgb(ctx, crate::theme_css::current().text_secondary);
    for j in &junctions {
        ctx.arc(j.0, j.1, 3.0, 0.0, std::f64::consts::TAU);
        ctx.fill().ok();
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

/// A wire leaves/enters a port straight for this length before turning.
const ROUTE_STUB: f64 = 16.0;
/// Clearance kept between a routed wire and a node body.
const ROUTE_CLEAR: f64 = 8.0;
/// Offset of a detour lane past the bounding box of all nodes.
const ROUTE_LANE: f64 = 22.0;

fn anchor_dir(a: PortAnchor) -> (f64, f64) {
    match a {
        PortAnchor::Top => (0.0, -1.0),
        PortAnchor::Bottom => (0.0, 1.0),
        PortAnchor::Left => (-1.0, 0.0),
        PortAnchor::Right => (1.0, 0.0),
    }
}

/// Whether the axis-aligned segment `p`–`q` intersects rect `(x, y, w, h)`.
fn seg_hits_rect(p: (f64, f64), q: (f64, f64), r: (f64, f64, f64, f64)) -> bool {
    let (rx, ry, rw, rh) = r;
    let (sx0, sx1) = (p.0.min(q.0), p.0.max(q.0));
    let (sy0, sy1) = (p.1.min(q.1), p.1.max(q.1));
    sx1 >= rx && sx0 <= rx + rw && sy1 >= ry && sy0 <= ry + rh
}

fn path_clear(pts: &[(f64, f64)], obstacles: &[(f64, f64, f64, f64)]) -> bool {
    pts.windows(2)
        .all(|s| !obstacles.iter().any(|r| seg_hits_rect(s[0], s[1], *r)))
}

/// Drop duplicate and collinear points so the polyline has only real corners.
fn simplify(raw: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(raw.len());
    for p in raw {
        match out.last() {
            Some(&l) if close(l.0, p.0) && close(l.1, p.1) => {}
            _ => out.push(p),
        }
    }
    let mut i = 1;
    while i + 1 < out.len() {
        let (a, b, c) = (out[i - 1], out[i], out[i + 1]);
        let collinear =
            (close(a.0, b.0) && close(b.0, c.0)) || (close(a.1, b.1) && close(b.1, c.1));
        if collinear {
            out.remove(i);
        } else {
            i += 1;
        }
    }
    out
}

/// Orthogonal route for an edge between two ports, avoiding node bodies. The
/// wire leaves the source and enters the target along their port normals, then
/// takes the first candidate path that clears every other node. `lane` pushes
/// the detour lanes further out so unrelated signals don't share one. Returns
/// the raw corner list (`route_flow` simplifies it for drawing/hit-testing).
fn route_edge(
    from: &FlowNode,
    from_port: &str,
    to: &FlowNode,
    to_port: &str,
    obstacles: &[(f64, f64, f64, f64)],
    lane: f64,
) -> Vec<(f64, f64)> {
    let start = port_point(from, from_port);
    let end = port_point(to, to_port);
    let sd = anchor_dir(
        from.kind
            .port_anchor(from_port)
            .unwrap_or(PortAnchor::Right),
    );
    let ed = anchor_dir(to.kind.port_anchor(to_port).unwrap_or(PortAnchor::Left));
    let a = (start.0 + sd.0 * ROUTE_STUB, start.1 + sd.1 * ROUTE_STUB);
    let b = (end.0 + ed.0 * ROUTE_STUB, end.1 + ed.1 * ROUTE_STUB);

    // Inflate obstacles by the clearance; the a→…→b path is tested against these.
    let infl: Vec<(f64, f64, f64, f64)> = obstacles
        .iter()
        .map(|&(x, y, w, h)| {
            (
                x - ROUTE_CLEAR,
                y - ROUTE_CLEAR,
                w + 2.0 * ROUTE_CLEAR,
                h + 2.0 * ROUTE_CLEAR,
            )
        })
        .collect();

    let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let top = infl.iter().map(|r| r.1).fold(a.1.min(b.1), f64::min) - ROUTE_LANE - lane;
    let bot = infl.iter().map(|r| r.1 + r.3).fold(a.1.max(b.1), f64::max) + ROUTE_LANE + lane;
    let left = infl.iter().map(|r| r.0).fold(a.0.min(b.0), f64::min) - ROUTE_LANE - lane;
    let right = infl.iter().map(|r| r.0 + r.2).fold(a.0.max(b.0), f64::max) + ROUTE_LANE + lane;

    // Candidate bend-lists between the two stub points, by aesthetic preference.
    let candidates: [Vec<(f64, f64)>; 8] = [
        vec![(mx, a.1), (mx, b.1)],       // vertical jog at the midpoint
        vec![(a.0, my), (b.0, my)],       // horizontal jog at the midpoint
        vec![(b.0, a.1)],                 // across then in
        vec![(a.0, b.1)],                 // out then across
        vec![(a.0, bot), (b.0, bot)],     // detour under everything
        vec![(a.0, top), (b.0, top)],     // detour over everything
        vec![(right, a.1), (right, b.1)], // detour around the right
        vec![(left, a.1), (left, b.1)],   // detour around the left
    ];

    for bends in candidates {
        let mut path = Vec::with_capacity(bends.len() + 2);
        path.push(a);
        path.extend(bends);
        path.push(b);
        if path_clear(&path, &infl) {
            let mut full = vec![start];
            full.extend(path);
            full.push(end);
            return full;
        }
    }
    // Nothing clear: fall back to the simple midpoint jog.
    vec![start, a, (mx, a.1), (mx, b.1), b, end]
}

/// Per-net lane spacing: how far apart unrelated signals' detour lanes sit.
const ROUTE_LANE_GAP: f64 = 12.0;

/// Last vertex shared by every route in `routes` (their branch point), if they
/// share more than just the source port. Used to place a fan-out junction dot.
fn last_common_vertex(routes: &[&Vec<(f64, f64)>]) -> Option<(f64, f64)> {
    let first = routes.first()?;
    let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
    let mut last = None;
    let mut i = 0;
    while let Some(&p) = first.get(i) {
        if routes
            .iter()
            .all(|r| r.get(i).is_some_and(|&q| close(p, q)))
        {
            last = Some(p);
            i += 1;
        } else {
            break;
        }
    }
    // A real branch only if they ran together past the source port + stub.
    (i >= 2).then_some(last).flatten()
}

/// A routed edge: its id and the world-space corner points of its wire.
type Route = (String, Vec<(f64, f64)>);
/// A routed edge tagged with its source `(node, port)` net, for grouping.
type SourcedRoute<'a> = (String, (&'a str, &'a str), Vec<(f64, f64)>);

/// Route every edge of a flow, giving each net (source port) its own detour
/// lane so unrelated signals don't overlap, and returning the branch-point
/// junctions for nets that fan out. Shared by the renderer and the hit-test.
fn route_flow(nodes: &[FlowNode], edges: &[FlowEdge]) -> (Vec<Route>, Vec<(f64, f64)>) {
    let by_id: BTreeMap<&str, &FlowNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let obstacles: Vec<(f64, f64, f64, f64)> = nodes.iter().map(node_rect).collect();

    // Each distinct source port is a net; index them in first-seen order so each
    // gets a distinct detour lane.
    let mut nets: Vec<(&str, &str)> = Vec::new();
    for e in edges {
        let key = (e.from.node.as_str(), e.from.port.as_str());
        if !nets.contains(&key) {
            nets.push(key);
        }
    }

    let mut raw: Vec<SourcedRoute> = Vec::new();
    for e in edges {
        let (Some(from), Some(to)) = (
            by_id.get(e.from.node.as_str()),
            by_id.get(e.to.node.as_str()),
        ) else {
            continue;
        };
        let src = (e.from.node.as_str(), e.from.port.as_str());
        let lane = nets.iter().position(|k| *k == src).unwrap_or(0) as f64 * ROUTE_LANE_GAP;
        let pts = route_edge(from, &e.from.port, to, &e.to.port, &obstacles, lane);
        raw.push((e.id.clone(), src, pts));
    }

    // Fan-out junction: a dot where a net's branches part ways.
    let mut junctions = Vec::new();
    for net in &nets {
        let group: Vec<&Vec<(f64, f64)>> = raw
            .iter()
            .filter(|(_, s, _)| s == net)
            .map(|(_, _, p)| p)
            .collect();
        if group.len() > 1 {
            if let Some(j) = last_common_vertex(&group) {
                junctions.push(j);
            }
        }
    }

    let routes = raw
        .into_iter()
        .map(|(id, _, pts)| (id, simplify(pts)))
        .collect();
    (routes, junctions)
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
    let p = (world.x, world.y);
    let (routes, _) = route_flow(&flow.nodes, &flow.edges);
    let mut hit = None;
    for (id, pts) in &routes {
        let near = pts
            .windows(2)
            .any(|s| point_segment_distance(p, s[0], s[1]) <= tol);
        if near {
            hit = Some(id.clone());
        }
    }
    hit
}

fn draw_polyline(ctx: &cairo::Context, pts: &[(f64, f64)], selected: bool) {
    if pts.len() < 2 {
        return;
    }
    let end = *pts.last().unwrap();
    let tokens = crate::theme_css::current();
    let color = if selected {
        tokens.blue
    } else {
        tokens.text_secondary
    };
    set_rgb(ctx, color);
    ctx.set_line_width(if selected { 2.6 } else { 1.4 });
    ctx.move_to(pts[0].0, pts[0].1);
    for q in &pts[1..] {
        ctx.line_to(q.0, q.1);
    }
    ctx.stroke().ok();
    // Arrowhead at the destination port.
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
    use matforge_core::models::flowchart::{
        EdgeEndpoint, EdgeKind, FlowPort, FlowPorts, FlowUi, NodeData, SchemaKind,
    };
    use matforge_core::viewmodels::FlowchartViewModel;

    fn gain(id: &str, x: f64, y: f64) -> FlowNode {
        FlowNode::new(
            id,
            NodeKind::SignalGain,
            "",
            NodeKind::SignalGain.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x, y }),
        )
    }

    fn data_edge(id: &str, fnode: &str, tnode: &str) -> FlowEdge {
        FlowEdge::new(
            id,
            EdgeKind::Data,
            EdgeEndpoint::new(fnode, "out"),
            EdgeEndpoint::new(tnode, "in"),
        )
    }

    #[test]
    fn fan_out_source_gets_one_junction_unrelated_nets_get_none() {
        // src.out feeds two targets → exactly one branch junction.
        let nodes = vec![
            gain("src", 0.0, 100.0),
            gain("a", 320.0, 0.0),
            gain("b", 320.0, 200.0),
        ];
        let edges = vec![data_edge("e1", "src", "a"), data_edge("e2", "src", "b")];
        let (routes, junctions) = route_flow(&nodes, &edges);
        assert_eq!(routes.len(), 2);
        assert_eq!(junctions.len(), 1, "a fan-out should emit one junction");

        // Two separate single-sink nets share no source → no junction.
        let nodes = vec![
            gain("p", 0.0, 0.0),
            gain("q", 0.0, 200.0),
            gain("a", 320.0, 0.0),
            gain("b", 320.0, 200.0),
        ];
        let edges = vec![data_edge("e1", "p", "a"), data_edge("e2", "q", "b")];
        let (_, junctions) = route_flow(&nodes, &edges);
        assert!(
            junctions.is_empty(),
            "unrelated nets must not be junctioned"
        );
    }

    #[test]
    fn lane_offset_pushes_a_detour_further_out() {
        // `to` sits left of `from` (a feedback wire): the route must detour
        // around both bodies. A larger lane pushes that detour further out so
        // unrelated signals don't share it.
        let from = gain("from", 200.0, 0.0);
        let to = gain("to", 0.0, 0.0);
        let obstacles: Vec<(f64, f64, f64, f64)> =
            [&from, &to].iter().map(|n| node_rect(n)).collect();
        let r0 = route_edge(&from, "out", &to, "in", &obstacles, 0.0);
        let r1 = route_edge(&from, "out", &to, "in", &obstacles, 40.0);
        assert_ne!(r0, r1, "lane offset should change the route");

        // The wires detour below the nodes; the larger lane sits further down.
        let max_y = |r: &[(f64, f64)]| r.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        assert!(
            max_y(&r1) > max_y(&r0) + 30.0,
            "a larger lane should push the detour further out",
        );
    }

    #[test]
    fn ports_sharing_a_face_are_spread_not_collapsed() {
        // An integrator with three left inputs (in / reset / init) must place
        // them at three distinct points down its left face, not one shared point.
        let node = FlowNode::new(
            "vel",
            NodeKind::SignalIntegrator,
            "",
            FlowPorts {
                inputs: vec![
                    FlowPort::new("in"),
                    FlowPort::new("reset"),
                    FlowPort::new("init"),
                ],
                outputs: vec![FlowPort::new("out")],
            },
            NodeData::default(),
            FlowUi::at(FlowPosition { x: 0.0, y: 0.0 }),
        );
        let (x, y, w, h) = node_rect(&node);
        let p_in = port_point(&node, "in");
        let p_reset = port_point(&node, "reset");
        let p_init = port_point(&node, "init");
        // All on the left face, ordered top→bottom, and distinct.
        for p in [p_in, p_reset, p_init] {
            assert_eq!(p.0, x, "left-face ports stay on the left edge");
        }
        assert!(p_in.1 < p_reset.1 && p_reset.1 < p_init.1);
        assert!((p_reset.1 - (y + h / 2.0)).abs() < 1e-9); // middle of three
                                                           // The lone output stays centered on the right face.
        let p_out = port_point(&node, "out");
        assert_eq!(p_out, (x + w, y + h / 2.0));
    }

    #[test]
    fn route_avoids_a_node_sitting_between_the_ports() {
        // from --> to, with a blocker sitting where the direct mid-jog would
        // cross. The router must pick a path that clears the blocking node.
        let mk = |id: &str, x: f64, y: f64| {
            FlowNode::new(
                id,
                NodeKind::SignalGain,
                "",
                NodeKind::SignalGain.default_ports(),
                NodeData::default(),
                FlowUi::at(FlowPosition { x, y }),
            )
        };
        let from = mk("from", 0.0, 0.0);
        let to = mk("to", 400.0, 160.0);
        let blocker = mk("blk", 180.0, 0.0);
        let obstacles: Vec<(f64, f64, f64, f64)> = [&from, &to, &blocker]
            .iter()
            .map(|n| node_rect(n))
            .collect();

        let pts = route_edge(&from, "out", &to, "in", &obstacles, 0.0);

        // Endpoints are the real ports, and no segment crosses the blocker
        // (inflated by the routing clearance).
        assert_eq!(pts.first().copied(), Some(port_point(&from, "out")));
        assert_eq!(pts.last().copied(), Some(port_point(&to, "in")));
        let (bx, by, bw, bh) = node_rect(&blocker);
        let infl = (
            bx - ROUTE_CLEAR,
            by - ROUTE_CLEAR,
            bw + 2.0 * ROUTE_CLEAR,
            bh + 2.0 * ROUTE_CLEAR,
        );
        assert!(
            pts.windows(2).all(|s| !seg_hits_rect(s[0], s[1], infl)),
            "routed wire must not pass through the blocking node: {pts:?}",
        );
    }

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
            let obstacles: Vec<(f64, f64, f64, f64)> = flow.nodes.iter().map(node_rect).collect();
            let pts = route_edge(from, &edge.from.port, to, &edge.to.port, &obstacles, 0.0);
            // A point on the middle of the first routed segment is a hit.
            let mid = ((pts[0].0 + pts[1].0) / 2.0, (pts[0].1 + pts[1].1) / 2.0);
            let hit = edge_hit_test(doc, 0, FlowPosition { x: mid.0, y: mid.1 }, 3.0);
            assert_eq!(hit.as_deref(), Some(edge.id.as_str()));
            // A point far from any routed segment misses.
            let start = port_point(from, &edge.from.port);
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
