//! A self-contained renderer-agnostic parser + layout for the [Mermaid](https://mermaid.js.org)
//! diagram subset the editor's Markdown preview renders. ```mermaid fences become
//! real Cairo diagrams without shelling out to node/puppeteer; this module parses
//! the text and lays it out, returning pixel coordinates a Cairo `DrawingArea`
//! paints (see `app/src/mermaid_render.rs`).
//!
//! Three diagram families are supported via [`parse`] → [`Diagram`]:
//! - **Flowcharts** (`graph` / `flowchart`): directions `TD`/`TB`/`BT`/`LR`/`RL`;
//!   node shapes `[rect]`, `(round)`, `([stadium])`, `((circle))`, `{diamond}`,
//!   `{{hexagon}}`; edges `-->`, `---`, `-.->`, `==>` with `|label|` or
//!   `-- label -->` labels; chains `A --> B --> C`. Laid out by longest-path
//!   layering → [`layout`] returns a [`Scene`] of nodes + edges.
//! - **Sequence diagrams** (`sequenceDiagram`): `participant`/`actor` (with
//!   `as` aliases), messages `->>`/`-->>`/`->`/`-->`/`-x`/`--x`/`-)`/`--)` with
//!   `: text`, self-messages, and `Note over/left of/right of`. Laid out into
//!   lifelines + messages → [`layout_sequence`] returns a [`SeqScene`].
//! - **Class diagrams** (`classDiagram`): `class Name { members }` blocks and
//!   `Name : member` shorthands; relations `<|--`, `*--`, `o--`, `-->`, `..>`,
//!   `<|..`, `--`/`..` (with cardinality strings stripped) and `: label`. Laid
//!   out by layering → [`layout_class`] returns a [`ClassScene`].
//! - **State diagrams** (`stateDiagram-v2` / `stateDiagram`): `[*]` start/end
//!   pseudo-states, transitions `A --> B : label`, `state "desc" as id` and
//!   `id : desc` declarations, and `<<choice>>` states. Laid out by layering →
//!   [`layout_state`] returns a [`StateScene`].
//!
//! Unsupported diagram types return `None` so the caller falls back to the raw
//! source. Pure and unit-tested.

/// A parsed Mermaid diagram, tagged by family. Returned by [`parse`].
#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    /// A `graph` / `flowchart` diagram; lay out with [`layout`].
    Flow(Graph),
    /// A `sequenceDiagram`; lay out with [`layout_sequence`].
    Sequence(Sequence),
    /// A `classDiagram`; lay out with [`layout_class`].
    Class(ClassDiagram),
    /// A `stateDiagram` / `stateDiagram-v2`; lay out with [`layout_state`].
    State(StateMachine),
}

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

/// Parse Mermaid `src` into a [`Diagram`], or `None` if it isn't a supported type.
pub fn parse(src: &str) -> Option<Diagram> {
    let header = src.lines().map(str::trim).find(|l| !l.is_empty())?;
    let kw = header.split_whitespace().next().unwrap_or("");
    match kw {
        "graph" | "flowchart" => parse_flow(src).map(Diagram::Flow),
        "sequenceDiagram" => parse_sequence(src).map(Diagram::Sequence),
        "classDiagram" => parse_class(src).map(Diagram::Class),
        "stateDiagram" | "stateDiagram-v2" => parse_state(src).map(Diagram::State),
        _ => None,
    }
}

/// Parse a `graph` / `flowchart` diagram. Returns `None` if it has no nodes.
fn parse_flow(src: &str) -> Option<Graph> {
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
/// relaxed until stable. Back edges (which would form cycles, e.g. a state
/// machine's `Running -> Paused -> Running`) are dropped first so cyclic graphs
/// lay out as a DAG instead of inflating every node's layer.
fn longest_path_layers(n: usize, edges: &[Edge]) -> Vec<usize> {
    let forward = remove_back_edges(n, edges);
    let mut layer = vec![0usize; n];
    for _ in 0..=n {
        let mut changed = false;
        for e in &forward {
            if layer[e.to] < layer[e.from] + 1 {
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

/// Return `edges` with cycle-forming back edges removed, found by a DFS that
/// drops any edge pointing at a node still on the recursion stack.
fn remove_back_edges(n: usize, edges: &[Edge]) -> Vec<Edge> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n]; // node -> outgoing edge indices
    for (i, e) in edges.iter().enumerate() {
        if e.from < n && e.to < n {
            adj[e.from].push(i);
        }
    }
    let mut color = vec![0u8; n]; // 0 = unvisited, 1 = on stack, 2 = done
    let mut keep = vec![true; edges.len()];
    for root in 0..n {
        if color[root] != 0 {
            continue;
        }
        color[root] = 1;
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&(u, ai)) = stack.last() {
            if ai < adj[u].len() {
                stack.last_mut().unwrap().1 += 1;
                let ei = adj[u][ai];
                let v = edges[ei].to;
                match color[v] {
                    1 => keep[ei] = false, // points at an ancestor → back edge
                    0 => {
                        color[v] = 1;
                        stack.push((v, 0));
                    }
                    _ => {} // forward / cross edge → keep
                }
            } else {
                color[u] = 2;
                stack.pop();
            }
        }
    }
    edges
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, e)| e.clone())
        .collect()
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

// ===== sequence diagrams =====================================================

/// The arrowhead style at a message's target end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    /// No arrowhead (`->` / `-->`): a plain line.
    None,
    /// A solid filled triangle (`->>` / `-->>`).
    Head,
    /// An open async arrow (`-)` / `--)`).
    Open,
    /// A cross / lost-message marker (`-x` / `--x`).
    Cross,
}

#[derive(Debug, Clone, PartialEq)]
struct Participant {
    label: String,
}

/// Which participants a note is attached to.
#[derive(Debug, Clone, PartialEq)]
enum NoteSpan {
    Over(usize, Option<usize>),
    LeftOf(usize),
    RightOf(usize),
}

