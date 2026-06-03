//! A self-contained renderer-agnostic parser + layout for the flowchart subset
//! of [Mermaid](https://mermaid.js.org) (`graph` / `flowchart` diagrams). The
//! editor's Markdown preview turns ```mermaid fences into real diagrams without
//! shelling out to node/puppeteer: this module parses the text into a node/edge
//! graph and lays it out with a simple longest-path layering, returning pixel
//! coordinates that a Cairo `DrawingArea` paints (see `app/src/mermaid_render.rs`).
//!
//! Supported: directions `TD`/`TB`/`BT`/`LR`/`RL`; node shapes `[rect]`,
//! `(round)`, `([stadium])`, `((circle))`, `{diamond}`, `{{hexagon}}`; edges
//! `-->`, `---`, `-.->`, `==>` with `|label|` or `-- label -->` labels; chains
//! `A --> B --> C`. Unsupported diagram types return `None` so the caller can
//! fall back to showing the raw source. Pure and unit-tested.

/// Diagram flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Top-down (`TD` / `TB`).
    Down,
    /// Bottom-up (`BT`).
    Up,
    /// Left-right (`LR`).
    Right,
    /// Right-left (`RL`).
    Left,
}

impl Dir {
    fn horizontal(self) -> bool {
        matches!(self, Dir::Right | Dir::Left)
    }
}

/// A node's outline shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Round,
    Stadium,
    Circle,
    Diamond,
    Hexagon,
}

#[derive(Debug, Clone, PartialEq)]
struct Node {
    id: String,
    label: String,
    shape: Shape,
}

#[derive(Debug, Clone, PartialEq)]
struct Edge {
    from: usize,
    to: usize,
    label: Option<String>,
}

/// A parsed flowchart graph (pre-layout).
#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    pub dir: Dir,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

impl Graph {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

/// A laid-out node with pixel geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    pub label: String,
    pub shape: Shape,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl SceneNode {
    fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// A laid-out edge: border-clipped endpoints plus an optional mid-point label.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneEdge {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub label: Option<String>,
    pub label_pos: (f64, f64),
}

/// A fully laid-out diagram ready to paint, sized `width` × `height` pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<SceneNode>,
    pub edges: Vec<SceneEdge>,
}

// ----- parsing ---------------------------------------------------------------

/// Parse Mermaid `src`. Returns `None` if it isn't a supported flowchart.
pub fn parse(src: &str) -> Option<Graph> {
    let mut lines = src.lines().map(str::trim).filter(|l| !l.is_empty());
    let header = lines.next()?;
    // The header sets the direction; some diagrams also put `direction LR` on
    // its own line, which overrides it below.
    let mut dir = parse_header(header)?;

    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    for raw in lines {
        let line = raw.trim();
        if line.starts_with("%%") {
            continue; // comment
        }
        if let Some(rest) = line.strip_prefix("direction ") {
            if let Some(d) = parse_dir(rest.trim()) {
                dir = d;
            }
            continue;
        }
        // Ignore styling / grouping statements we don't render.
        let first = line.split_whitespace().next().unwrap_or("");
        if matches!(
            first,
            "subgraph" | "end" | "style" | "classDef" | "class" | "click" | "linkStyle"
        ) {
            continue;
        }
        // A statement may chain several nodes: A --> B --> C.
        for stmt in line.split(';') {
            parse_statement(stmt.trim(), &mut nodes, &mut edges);
        }
    }

    if nodes.is_empty() {
        return None;
    }
    Some(Graph { dir, nodes, edges })
}

fn parse_header(header: &str) -> Option<Dir> {
    let mut it = header.split_whitespace();
    let kw = it.next()?;
    if !matches!(kw, "graph" | "flowchart") {
        return None;
    }
    Some(it.next().and_then(parse_dir).unwrap_or(Dir::Down))
}

fn parse_dir(s: &str) -> Option<Dir> {
    match s.to_uppercase().as_str() {
        "TD" | "TB" => Some(Dir::Down),
        "BT" => Some(Dir::Up),
        "LR" => Some(Dir::Right),
        "RL" => Some(Dir::Left),
        _ => None,
    }
}

