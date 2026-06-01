//! REPL / Run stdout sentinel protocol. The `matlab_plot` runtime and the
//! IDE-injected `whos` / value probes wrap structured payloads in sentinel
//! lines; this module is the line state machine that separates visible console
//! text from those payloads. Ports `FigureSentinelParser.swift` and the
//! `___MF_*` handling in `ReplViewModel.swift`.

pub const WS_BEGIN: &str = "___MF_WS_BEGIN___";
pub const WS_END: &str = "___MF_WS_END___";
pub const VAL_BEGIN: &str = "___MF_VAL_BEGIN___";
pub const VAL_END: &str = "___MF_VAL_END___";
pub const FIG_BEGIN: &str = "___MF_FIG_BEGIN___";
pub const FIG_END: &str = "___MF_FIG_END___";

/// One structured payload (or a passthrough console line) produced by routing a
/// stdout line through [`SentinelRouter`].
#[derive(Clone, Debug, PartialEq)]
pub enum ReplEvent {
    /// A normal console line (forward to the transcript).
    Console(String),
    /// A complete `whos` block (inner text between the WS sentinels).
    Workspace(String),
    /// A complete value/`disp` block (inner text between the VAL sentinels).
    Value(String),
    /// A complete figure PNG reassembled from the FIG block.
    Figure { runtime_id: i64, width: i64, height: i64, png: Vec<u8> },
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Workspace,
    Value,
    Figure,
}

/// Stateful line router. Feed it one stdout line at a time via [`consume`].
/// Thread-unsafe by design (the caller marshals lines onto the UI thread).
pub struct SentinelRouter {
    mode: Mode,
    buffer: String,
    fig_runtime_id: i64,
    fig_width: i64,
    fig_height: i64,
    fig_b64: String,
}

impl Default for SentinelRouter {
    fn default() -> Self {
        SentinelRouter::new()
    }
}

impl SentinelRouter {
    pub fn new() -> SentinelRouter {
        SentinelRouter {
            mode: Mode::Normal,
            buffer: String::new(),
            fig_runtime_id: 0,
            fig_width: 0,
            fig_height: 0,
            fig_b64: String::new(),
        }
    }

    /// Drop any partial state (e.g. the upstream process exited mid-block).
    pub fn reset(&mut self) {
        self.mode = Mode::Normal;
        self.buffer.clear();
        self.fig_b64.clear();
        self.fig_runtime_id = 0;
        self.fig_width = 0;
        self.fig_height = 0;
    }

    /// Route one stdout line, returning zero or one events.
    pub fn consume(&mut self, line: &str) -> Option<ReplEvent> {
        match self.mode {
            Mode::Workspace => {
                if line.contains(WS_END) {
                    let text = std::mem::take(&mut self.buffer);
                    self.mode = Mode::Normal;
                    return Some(ReplEvent::Workspace(text));
                }
                self.append_buffer(line);
                None
            }
            Mode::Value => {
                if line.contains(VAL_END) {
                    let text = std::mem::take(&mut self.buffer);
                    self.mode = Mode::Normal;
                    return Some(ReplEvent::Value(text));
                }
                self.append_buffer(line);
                None
            }
            Mode::Figure => {
                if line.contains(FIG_END) {
                    let result = self.decode_figure();
                    self.reset();
                    return result;
                }
                self.fig_b64.push_str(line.trim());
                None
            }
            Mode::Normal => self.consume_normal(line),
        }
    }

    fn append_buffer(&mut self, line: &str) {
        if !self.buffer.is_empty() {
            self.buffer.push('\n');
        }
        self.buffer.push_str(line);
    }

    fn consume_normal(&mut self, line: &str) -> Option<ReplEvent> {
        if line.contains(WS_BEGIN) {
            self.mode = Mode::Workspace;
            self.buffer.clear();
            return None;
        }
        if line.contains(VAL_BEGIN) {
            self.mode = Mode::Value;
            self.buffer.clear();
            return None;
        }
        if let Some(pos) = line.find(FIG_BEGIN) {
            let tail = &line[pos + FIG_BEGIN.len()..];
            let (id, w, h) = parse_fig_header(tail);
            self.fig_runtime_id = id;
            self.fig_width = w;
            self.fig_height = h;
            self.fig_b64.clear();
            self.mode = Mode::Figure;
            return None;
        }
        Some(ReplEvent::Console(line.to_string()))
    }

    fn decode_figure(&self) -> Option<ReplEvent> {
        let png = base64_decode(&self.fig_b64);
        if png.is_empty() {
            return None;
        }
        Some(ReplEvent::Figure {
            runtime_id: self.fig_runtime_id,
            width: self.fig_width,
            height: self.fig_height,
            png,
        })
    }
}