#[derive(Debug, Clone, PartialEq)]
enum SeqEvent {
    Message {
        from: usize,
        to: usize,
        label: String,
        arrow: Arrow,
        dashed: bool,
    },
    Note {
        span: NoteSpan,
        label: String,
    },
}

/// A parsed sequence diagram (pre-layout): ordered participant columns + events.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    participants: Vec<Participant>,
    events: Vec<SeqEvent>,
}

impl Sequence {
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

/// A laid-out box (a participant header/footer, or a note) in a [`SeqScene`].
#[derive(Debug, Clone, PartialEq)]
pub struct SeqBox {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// True for note boxes (rendered with a distinct tint), false for participants.
    pub note: bool,
}

/// A laid-out message arrow in a [`SeqScene`].
#[derive(Debug, Clone, PartialEq)]
pub struct SeqMessage {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub label: String,
    pub dashed: bool,
    pub arrow: Arrow,
    /// A message to the same lifeline; the renderer draws a small loop instead
    /// of a straight arrow. `from`/`to` carry the loop's top/bottom anchor.
    pub self_loop: bool,
}

/// A fully laid-out sequence diagram, sized `width` × `height` pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct SeqScene {
    pub width: f64,
    pub height: f64,
    /// One vertical dashed lifeline per participant: `(x, y_top, y_bottom)`.
    pub lifelines: Vec<(f64, f64, f64)>,
    pub boxes: Vec<SeqBox>,
    pub messages: Vec<SeqMessage>,
}

/// Parse a `sequenceDiagram`. Returns `None` if it declares no participants.
fn parse_sequence(src: &str) -> Option<Sequence> {
    let mut lines = src.lines().map(str::trim).filter(|l| !l.is_empty());
    lines.next()?; // consume the `sequenceDiagram` header

    let mut ids: Vec<String> = Vec::new(); // participant ids in column order
    let mut participants: Vec<Participant> = Vec::new();
    let mut events: Vec<SeqEvent> = Vec::new();

    // Intern a participant id, creating it (label = id) on first use.
    let intern = |ids: &mut Vec<String>, parts: &mut Vec<Participant>, id: &str| -> usize {
        if let Some(i) = ids.iter().position(|x| x == id) {
            i
        } else {
            ids.push(id.to_string());
            parts.push(Participant { label: id.to_string() });
            ids.len() - 1
        }
    };

    for line in lines {
        if line.starts_with("%%") {
            continue;
        }
        let first = line.split_whitespace().next().unwrap_or("");

        // Declarations: `participant A`, `actor A`, `participant A as Alice`.
        if first == "participant" || first == "actor" {
            let rest = line[first.len()..].trim();
            let (id, label) = match rest.split_once(" as ") {
                Some((id, label)) => (id.trim(), label.trim()),
                None => (rest, rest),
            };
            let idx = intern(&mut ids, &mut participants, id);
            participants[idx].label = label.to_string();
            continue;
        }

        // Notes: `Note over A,B: text`, `Note left of A: text`, `Note right of A: text`.
        if first == "Note" || first == "note" {
            if let Some((span, label)) = parse_note(&line[first.len()..], &mut ids, &mut participants, &intern) {
                events.push(SeqEvent::Note { span, label });
            }
            continue;
        }

        // Block keywords we don't lay out yet — skip without breaking the diagram.
        if matches!(
            first,
            "loop" | "alt" | "opt" | "par" | "and" | "else" | "end" | "rect"
                | "activate" | "deactivate" | "autonumber" | "critical" | "break"
        ) {
            continue;
        }

        // Otherwise: a message `A->>B: text`.
        if let Some((from, to, arrow, dashed, label)) =
            parse_message(line, &mut ids, &mut participants, &intern)
        {
            events.push(SeqEvent::Message { from, to, label, arrow, dashed });
        }
    }

    if participants.is_empty() {
        return None;
    }
    Some(Sequence { participants, events })
}

type Intern<'a> = dyn Fn(&mut Vec<String>, &mut Vec<Participant>, &str) -> usize + 'a;

/// Parse the part of a `Note ...` line after the `Note` keyword.
fn parse_note(
    rest: &str,
    ids: &mut Vec<String>,
    parts: &mut Vec<Participant>,
    intern: &Intern,
) -> Option<(NoteSpan, String)> {
    let (spec, label) = rest.split_once(':')?;
    let label = label.trim().to_string();
    let spec = spec.trim();
    if let Some(rest) = spec.strip_prefix("over ") {
        let mut names = rest.split(',').map(str::trim);
        let a = intern(ids, parts, names.next()?);
        let b = names.next().map(|n| intern(ids, parts, n));
        Some((NoteSpan::Over(a, b), label))
    } else if let Some(rest) = spec.strip_prefix("left of ") {
        Some((NoteSpan::LeftOf(intern(ids, parts, rest.trim())), label))
    } else if let Some(rest) = spec.strip_prefix("right of ") {
        Some((NoteSpan::RightOf(intern(ids, parts, rest.trim())), label))
    } else {
        None
    }
}

/// Parse a message line `A<arrow>B: text`, interning both participants.
fn parse_message(
    line: &str,
    ids: &mut Vec<String>,
    parts: &mut Vec<Participant>,
    intern: &Intern,
) -> Option<(usize, usize, Arrow, bool, String)> {
    // The message text (if any) follows the first ':'.
    let (head, label) = match line.split_once(':') {
        Some((h, l)) => (h.trim(), l.trim().to_string()),
        None => (line.trim(), String::new()),
    };
    let (lhs, op, rhs) = split_message_op(head)?;
    let dashed = op.starts_with("--");
    let arrow = if op.ends_with(">>") {
        Arrow::Head
    } else if op.ends_with('x') {
        Arrow::Cross
    } else if op.ends_with(')') {
        Arrow::Open
    } else {
        Arrow::None
    };
    let from = intern(ids, parts, lhs.trim());
    let to = intern(ids, parts, rhs.trim());
    Some((from, to, arrow, dashed, label))
}

