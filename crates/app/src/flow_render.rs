//! Cairo rendering + hit-testing for a flowchart document. Draws each node as
//! its kind's shape, routes edges orthogonally between port anchors, and marks
//! selection / breakpoints / the paused node. A pan + zoom viewport transforms
//! world → screen. Geometry reads the tested core model (`NodeKind` shapes,
//! ports, anchors); this only paints.

use std::collections::BTreeMap;

use gtk::cairo;

use matforge_core::models::flowchart::{
    FlowNode, FlowPosition, FlowchartDocument, NodeKind, NodeShape, PortAnchor,
};
use matforge_core::models::BreakpointConfig;
use matforge_core::theme::{palette, Rgb};

/// Pan offset + zoom factor.
#[derive(Clone, Copy)]
pub struct Viewport {
    pub pan: (f64, f64),
    pub zoom: f64,
}

fn node_rect(node: &FlowNode) -> (f64, f64, f64, f64) {
    let size = node.ui.size.unwrap_or_else(|| node.kind.default_size());
    (node.ui.position.x, node.ui.position.y, size.width, size.height)
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
pub fn draw_document(
    ctx: &cairo::Context,
    w: f64,
    h: f64,
    doc: &FlowchartDocument,
    vp: Viewport,
    selected: Option<&str>,
    breakpoints: &BTreeMap<String, BreakpointConfig>,
    exec_node: Option<&str>,
) {
    set_rgb(ctx, palette::EDITOR_BACKGROUND);
    ctx.rectangle(0.0, 0.0, w, h);
    ctx.fill().ok();

    ctx.save().ok();
    ctx.translate(vp.pan.0, vp.pan.1);
    ctx.scale(vp.zoom, vp.zoom);

    let Some(flow) = doc.flows.first() else {
        ctx.restore().ok();
        return;
    };
    let by_id: BTreeMap<&str, &FlowNode> = flow.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Edges first (under nodes).
    for edge in &flow.edges {
        let (Some(from), Some(to)) = (by_id.get(edge.from.node.as_str()), by_id.get(edge.to.node.as_str()))
        else {
            continue;
        };
        let start = port_point(from, &edge.from.port);
        let end = port_point(to, &edge.to.port);
        draw_edge(ctx, from.kind.port_anchor(&edge.from.port), start, end);
    }

    // Nodes.
    for node in &flow.nodes {
        let (x, y, nw, nh) = node_rect(node);
        let is_sel = selected == Some(node.id.as_str());
        let is_exec = exec_node == Some(node.id.as_str());
        let accent = node.kind.category().accent();

        // Body.
        draw_shape(ctx, node.kind.shape(), x, y, nw, nh);
        set_rgb(ctx, palette::CARD);
        ctx.fill_preserve().ok();
        ctx.set_line_width(if is_sel { 2.5 } else { 1.3 });
        set_rgb(ctx, if is_sel { palette::ACCENT_BLUE } else { accent });
        ctx.stroke().ok();

        if is_exec {
            draw_shape(ctx, node.kind.shape(), x - 2.0, y - 2.0, nw + 4.0, nh + 4.0);
            set_rgb(ctx, palette::ACCENT_YELLOW);
            ctx.set_line_width(2.0);
            ctx.stroke().ok();
        }

        // Label.
        set_rgb(ctx, palette::TEXT_PRIMARY);
        ctx.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        ctx.set_font_size(12.0);
        let label = node_label(node);
        let ext = ctx.text_extents(&label).map(|e| e.width()).unwrap_or(0.0);
        ctx.move_to(x + (nw - ext) / 2.0, y + nh / 2.0 + 4.0);
        ctx.show_text(&label).ok();

        // Breakpoint dot.
        if breakpoints.contains_key(&node.id) {
            set_rgb(ctx, palette::ACCENT_RED);
            ctx.arc(x + 8.0, y + 8.0, 4.0, 0.0, std::f64::consts::TAU);
            ctx.fill().ok();
        }
    }

    ctx.restore().ok();
}

fn draw_edge(ctx: &cairo::Context, from_anchor: Option<PortAnchor>, start: (f64, f64), end: (f64, f64)) {
    set_rgb(ctx, palette::TEXT_SECONDARY);
    ctx.set_line_width(1.4);
    ctx.move_to(start.0, start.1);
    let horizontal = matches!(from_anchor, Some(PortAnchor::Left) | Some(PortAnchor::Right));
    if horizontal {
        let mid_x = (start.0 + end.0) / 2.0;
        ctx.line_to(mid_x, start.1);
        ctx.line_to(mid_x, end.1);
    } else {
        let mid_y = (start.1 + end.1) / 2.0;
        ctx.line_to(start.0, mid_y);
        ctx.line_to(end.0, mid_y);
    }
    ctx.line_to(end.0, end.1);
    ctx.stroke().ok();
    // Arrowhead.
    set_rgb(ctx, palette::TEXT_SECONDARY);
    ctx.arc(end.0, end.1, 2.5, 0.0, std::f64::consts::TAU);
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
    FlowPosition { x: (sx - vp.pan.0) / vp.zoom, y: (sy - vp.pan.1) / vp.zoom }
}

/// Topmost node id containing the world point, if any.
pub fn hit_test(doc: &FlowchartDocument, world: FlowPosition) -> Option<String> {
    let flow = doc.flows.first()?;
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
        Expression | Display => some(&d.expression).unwrap_or_else(|| node.kind.display_name().to_string()),
        Constant | Variable => some(&d.name).unwrap_or_else(|| node.kind.display_name().to_string()),
        IfBlock | WhileLoop => some(&d.cond).unwrap_or_else(|| node.kind.display_name().to_string()),
        FunctionCall => some(&d.callee).unwrap_or_else(|| node.kind.display_name().to_string()),
        _ => node.kind.display_name().to_string(),
    }
}

/// Bounding box `(min_x, min_y, max_x, max_y)` of all nodes in the entry flow,
/// in world coordinates. `None` for an empty flow. Used for zoom-to-fit.
pub fn content_bounds(doc: &FlowchartDocument) -> Option<(f64, f64, f64, f64)> {
    let flow = doc.flows.first()?;
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
pub fn port_world(doc: &FlowchartDocument, node_id: &str, port: &str) -> Option<(f64, f64)> {
    let flow = doc.flows.first()?;
    let node = flow.nodes.iter().find(|n| n.id == node_id)?;
    Some(port_point(node, port))
}

/// Nearest *output* port within `radius` world-units of `world`, as
/// `(node_id, port_id)`. Used to start an edge drag from a port stub.
pub fn output_port_hit(
    doc: &FlowchartDocument,
    world: FlowPosition,
    radius: f64,
) -> Option<(String, String)> {
    let flow = doc.flows.first()?;
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
pub fn nearest_input_port(doc: &FlowchartDocument, node_id: &str, world: FlowPosition) -> Option<String> {
    let flow = doc.flows.first()?;
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