/// Parse one statement (no `;`), appending any nodes/edges it declares.
fn parse_statement(stmt: &str, nodes: &mut Vec<Node>, edges: &mut Vec<Edge>) {
    let chars: Vec<char> = stmt.chars().collect();
    let n = chars.len();
    let mut i = skip_ws(&chars, 0);
    let mut prev: Option<usize> = None;
    let mut pending: Option<String> = None;

    while i < n {
        let Some((node, ni)) = read_node(&chars, i) else { break };
        let idx = intern(nodes, node);
        if let Some(p) = prev {
            edges.push(Edge { from: p, to: idx, label: pending.take() });
        }
        i = skip_ws(&chars, ni);
        match read_link(&chars, i) {
            Some((label, li)) => {
                prev = Some(idx);
                pending = label;
                i = skip_ws(&chars, li);
            }
            None => break,
        }
    }
}

/// Insert `node` (merging labels/shapes by id), returning its index.
fn intern(nodes: &mut Vec<Node>, node: Node) -> usize {
    if let Some(idx) = nodes.iter().position(|x| x.id == node.id) {
        // A later mention carrying a label/shape upgrades the placeholder.
        if node.label != node.id {
            nodes[idx].label = node.label;
            nodes[idx].shape = node.shape;
        }
        idx
    } else {
        nodes.push(node);
        nodes.len() - 1
    }
}

fn skip_ws(s: &[char], mut i: usize) -> usize {
    while i < s.len() && s[i] == ' ' {
        i += 1;
    }
    i
}

/// Read a node `id` plus an optional `[shape label]`, returning it and the next
/// index. Returns `None` if no identifier is present at `i`.
fn read_node(s: &[char], i: usize) -> Option<(Node, usize)> {
    let n = s.len();
    let start = i;
    let mut j = i;
    while j < n && (s[j].is_alphanumeric() || s[j] == '_') {
        j += 1;
    }
    if j == start {
        return None;
    }
    let id: String = s[start..j].iter().collect();

    // Optional shape wrapper.
    if let Some((open, close, shape)) = shape_delims(s, j) {
        let body_start = j + open.len();
        if let Some(end) = find_seq(s, body_start, close) {
            let label: String = s[body_start..end].iter().collect();
            let label = label.trim().trim_matches('"').to_string();
            let label = if label.is_empty() { id.clone() } else { label };
            return Some((Node { id, label, shape }, end + close.len()));
        }
    }
    Some((Node { id: id.clone(), label: id, shape: Shape::Rect }, j))
}

/// Match a node shape opener at `i`, returning (open delim, close delim, shape).
fn shape_delims(s: &[char], i: usize) -> Option<(&'static [char], &'static [char], Shape)> {
    let at = |k: usize| s.get(i + k).copied();
    match (at(0), at(1)) {
        (Some('('), Some('(')) => Some((&['(', '('], &[')', ')'], Shape::Circle)),
        (Some('('), Some('[')) => Some((&['(', '['], &[']', ')'], Shape::Stadium)),
        (Some('('), _) => Some((&['('], &[')'], Shape::Round)),
        (Some('{'), Some('{')) => Some((&['{', '{'], &['}', '}'], Shape::Hexagon)),
        (Some('{'), _) => Some((&['{'], &['}'], Shape::Diamond)),
        (Some('['), Some('[')) => Some((&['[', '['], &[']', ']'], Shape::Rect)),
        (Some('['), _) => Some((&['['], &[']'], Shape::Rect)),
        _ => None,
    }
}

/// Find the first occurrence of `seq` in `s` at or after `from`.
fn find_seq(s: &[char], from: usize, seq: &[char]) -> Option<usize> {
    if seq.is_empty() || from + seq.len() > s.len() {
        return None;
    }
    (from..=s.len() - seq.len()).find(|&k| s[k..k + seq.len()] == *seq)
}