/// Parse `id=N w=W h=H` from the FIG_BEGIN tail. Missing fields default to 0.
fn parse_fig_header(tail: &str) -> (i64, i64, i64) {
    let (mut id, mut w, mut h) = (0, 0, 0);
    for token in tail.split_whitespace() {
        if let Some((key, value)) = token.split_once('=') {
            if let Ok(v) = value.parse::<i64>() {
                match key {
                    "id" => id = v,
                    "w" => w = v,
                    "h" => h = v,
                    _ => {}
                }
            }
        }
    }
    (id, w, h)
}

/// Minimal standard-alphabet base64 decoder. Ignores any non-alphabet bytes
/// (whitespace, newlines), tolerating the runtime's 76-char chunking. Returns
/// an empty vec on a fundamentally malformed payload.
pub fn base64_decode(input: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut quad = [0u8; 4];
    let mut count = 0usize;
    for &b in input.as_bytes() {
        if b == b'=' {
            break;
        }
        let Some(v) = val(b) else { continue };
        quad[count] = v;
        count += 1;
        if count == 4 {
            out.push((quad[0] << 2) | (quad[1] >> 4));
            out.push((quad[1] << 4) | (quad[2] >> 2));
            out.push((quad[2] << 6) | quad[3]);
            count = 0;
        }
    }
    match count {
        2 => out.push((quad[0] << 2) | (quad[1] >> 4)),
        3 => {
            out.push((quad[0] << 2) | (quad[1] >> 4));
            out.push((quad[1] << 4) | (quad[2] >> 2));
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base64_encode(data: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            out.push(A[(b[0] >> 2) as usize] as char);
            out.push(A[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(A[(((b[1] & 0x0f) << 2) | (b[2] >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(A[(b[2] & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    #[test]
    fn passthrough_normal_lines() {
        let mut r = SentinelRouter::new();
        assert_eq!(r.consume("hello"), Some(ReplEvent::Console("hello".into())));
    }

    #[test]
    fn extracts_workspace_block() {
        let mut r = SentinelRouter::new();
        assert_eq!(r.consume(WS_BEGIN), None);
        assert_eq!(r.consume("a  1x1  double"), None);
        assert_eq!(r.consume("b  2x2  double"), None);
        let ev = r.consume(WS_END).unwrap();
        assert_eq!(ev, ReplEvent::Workspace("a  1x1  double\nb  2x2  double".into()));
        // back to normal afterwards
        assert_eq!(r.consume("ok"), Some(ReplEvent::Console("ok".into())));
    }

    #[test]
    fn extracts_value_block() {
        let mut r = SentinelRouter::new();
        r.consume(VAL_BEGIN);
        r.consume("1 2 3");
        let ev = r.consume(VAL_END).unwrap();
        assert_eq!(ev, ReplEvent::Value("1 2 3".into()));
    }

    #[test]
    fn reassembles_figure_png() {
        let png = vec![0x89u8, b'P', b'N', b'G', 1, 2, 3, 4, 5];
        let b64 = base64_encode(&png);
        let mut r = SentinelRouter::new();
        assert_eq!(r.consume(&format!("{FIG_BEGIN} id=7 w=320 h=240")), None);
        // chunk the base64 across two lines
        let mid = b64.len() / 2;
        r.consume(&b64[..mid]);
        r.consume(&b64[mid..]);
        let ev = r.consume(FIG_END).unwrap();
        assert_eq!(
            ev,
            ReplEvent::Figure { runtime_id: 7, width: 320, height: 240, png }
        );
    }

    #[test]
    fn figure_header_parsing_is_tolerant() {
        assert_eq!(parse_fig_header(" id=3 w=10 h=20"), (3, 10, 20));
        assert_eq!(parse_fig_header(" garbage"), (0, 0, 0));
        assert_eq!(parse_fig_header(" id=5"), (5, 0, 0));
    }

    #[test]
    fn base64_roundtrip_various_lengths() {
        for n in 0..20 {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 1) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&data)), data, "n={n}");
        }
    }

    #[test]
    fn base64_ignores_whitespace() {
        let data = b"hello world";
        let enc = base64_encode(data);
        let spaced = format!("{}\n  {}", &enc[..4], &enc[4..]);
        assert_eq!(base64_decode(&spaced), data);
    }

    #[test]
    fn reset_clears_partial_block() {
        let mut r = SentinelRouter::new();
        r.consume(WS_BEGIN);
        r.consume("partial");
        r.reset();
        assert_eq!(r.consume("after"), Some(ReplEvent::Console("after".into())));
    }

    #[test]
    fn garbled_figure_drops_silently() {
        let mut r = SentinelRouter::new();
        r.consume(&format!("{FIG_BEGIN} id=1 w=1 h=1"));
        r.consume("!!!"); // no valid base64 chars
        assert_eq!(r.consume(FIG_END), None);
    }
}