/// Find the message operator in `head`, returning `(lhs, op, rhs)` as byte
/// slices of `head`. Operators are matched longest-first so `-->>` beats `->>`,
/// and the left identifier must be non-empty (so a leading `-` isn't an op).
fn split_message_op(head: &str) -> Option<(&str, &str, &str)> {
    const OPS: &[&str] = &["-->>", "->>", "-->", "--x", "--)", "->", "-x", "-)"];
    for (bstart, _) in head.char_indices() {
        if head[..bstart].trim().is_empty() {
            continue; // need an identifier before the operator
        }
        for op in OPS {
            if head[bstart..].starts_with(op) {
                let bend = bstart + op.len();
                return Some((&head[..bstart], &head[bstart..bend], &head[bend..]));
            }
        }
    }
    None
}

const SEQ_BOX_H: f64 = 36.0;
const SEQ_COL_GAP: f64 = 40.0;
const SEQ_ROW_H: f64 = 46.0;
const SEQ_TOP: f64 = 16.0;

/// Lay a sequence diagram out into pixel space: evenly-spaced participant
/// columns with dashed lifelines, messages as horizontal arrows down the page,
/// and notes as boxes over their participants.
pub fn layout_sequence(seq: &Sequence) -> SeqScene {
    let n = seq.participants.len();
    let col_w: Vec<f64> = seq
        .participants
        .iter()
        .map(|p| node_width(&p.label).max(MIN_W))
        .collect();

    // Column centers, left to right.
    let mut centers = vec![0.0; n];
    let mut x = MARGIN;
    for i in 0..n {
        centers[i] = x + col_w[i] / 2.0;
        x += col_w[i] + SEQ_COL_GAP;
    }
    let content_right = x - SEQ_COL_GAP + MARGIN;

    let top_box_y = SEQ_TOP;
    let lifeline_top = top_box_y + SEQ_BOX_H;
    let body_top = lifeline_top + 18.0;

    let mut boxes: Vec<SeqBox> = Vec::new();
    let mut messages: Vec<SeqMessage> = Vec::new();

    // Walk events top-to-bottom, advancing y per event.
    let mut y = body_top;
    for ev in &seq.events {
        match ev {
            SeqEvent::Message { from, to, label, arrow, dashed } => {
                if from == to {
                    let lx = centers[*from];
                    messages.push(SeqMessage {
                        from: (lx, y),
                        to: (lx, y + SEQ_ROW_H * 0.6),
                        label: label.clone(),
                        dashed: *dashed,
                        arrow: *arrow,
                        self_loop: true,
                    });
                    y += SEQ_ROW_H * 1.1;
                } else {
                    let (a, b) = (centers[*from], centers[*to]);
                    messages.push(SeqMessage {
                        from: (a, y),
                        to: (b, y),
                        label: label.clone(),
                        dashed: *dashed,
                        arrow: *arrow,
                        self_loop: false,
                    });
                    y += SEQ_ROW_H;
                }
            }
            SeqEvent::Note { span, label } => {
                // Box centre and the minimum width its span must cover.
                let (cx, span_w) = match span {
                    NoteSpan::Over(a, Some(b)) => (
                        (centers[*a] + centers[*b]) / 2.0,
                        (centers[*b] - centers[*a]).abs() + col_w[*a].max(col_w[*b]),
                    ),
                    NoteSpan::Over(a, None) => (centers[*a], col_w[*a] + 24.0),
                    NoteSpan::RightOf(a) => (centers[*a] + 80.0, 0.0),
                    NoteSpan::LeftOf(a) => (centers[*a] - 80.0, 0.0),
                };
                let bw = node_width(label).max(span_w);
                boxes.push(SeqBox {
                    label: label.clone(),
                    x: cx - bw / 2.0,
                    y,
                    w: bw,
                    h: SEQ_BOX_H,
                    note: true,
                });
                y += SEQ_ROW_H;
            }
        }
    }

    let body_bottom = y + 6.0;
    let bottom_box_y = body_bottom;
    let height = bottom_box_y + SEQ_BOX_H + MARGIN;

    // Participant header + footer boxes and their lifelines.
    let mut lifelines = Vec::with_capacity(n);
    for i in 0..n {
        let bx = centers[i] - col_w[i] / 2.0;
        boxes.push(SeqBox {
            label: seq.participants[i].label.clone(),
            x: bx,
            y: top_box_y,
            w: col_w[i],
            h: SEQ_BOX_H,
            note: false,
        });
        boxes.push(SeqBox {
            label: seq.participants[i].label.clone(),
            x: bx,
            y: bottom_box_y,
            w: col_w[i],
            h: SEQ_BOX_H,
            note: false,
        });
        lifelines.push((centers[i], lifeline_top, bottom_box_y));
    }

    SeqScene {
        width: content_right.max(MARGIN * 2.0),
        height,
        lifelines,
        boxes,
        messages,
    }
}

// ===== class diagrams ========================================================

/// A relationship endpoint marker (UML arrowheads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    None,
    /// Hollow triangle — inheritance / realization.
    Triangle,
    /// Filled diamond — composition.
    Diamond,
    /// Hollow diamond — aggregation.
    DiamondHollow,
    /// Open arrow — association / dependency.
    Arrow,
}

#[derive(Debug, Clone, PartialEq)]
struct Class {
    name: String,
    fields: Vec<String>,
    methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct Relation {
    from: usize,
    to: usize,
    left: Marker,
    right: Marker,
    dashed: bool,
    label: Option<String>,
}

/// A parsed class diagram (pre-layout): classes + relationships.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDiagram {
    classes: Vec<Class>,
    relations: Vec<Relation>,
}

impl ClassDiagram {
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }
    pub fn relation_count(&self) -> usize {
        self.relations.len()
    }
}

/// A laid-out UML class box: a title bar over field and method compartments.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassBox {
    pub name: String,
    pub fields: Vec<String>,
    pub methods: Vec<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A laid-out class relationship with border-clipped endpoints and end markers.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassEdge {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub left: Marker,
    pub right: Marker,
    pub dashed: bool,
    pub label: Option<String>,
}