/// Read a link operator at `i` (e.g. `-->`, `---`, `-.->`, `==>`, with an
/// optional `|label|` or `-- label -->`), returning its label and the next index.
fn read_link(s: &[char], i: usize) -> Option<(Option<String>, usize)> {
    let n = s.len();
    if i >= n || (s[i] != '-' && s[i] != '=') {
        return None;
    }
    let edge = s[i];
    let mut j = i;
    while j < n && (s[j] == edge || s[j] == '.') {
        j += 1;
    }
    let mut label: Option<String> = None;

    let after_dashes = j;
    if j < n && s[j] == '>' {
        j += 1; // directed arrowhead
    } else if j < n && s[j] == ' ' {
        // Mid-label form: `-- text -->`. The label is bounded by a *closing*
        // dash/equals run; if none follows, this was an undirected `---` link
        // and the space + text belong to the next node, so we revert.
        let lstart = j;
        let mut k = j;
        while k < n && s[k] != edge {
            k += 1;
        }
        if k < n {
            let lbl: String = s[lstart..k].iter().collect();
            let lbl = lbl.trim().trim_matches('"').to_string();
            if !lbl.is_empty() {
                label = Some(lbl);
            }
            j = k;
            while j < n && (s[j] == edge || s[j] == '.') {
                j += 1;
            }
            if j < n && s[j] == '>' {
                j += 1;
            }
        } else {
            j = after_dashes; // undirected link; leave the rest for the node
        }
    }

    // Optional pipe label after a directed arrow: `-->|text|`.
    let k = skip_ws(s, j);
    if k < n && s[k] == '|' {
        let lstart = k + 1;
        let end = find_seq(s, lstart, &['|']).unwrap_or(n);
        let lbl: String = s[lstart..end].iter().collect();
        let lbl = lbl.trim().trim_matches('"').to_string();
        if !lbl.is_empty() {
            label = Some(lbl);
        }
        j = if end < n { end + 1 } else { end };
    }

    Some((label, j))
}

// ----- layout ----------------------------------------------------------------

const NODE_H: f64 = 40.0;
const CHAR_W: f64 = 7.6;
const NODE_PAD: f64 = 28.0;
const MIN_W: f64 = 64.0;
const LAYER_GAP: f64 = 56.0;
const SIBLING_GAP: f64 = 28.0;
const MARGIN: f64 = 20.0;

fn node_width(label: &str) -> f64 {
    (label.chars().count() as f64 * CHAR_W + NODE_PAD).max(MIN_W)
}