/// A fully laid-out class diagram, sized `width` × `height` pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassScene {
    pub width: f64,
    pub height: f64,
    pub boxes: Vec<ClassBox>,
    pub edges: Vec<ClassEdge>,
}

/// Parse a `classDiagram`. Returns `None` if it declares no classes.
fn parse_class(src: &str) -> Option<ClassDiagram> {
    let lines: Vec<&str> = src.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut names: Vec<String> = Vec::new();
    let mut classes: Vec<Class> = Vec::new();
    let mut relations: Vec<Relation> = Vec::new();

    let mut i = 1; // skip the `classDiagram` header
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.starts_with("%%") || line == "}" {
            continue;
        }

        // `class Name { ... }` block (members on following lines) or `class Name`.
        if let Some(rest) = line.strip_prefix("class ") {
            let rest = rest.trim().trim_end_matches('{').trim();
            let name = rest.split_whitespace().next().unwrap_or(rest);
            let idx = intern_class(&mut names, &mut classes, name);
            if line.ends_with('{') {
                while i < lines.len() && lines[i] != "}" {
                    add_member(&mut classes[idx], lines[i]);
                    i += 1;
                }
                i += 1; // consume the closing brace
            }
            continue;
        }

        // Relationship: `A <|-- B`, `A --> B : label`, etc.
        if let Some((from, to, left, right, dashed, label)) = parse_relation(line) {
            let f = intern_class(&mut names, &mut classes, &from);
            let t = intern_class(&mut names, &mut classes, &to);
            relations.push(Relation { from: f, to: t, left, right, dashed, label });
            continue;
        }

        // Member shorthand: `Name : +member`.
        if let Some((name, member)) = line.split_once(':') {
            let name = name.trim();
            if !name.is_empty() && name.split_whitespace().count() == 1 {
                let idx = intern_class(&mut names, &mut classes, name);
                add_member(&mut classes[idx], member.trim());
            }
            continue;
        }
        // Anything else (notes, stereotypes, direction, styling) is ignored.
    }

    if classes.is_empty() {
        return None;
    }
    Some(ClassDiagram { classes, relations })
}

fn intern_class(names: &mut Vec<String>, classes: &mut Vec<Class>, name: &str) -> usize {
    let name = name.trim();
    if let Some(i) = names.iter().position(|n| n == name) {
        i
    } else {
        names.push(name.to_string());
        classes.push(Class { name: name.to_string(), fields: Vec::new(), methods: Vec::new() });
        names.len() - 1
    }
}

/// Add a member line to `class`, splitting into the field or method compartment.
fn add_member(class: &mut Class, line: &str) {
    let m = line.trim();
    if m.is_empty() || m.starts_with("<<") {
        return; // stereotype / annotation — skip
    }
    if m.contains('(') {
        class.methods.push(m.to_string());
    } else {
        class.fields.push(m.to_string());
    }
}

/// Operator → `(left marker, right marker, dashed)`, matched longest-first.
const REL_OPS: &[(&str, Marker, Marker, bool)] = &[
    ("<|--", Marker::Triangle, Marker::None, false),
    ("--|>", Marker::None, Marker::Triangle, false),
    ("<|..", Marker::Triangle, Marker::None, true),
    ("..|>", Marker::None, Marker::Triangle, true),
    ("*--", Marker::Diamond, Marker::None, false),
    ("--*", Marker::None, Marker::Diamond, false),
    ("o--", Marker::DiamondHollow, Marker::None, false),
    ("--o", Marker::None, Marker::DiamondHollow, false),
    ("-->", Marker::None, Marker::Arrow, false),
    ("<--", Marker::Arrow, Marker::None, false),
    ("..>", Marker::None, Marker::Arrow, true),
    ("<..", Marker::Arrow, Marker::None, true),
    ("--", Marker::None, Marker::None, false),
    ("..", Marker::None, Marker::None, true),
];

/// Parse a relationship line, returning `(from, to, left, right, dashed, label)`.
fn parse_relation(line: &str) -> Option<(String, String, Marker, Marker, bool, Option<String>)> {
    // Strip quoted cardinality tokens like "1" / "*".
    let stripped = strip_quoted(line);
    // Find the operator (longest first).
    let (pos, op, left, right, dashed) = REL_OPS.iter().find_map(|&(op, l, r, d)| {
        stripped.find(op).map(|p| (p, op, l, r, d))
    })?;
    let from = stripped[..pos].trim().split_whitespace().last()?.to_string();
    let after = stripped[pos + op.len()..].trim();
    let (to_part, label) = match after.split_once(':') {
        Some((t, lbl)) => (t.trim(), Some(lbl.trim().to_string()).filter(|s| !s.is_empty())),
        None => (after, None),
    };
    let to = to_part.split_whitespace().next()?.to_string();
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some((from, to, left, right, dashed, label))
}

/// Remove `"..."`-quoted substrings (UML cardinality labels) from `s`.
fn strip_quoted(s: &str) -> String {
    let mut out = String::new();
    let mut in_quote = false;
    for c in s.chars() {
        if c == '"' {
            in_quote = !in_quote;
        } else if !in_quote {
            out.push(c);
        }
    }
    out
}

const CLASS_LINE_H: f64 = 19.0;
const CLASS_TITLE_H: f64 = 28.0;
const CLASS_PAD_X: f64 = 14.0;
const CLASS_CHAR_W: f64 = 7.2;

fn class_box_size(c: &Class) -> (f64, f64) {
    let widest = std::iter::once(c.name.chars().count())
        .chain(c.fields.iter().map(|s| s.chars().count()))
        .chain(c.methods.iter().map(|s| s.chars().count()))
        .max()
        .unwrap_or(0);
    let w = (widest as f64 * CLASS_CHAR_W + 2.0 * CLASS_PAD_X).max(96.0);
    let members = (c.fields.len() + c.methods.len()) as f64;
    let body = if members > 0.0 { members * CLASS_LINE_H + 10.0 } else { 0.0 };
    (w, CLASS_TITLE_H + body)
}

/// Lay a class diagram out into pixel space: classes layered top-down by their
/// relationships, edges clipped to box borders.
pub fn layout_class(cd: &ClassDiagram) -> ClassScene {
    let n = cd.classes.len();
    let sizes: Vec<(f64, f64)> = cd.classes.iter().map(class_box_size).collect();

    // Reuse the flowchart layering over relations treated as plain edges.
    let edges: Vec<Edge> = cd
        .relations
        .iter()
        .map(|r| Edge { from: r.from, to: r.to, label: None })
        .collect();
    let layer = longest_path_layers(n, &edges);
    let max_layer = layer.iter().copied().max().unwrap_or(0);

    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (idx, &l) in layer.iter().enumerate() {
        by_layer[l].push(idx);
    }

    // Cross-axis (x) extents per layer to center each row.
    let row_w: Vec<f64> = by_layer
        .iter()
        .map(|ids| {
            ids.iter().map(|&id| sizes[id].0).sum::<f64>()
                + SIBLING_GAP * ids.len().saturating_sub(1) as f64
        })
        .collect();
    let canvas_w = row_w.iter().cloned().fold(0.0, f64::max);

    let mut boxes = vec![
        ClassBox { name: String::new(), fields: vec![], methods: vec![], x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        n
    ];
    let mut y = MARGIN;
    for (l, ids) in by_layer.iter().enumerate() {
        let row_h = ids.iter().map(|&id| sizes[id].1).fold(0.0, f64::max);
        let mut x = MARGIN + (canvas_w - row_w[l]) / 2.0;
        for &id in ids {
            let (w, h) = sizes[id];
            boxes[id] = ClassBox {
                name: cd.classes[id].name.clone(),
                fields: cd.classes[id].fields.clone(),
                methods: cd.classes[id].methods.clone(),
                x,
                y,
                w,
                h,
            };
            x += w + SIBLING_GAP;
        }
        y += row_h + LAYER_GAP;
    }
    let height = y - LAYER_GAP + MARGIN;

    let edges = cd
        .relations
        .iter()
        .map(|r| {
            let a = &boxes[r.from];
            let b = &boxes[r.to];
            let ac = (a.x + a.w / 2.0, a.y + a.h / 2.0);
            let bc = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            ClassEdge {
                from: clip_rect((a.x, a.y, a.w, a.h), bc),
                to: clip_rect((b.x, b.y, b.w, b.h), ac),
                left: r.left,
                right: r.right,
                dashed: r.dashed,
                label: r.label.clone(),
            }
        })
        .collect();

    ClassScene { width: (canvas_w + 2.0 * MARGIN).max(MARGIN * 2.0), height, boxes, edges }
}

/// Intersection of the segment from a rect's center toward `target` with its border.
fn clip_rect((x, y, w, h): (f64, f64, f64, f64), target: (f64, f64)) -> (f64, f64) {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (dx, dy) = (target.0 - cx, target.1 - cy);
    if dx == 0.0 && dy == 0.0 {
        return (cx, cy);
    }
    let tx = if dx != 0.0 { (w / 2.0) / dx.abs() } else { f64::INFINITY };
    let ty = if dy != 0.0 { (h / 2.0) / dy.abs() } else { f64::INFINITY };
    let t = tx.min(ty);
    (cx + dx * t, cy + dy * t)
}

// ===== state diagrams ========================================================

/// What a state node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    /// A regular named state (rounded box).
    Normal,
    /// The `[*]` initial pseudo-state (filled dot).
    Start,
    /// The `[*]` final pseudo-state (ringed dot).
    End,
    /// A `<<choice>>` state (diamond).
    Choice,
}

#[derive(Debug, Clone, PartialEq)]
struct State {
    id: String,
    label: String,
    kind: StateKind,
}

#[derive(Debug, Clone, PartialEq)]
struct Transition {
    from: usize,
    to: usize,
    label: Option<String>,
}

/// A parsed state machine (pre-layout): states + transitions.
#[derive(Debug, Clone, PartialEq)]
pub struct StateMachine {
    pub dir: Dir,
    states: Vec<State>,
    transitions: Vec<Transition>,
}

impl StateMachine {
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }
}