/// Lay `g` out into pixel space using longest-path layering along its direction.
pub fn layout(g: &Graph) -> Scene {
    let n = g.nodes.len();
    let layer = longest_path_layers(n, &g.edges);
    let max_layer = layer.iter().copied().max().unwrap_or(0);

    // Group node indices by layer, preserving insertion order within a layer.
    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (idx, &l) in layer.iter().enumerate() {
        by_layer[l].push(idx);
    }

    let widths: Vec<f64> = g.nodes.iter().map(|nd| node_width(&nd.label)).collect();
    let horizontal = g.dir.horizontal();

    // Cross-axis extent of each layer (sum of node sizes along the cross axis).
    let cross_size = |idx: usize| if horizontal { NODE_H } else { widths[idx] };
    let main_size = |idx: usize| if horizontal { widths[idx] } else { NODE_H };

    let layer_cross_extent: Vec<f64> = by_layer
        .iter()
        .map(|ids| {
            let sum: f64 = ids.iter().map(|&id| cross_size(id)).sum();
            sum + SIBLING_GAP * ids.len().saturating_sub(1) as f64
        })
        .collect();
    let cross_canvas = layer_cross_extent.iter().cloned().fold(0.0, f64::max);

    // Main-axis offset of each layer (running sum of per-layer max main size + gap).
    let mut layer_main_off = vec![0.0; max_layer + 1];
    let mut acc = MARGIN;
    for l in 0..=max_layer {
        layer_main_off[l] = acc;
        let layer_main = by_layer[l]
            .iter()
            .map(|&id| main_size(id))
            .fold(0.0, f64::max);
        acc += layer_main + LAYER_GAP;
    }
    let main_canvas = acc - LAYER_GAP + MARGIN;

    // Place nodes. `reverse` flips the main axis for Up/Left flows.
    let reverse = matches!(g.dir, Dir::Up | Dir::Left);
    let mut nodes: Vec<SceneNode> = vec![
        SceneNode { label: String::new(), shape: Shape::Rect, x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        n
    ];
    for (l, ids) in by_layer.iter().enumerate() {
        let mut cross = MARGIN + (cross_canvas - layer_cross_extent[l]) / 2.0;
        for &id in ids {
            let cs = cross_size(id);
            let main_off = if reverse {
                main_canvas - layer_main_off[l] - main_size(id)
            } else {
                layer_main_off[l]
            };
            let (x, y, w, h) = if horizontal {
                (main_off, cross, widths[id], NODE_H)
            } else {
                (cross, main_off, widths[id], NODE_H)
            };
            nodes[id] = SceneNode {
                label: g.nodes[id].label.clone(),
                shape: g.nodes[id].shape,
                x,
                y,
                w,
                h,
            };
            cross += cs + SIBLING_GAP;
        }
    }

    let (width, height) = if horizontal {
        (main_canvas, cross_canvas + 2.0 * MARGIN)
    } else {
        (cross_canvas + 2.0 * MARGIN, main_canvas)
    };

    // Route edges: clip the center-to-center line to each node's border.
    let edges = g
        .edges
        .iter()
        .map(|e| {
            let a = &nodes[e.from];
            let b = &nodes[e.to];
            let from = clip_to_border(a, b.center());
            let to = clip_to_border(b, a.center());
            let label_pos = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
            SceneEdge { from, to, label: e.label.clone(), label_pos }
        })
        .collect();

    Scene { width, height, nodes, edges }
}

/// Longest-path layering: `layer[v] = max(layer[u]) + 1` over edges `u -> v`,
/// relaxed until stable. Cycles are bounded by the node count.
fn longest_path_layers(n: usize, edges: &[Edge]) -> Vec<usize> {
    let mut layer = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for e in edges {
            if e.from != e.to && layer[e.to] < layer[e.from] + 1 {
                layer[e.to] = layer[e.from] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    layer
}

/// Intersection of the segment from `node`'s center toward `target` with the
/// node's rectangular border.
fn clip_to_border(node: &SceneNode, target: (f64, f64)) -> (f64, f64) {
    let (cx, cy) = node.center();
    let (dx, dy) = (target.0 - cx, target.1 - cy);
    if dx == 0.0 && dy == 0.0 {
        return (cx, cy);
    }
    let hw = node.w / 2.0;
    let hh = node.h / 2.0;
    let tx = if dx != 0.0 { hw / dx.abs() } else { f64::INFINITY };
    let ty = if dy != 0.0 { hh / dy.abs() } else { f64::INFINITY };
    let t = tx.min(ty);
    (cx + dx * t, cy + dy * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_chain() {
        let g = parse("graph TD\nA --> B\nB --> C").unwrap();
        assert_eq!(g.dir, Dir::Down);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn chained_edges_on_one_line() {
        let g = parse("flowchart LR\nA --> B --> C").unwrap();
        assert_eq!(g.dir, Dir::Right);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn shapes_and_labels() {
        let g = parse("graph TD\nA[Start] --> B(stop)\nC{maybe}").unwrap();
        assert_eq!(g.nodes[0].label, "Start");
        assert_eq!(g.nodes[0].shape, Shape::Rect);
        assert_eq!(g.nodes[1].label, "stop");
        assert_eq!(g.nodes[1].shape, Shape::Round);
        assert_eq!(g.nodes[2].shape, Shape::Diamond);
    }

    #[test]
    fn edge_labels_pipe_and_mid() {
        let g = parse("graph TD\nA -->|yes| B\nA -- no --> C").unwrap();
        assert_eq!(g.edges[0].label.as_deref(), Some("yes"));
        assert_eq!(g.edges[1].label.as_deref(), Some("no"));
    }

    #[test]
    fn dotted_and_thick_links() {
        let g = parse("graph LR\nA -.-> B\nB ==> C").unwrap();
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn undirected_link_does_not_eat_target() {
        let g = parse("graph TD\nA --- B").unwrap();
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.nodes[1].id, "B");
    }

    #[test]
    fn non_flowchart_returns_none() {
        assert!(parse("sequenceDiagram\nAlice->>Bob: hi").is_none());
        assert!(parse("").is_none());
        assert!(parse("not a diagram").is_none());
    }

    #[test]
    fn layout_stacks_downward_for_td() {
        let g = parse("graph TD\nA --> B").unwrap();
        let s = layout(&g);
        assert_eq!(s.nodes.len(), 2);
        // B sits below A and the canvas is taller than it is wide-ish.
        assert!(s.nodes[1].y > s.nodes[0].y, "B should be below A");
        assert!(s.height > 2.0 * NODE_H);
    }

    #[test]
    fn layout_flows_rightward_for_lr() {
        let g = parse("flowchart LR\nA --> B").unwrap();
        let s = layout(&g);
        assert!(s.nodes[1].x > s.nodes[0].x, "B should be right of A");
    }

    #[test]
    fn layout_edge_endpoints_on_borders() {
        let g = parse("graph TD\nA --> B").unwrap();
        let s = layout(&g);
        let e = &s.edges[0];
        let a = &s.nodes[0];
        // The edge leaves A's bottom border.
        assert!((e.from.1 - (a.y + a.h)).abs() < 1.0);
    }
}