/// A laid-out state node.
#[derive(Debug, Clone, PartialEq)]
pub struct StateNode {
    pub label: String,
    pub kind: StateKind,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A fully laid-out state diagram, sized `width` × `height` pixels. Transitions
/// reuse [`SceneEdge`].
#[derive(Debug, Clone, PartialEq)]
pub struct StateScene {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<StateNode>,
    pub edges: Vec<SceneEdge>,
}

/// Parse a `stateDiagram` / `stateDiagram-v2`. Returns `None` if it has no states.
fn parse_state(src: &str) -> Option<StateMachine> {
    let lines: Vec<&str> = src.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut dir = Dir::Down;
    let mut ids: Vec<String> = Vec::new();
    let mut states: Vec<State> = Vec::new();
    let mut transitions: Vec<Transition> = Vec::new();

    for &line in lines.iter().skip(1) {
        if line.starts_with("%%") || line == "}" {
            continue;
        }
        if let Some(rest) = line.strip_prefix("direction ") {
            if let Some(d) = parse_dir(rest.trim()) {
                dir = d;
            }
            continue;
        }
        if line.starts_with("note ") {
            continue; // notes not laid out yet
        }

        // Transition: `A --> B` (optionally `: label`), endpoints may be `[*]`.
        if let Some(pos) = line.find("-->") {
            let from_tok = line[..pos].trim();
            let after = line[pos + 3..].trim();
            let (to_tok, label) = match after.split_once(':') {
                Some((t, l)) => (t.trim(), Some(l.trim().to_string()).filter(|s| !s.is_empty())),
                None => (after, None),
            };
            let from = intern_state(&mut ids, &mut states, from_tok, true);
            let to = intern_state(&mut ids, &mut states, to_tok, false);
            transitions.push(Transition { from, to, label });
            continue;
        }

        // `state "desc" as id`, `state id <<choice>>`, `state id { ... }`, `state id`.
        if let Some(rest) = line.strip_prefix("state ") {
            parse_state_decl(rest.trim(), &mut ids, &mut states);
            continue;
        }

        // `id : description`.
        if let Some((id, desc)) = line.split_once(':') {
            let id = id.trim();
            if !id.is_empty() && id.split_whitespace().count() == 1 {
                let idx = intern_state(&mut ids, &mut states, id, false);
                states[idx].label = desc.trim().to_string();
            }
            continue;
        }
        // A bare state id on its own line.
        if line.split_whitespace().count() == 1 && line != "{" {
            intern_state(&mut ids, &mut states, line, false);
        }
    }

    if states.is_empty() {
        return None;
    }
    Some(StateMachine { dir, states, transitions })
}

/// Parse the text after `state `: `"desc" as id`, `id <<choice>>`, `id {`, `id`.
fn parse_state_decl(rest: &str, ids: &mut Vec<String>, states: &mut Vec<State>) {
    // `state "Long description" as id`
    if let Some(after_quote) = rest.strip_prefix('"') {
        if let Some((desc, tail)) = after_quote.split_once('"') {
            if let Some(id) = tail.trim().strip_prefix("as ") {
                let idx = intern_state(ids, states, id.trim(), false);
                states[idx].label = desc.to_string();
                return;
            }
        }
    }
    let id = rest.trim_end_matches('{').trim();
    let (id, kind) = if let Some(p) = id.find("<<") {
        let stereo = &id[p..];
        let kind = if stereo.contains("choice") { StateKind::Choice } else { StateKind::Normal };
        (id[..p].trim(), kind)
    } else {
        (id, StateKind::Normal)
    };
    if let Some(name) = id.split_whitespace().next() {
        let idx = intern_state(ids, states, name, false);
        if kind == StateKind::Choice {
            states[idx].kind = StateKind::Choice;
        }
    }
}

/// Intern a state by token. `[*]` maps to a single Start node when it is the
/// source of a transition, or a single End node when it is the target.
fn intern_state(ids: &mut Vec<String>, states: &mut Vec<State>, tok: &str, is_source: bool) -> usize {
    let (key, label, kind) = if tok == "[*]" {
        if is_source {
            ("\u{1}start", String::new(), StateKind::Start)
        } else {
            ("\u{1}end", String::new(), StateKind::End)
        }
    } else {
        (tok, tok.to_string(), StateKind::Normal)
    };
    if let Some(i) = ids.iter().position(|n| n == key) {
        i
    } else {
        ids.push(key.to_string());
        states.push(State { id: key.to_string(), label, kind });
        ids.len() - 1
    }
}

const STATE_DOT: f64 = 18.0;
const STATE_CHOICE: f64 = 34.0;

fn state_node_size(s: &State) -> (f64, f64) {
    match s.kind {
        StateKind::Start | StateKind::End => (STATE_DOT, STATE_DOT),
        StateKind::Choice => (STATE_CHOICE, STATE_CHOICE),
        StateKind::Normal => (node_width(&s.label).max(70.0), NODE_H),
    }
}

/// Lay a state machine out into pixel space using longest-path layering along
/// its direction (default top-down), edges clipped to node borders.
pub fn layout_state(sm: &StateMachine) -> StateScene {
    let n = sm.states.len();
    let sizes: Vec<(f64, f64)> = sm.states.iter().map(state_node_size).collect();
    let edges_g: Vec<Edge> = sm
        .transitions
        .iter()
        .map(|t| Edge { from: t.from, to: t.to, label: None })
        .collect();
    let layer = longest_path_layers(n, &edges_g);
    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let horizontal = sm.dir.horizontal();

    let mut by_layer: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (idx, &l) in layer.iter().enumerate() {
        by_layer[l].push(idx);
    }

    let cross = |id: usize| if horizontal { sizes[id].1 } else { sizes[id].0 };
    let main = |id: usize| if horizontal { sizes[id].0 } else { sizes[id].1 };

    let layer_cross: Vec<f64> = by_layer
        .iter()
        .map(|ids| {
            ids.iter().map(|&id| cross(id)).sum::<f64>()
                + SIBLING_GAP * ids.len().saturating_sub(1) as f64
        })
        .collect();
    let cross_canvas = layer_cross.iter().cloned().fold(0.0, f64::max);

    let mut nodes = vec![
        StateNode { label: String::new(), kind: StateKind::Normal, x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        n
    ];
    let mut acc = MARGIN;
    for (l, ids) in by_layer.iter().enumerate() {
        let layer_main = ids.iter().map(|&id| main(id)).fold(0.0, f64::max);
        let mut c = MARGIN + (cross_canvas - layer_cross[l]) / 2.0;
        for &id in ids {
            let (w, h) = sizes[id];
            // Center each node within its layer's main extent.
            let main_off = acc + (layer_main - main(id)) / 2.0;
            let (x, y) = if horizontal { (main_off, c) } else { (c, main_off) };
            nodes[id] = StateNode {
                label: sm.states[id].label.clone(),
                kind: sm.states[id].kind,
                x,
                y,
                w,
                h,
            };
            c += cross(id) + SIBLING_GAP;
        }
        acc += layer_main + LAYER_GAP;
    }
    let main_canvas = acc - LAYER_GAP + MARGIN;
    let (width, height) = if horizontal {
        (main_canvas, cross_canvas + 2.0 * MARGIN)
    } else {
        (cross_canvas + 2.0 * MARGIN, main_canvas)
    };

    let edges = sm
        .transitions
        .iter()
        .map(|t| {
            let a = &nodes[t.from];
            let b = &nodes[t.to];
            let ac = (a.x + a.w / 2.0, a.y + a.h / 2.0);
            let bc = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            let from = clip_rect((a.x, a.y, a.w, a.h), bc);
            let to = clip_rect((b.x, b.y, b.w, b.h), ac);
            SceneEdge {
                from,
                to,
                label: t.label.clone(),
                label_pos: ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0),
            }
        })
        .collect();

    StateScene { width: width.max(MARGIN * 2.0), height, nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse `src` expecting a flowchart, panicking otherwise.
    fn flow(src: &str) -> Graph {
        match parse(src) {
            Some(Diagram::Flow(g)) => g,
            other => panic!("expected a flowchart, got {other:?}"),
        }
    }

    /// Parse `src` expecting a sequence diagram, panicking otherwise.
    fn seq(src: &str) -> Sequence {
        match parse(src) {
            Some(Diagram::Sequence(s)) => s,
            other => panic!("expected a sequence diagram, got {other:?}"),
        }
    }

    /// Parse `src` expecting a class diagram, panicking otherwise.
    fn cls(src: &str) -> ClassDiagram {
        match parse(src) {
            Some(Diagram::Class(c)) => c,
            other => panic!("expected a class diagram, got {other:?}"),
        }
    }

    /// Parse `src` expecting a state diagram, panicking otherwise.
    fn state(src: &str) -> StateMachine {
        match parse(src) {
            Some(Diagram::State(s)) => s,
            other => panic!("expected a state diagram, got {other:?}"),
        }
    }

    #[test]
    fn parses_simple_chain() {
        let g = flow("graph TD\nA --> B\nB --> C");
        assert_eq!(g.dir, Dir::Down);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn chained_edges_on_one_line() {
        let g = flow("flowchart LR\nA --> B --> C");
        assert_eq!(g.dir, Dir::Right);
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn shapes_and_labels() {
        let g = flow("graph TD\nA[Start] --> B(stop)\nC{maybe}");
        assert_eq!(g.nodes[0].label, "Start");
        assert_eq!(g.nodes[0].shape, Shape::Rect);
        assert_eq!(g.nodes[1].label, "stop");
        assert_eq!(g.nodes[1].shape, Shape::Round);
        assert_eq!(g.nodes[2].shape, Shape::Diamond);
    }

    #[test]
    fn edge_labels_pipe_and_mid() {
        let g = flow("graph TD\nA -->|yes| B\nA -- no --> C");
        assert_eq!(g.edges[0].label.as_deref(), Some("yes"));
        assert_eq!(g.edges[1].label.as_deref(), Some("no"));
    }

    #[test]
    fn dotted_and_thick_links() {
        let g = flow("graph LR\nA -.-> B\nB ==> C");
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn undirected_link_does_not_eat_target() {
        let g = flow("graph TD\nA --- B");
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.nodes[1].id, "B");
    }

    #[test]
    fn unsupported_diagram_returns_none() {
        assert!(parse("").is_none());
        assert!(parse("not a diagram").is_none());
        assert!(parse("gantt\ntitle X").is_none());
    }

    #[test]
    fn layout_stacks_downward_for_td() {
        let s = layout(&flow("graph TD\nA --> B"));
        assert_eq!(s.nodes.len(), 2);
        // B sits below A and the canvas is taller than it is wide-ish.
        assert!(s.nodes[1].y > s.nodes[0].y, "B should be below A");
        assert!(s.height > 2.0 * NODE_H);
    }

    #[test]
    fn layout_flows_rightward_for_lr() {
        let s = layout(&flow("flowchart LR\nA --> B"));
        assert!(s.nodes[1].x > s.nodes[0].x, "B should be right of A");
    }

    #[test]
    fn layout_edge_endpoints_on_borders() {
        let s = layout(&flow("graph TD\nA --> B"));
        let e = &s.edges[0];
        let a = &s.nodes[0];
        // The edge leaves A's bottom border.
        assert!((e.from.1 - (a.y + a.h)).abs() < 1.0);
    }

    // ----- sequence diagrams -----

    #[test]
    fn sequence_implicit_participants_in_order() {
        let s = seq("sequenceDiagram\nAlice->>Bob: hi\nBob-->>Alice: yo");
        assert_eq!(s.participant_count(), 2);
        assert_eq!(s.participants[0].label, "Alice");
        assert_eq!(s.participants[1].label, "Bob");
        assert_eq!(s.event_count(), 2);
    }

    #[test]
    fn sequence_participant_alias_and_order() {
        let s = seq("sequenceDiagram\nparticipant B as Bob\nparticipant A as Alice\nA->>B: hi");
        // Declaration order fixes the columns regardless of message order.
        assert_eq!(s.participants[0].label, "Bob");
        assert_eq!(s.participants[1].label, "Alice");
    }

    #[test]
    fn sequence_arrow_kinds() {
        let s = seq("sequenceDiagram\nA->>B: a\nA-->>B: b\nA-xB: c\nA-)B: d\nA->B: e");
        let arrows: Vec<(Arrow, bool)> = s
            .events
            .iter()
            .filter_map(|e| match e {
                SeqEvent::Message { arrow, dashed, .. } => Some((*arrow, *dashed)),
                _ => None,
            })
            .collect();
        assert_eq!(arrows[0], (Arrow::Head, false));
        assert_eq!(arrows[1], (Arrow::Head, true));
        assert_eq!(arrows[2], (Arrow::Cross, false));
        assert_eq!(arrows[3], (Arrow::Open, false));
        assert_eq!(arrows[4], (Arrow::None, false));
    }

    #[test]
    fn sequence_notes_and_self_message() {
        let s = seq("sequenceDiagram\nA->>A: think\nNote over A,B: hmm\nNote right of A: ok");
        let notes = s.events.iter().filter(|e| matches!(e, SeqEvent::Note { .. })).count();
        assert_eq!(notes, 2);
        // The self-message and the note's `B` both create participants.
        assert_eq!(s.participant_count(), 2);
    }

    #[test]
    fn sequence_layout_lifelines_and_self_loop() {
        let s = layout_sequence(&seq("sequenceDiagram\nAlice->>Bob: hi\nBob->>Bob: ponder"));
        assert_eq!(s.lifelines.len(), 2);
        // Two participants → header + footer boxes = 4.
        assert_eq!(s.boxes.iter().filter(|b| !b.note).count(), 4);
        // Bob is right of Alice.
        assert!(s.lifelines[1].0 > s.lifelines[0].0);
        // The second message is a self-loop.
        assert!(s.messages[1].self_loop);
        // A downward flow: message 2 is below message 1.
        assert!(s.messages[1].from.1 > s.messages[0].from.1);
    }

    // ----- class diagrams -----

    #[test]
    fn class_block_members_split_fields_and_methods() {
        let c = cls("classDiagram\nclass Animal {\n+String name\n+int age\n+makeSound() void\n}");
        assert_eq!(c.class_count(), 1);
        assert_eq!(c.classes[0].name, "Animal");
        assert_eq!(c.classes[0].fields, vec!["+String name", "+int age"]);
        assert_eq!(c.classes[0].methods, vec!["+makeSound() void"]);
    }

    #[test]
    fn class_member_shorthand() {
        let c = cls("classDiagram\nDog : +fetch()\nDog : +String breed");
        assert_eq!(c.class_count(), 1);
        assert_eq!(c.classes[0].methods, vec!["+fetch()"]);
        assert_eq!(c.classes[0].fields, vec!["+String breed"]);
    }

    #[test]
    fn class_relations_and_markers() {
        let c = cls(
            "classDiagram\nAnimal <|-- Dog\nAnimal *-- Leg\nAnimal o-- Tail\nAnimal --> Owner : owns\nAnimal ..> Helper",
        );
        assert_eq!(c.class_count(), 6);
        assert_eq!(c.relation_count(), 5);
        assert_eq!(c.relations[0].left, Marker::Triangle); // <|--
        assert_eq!(c.relations[1].left, Marker::Diamond); // *--
        assert_eq!(c.relations[2].left, Marker::DiamondHollow); // o--
        assert_eq!(c.relations[3].right, Marker::Arrow); // -->
        assert_eq!(c.relations[3].label.as_deref(), Some("owns"));
        assert!(c.relations[4].dashed); // ..>
        assert_eq!(c.relations[4].right, Marker::Arrow);
    }

    #[test]
    fn class_relation_strips_cardinality() {
        let c = cls("classDiagram\nAnimal \"1\" --> \"*\" Leg : has");
        assert_eq!(c.relation_count(), 1);
        assert_eq!(c.classes[0].name, "Animal");
        assert_eq!(c.classes[1].name, "Leg");
        assert_eq!(c.relations[0].label.as_deref(), Some("has"));
    }

    #[test]
    fn class_layout_stacks_parent_above_child() {
        let s = layout_class(&cls("classDiagram\nAnimal <|-- Dog"));
        assert_eq!(s.boxes.len(), 2);
        // Animal (parent, layer 0) sits above Dog (child, layer 1).
        assert!(s.boxes[1].y > s.boxes[0].y, "Dog should be below Animal");
        assert_eq!(s.edges.len(), 1);
        assert_eq!(s.edges[0].left, Marker::Triangle);
    }

    // ----- state diagrams -----

    #[test]
    fn state_start_end_are_shared_pseudo_states() {
        let s = state("stateDiagram-v2\n[*] --> Idle\nIdle --> Running\nRunning --> [*]");
        // Start, Idle, Running, End.
        assert_eq!(s.state_count(), 4);
        assert_eq!(s.transition_count(), 3);
        let kinds: Vec<StateKind> = s.states.iter().map(|st| st.kind).collect();
        assert!(kinds.contains(&StateKind::Start));
        assert!(kinds.contains(&StateKind::End));
        assert_eq!(kinds.iter().filter(|k| **k == StateKind::Normal).count(), 2);
    }

    #[test]
    fn state_transition_labels_and_decls() {
        let s = state(
            "stateDiagram-v2\nstate \"Powered On\" as On\nOff --> On : flip\nstate Choosing <<choice>>",
        );
        let on = s.states.iter().find(|st| st.id == "On").unwrap();
        assert_eq!(on.label, "Powered On");
        let choosing = s.states.iter().find(|st| st.id == "Choosing").unwrap();
        assert_eq!(choosing.kind, StateKind::Choice);
        let t = s.transitions.iter().find(|t| t.label.is_some()).unwrap();
        assert_eq!(t.label.as_deref(), Some("flip"));
    }

    #[test]
    fn state_layout_flows_downward() {
        let s = layout_state(&state("stateDiagram-v2\n[*] --> A\nA --> [*]"));
        assert_eq!(s.nodes.len(), 3);
        // Start dot at top, end dot at bottom, A in the middle.
        let start = s.nodes.iter().find(|n| n.kind == StateKind::Start).unwrap();
        let end = s.nodes.iter().find(|n| n.kind == StateKind::End).unwrap();
        assert!(end.y > start.y, "end should be below start");
        // Pseudo-states are small dots.
        assert!(start.w < 24.0 && start.h < 24.0);
    }

    #[test]
    fn state_cycle_does_not_inflate_layout() {
        // Running <-> Paused is a cycle; without back-edge removal the layering
        // would push nodes many layers apart. The canvas should stay compact.
        let s = layout_state(&state(
            "stateDiagram-v2\n[*] --> Idle\nIdle --> Running\nRunning --> Paused\nPaused --> Running\nRunning --> [*]",
        ));
        // 5 nodes over ~4 layers; height stays bounded (no runaway).
        assert!(s.height < 6.0 * (NODE_H + LAYER_GAP), "height={}", s.height);
    }

    #[test]
    fn unsupported_still_none_after_new_types() {
        assert!(parse("erDiagram\nA ||--o{ B : has").is_none());
        assert!(parse("gantt\ntitle X").is_none());
    }
}
