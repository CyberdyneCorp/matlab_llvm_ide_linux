//! Debug Adapter Protocol transport framing for `matlabc -dap`. DAP speaks
//! JSON-RPC bodies wrapped in `Content-Length: N\r\n\r\n<body>` frames over
//! stdio. This module is the pure framing codec (encoder + streaming decoder)
//! plus a small request/sequence helper. The process plumbing lives in the app;
//! these pieces carry the coverage. Mirrors the framing in `DAPSession.swift`.

use serde_json::Value;

/// Wrap a JSON body string in a DAP `Content-Length` frame.
pub fn encode_frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

/// Streaming decoder: feed raw bytes as they arrive; pop complete JSON bodies.
#[derive(Default)]
pub struct DapFramer {
    buffer: Vec<u8>,
}

impl DapFramer {
    pub fn new() -> DapFramer {
        DapFramer { buffer: Vec::new() }
    }

    /// Append received bytes and return every complete body now available.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            // Find the header/body separator.
            let Some(sep) = find_subslice(&self.buffer, b"\r\n\r\n") else { break };
            let header = match std::str::from_utf8(&self.buffer[..sep]) {
                Ok(h) => h,
                Err(_) => break,
            };
            let Some(len) = content_length(header) else {
                // Malformed header — drop it to avoid wedging the stream.
                self.buffer.drain(..sep + 4);
                continue;
            };
            let body_start = sep + 4;
            if self.buffer.len() < body_start + len {
                break; // body not fully arrived yet
            }
            let body = self.buffer[body_start..body_start + len].to_vec();
            self.buffer.drain(..body_start + len);
            if let Ok(s) = String::from_utf8(body) {
                out.push(s);
            }
        }
        out
    }
}

fn content_length(header: &str) -> Option<usize> {
    for line in header.split("\r\n") {
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Monotonic request-sequence counter + request/body builders for the client
/// side of the protocol.
#[derive(Default)]
pub struct DapClient {
    seq: i64,
}

impl DapClient {
    pub fn new() -> DapClient {
        DapClient { seq: 0 }
    }

    /// Build a `request` body for `command` with optional `arguments`, framed
    /// and ready to write to the adapter's stdin.
    pub fn request(&mut self, command: &str, arguments: Option<Value>) -> String {
        self.seq += 1;
        let mut body = serde_json::json!({
            "seq": self.seq,
            "type": "request",
            "command": command,
        });
        if let Some(args) = arguments {
            body["arguments"] = args;
        }
        encode_frame(&body.to_string())
    }

    pub fn last_seq(&self) -> i64 {
        self.seq
    }
}

/// Classify a decoded DAP message body by its `type` field.
#[derive(Clone, Debug, PartialEq)]
pub enum DapMessage {
    Response { request_seq: i64, command: String, success: bool, body: Value },
    Event { event: String, body: Value },
    Other(Value),
}

/// Parse a decoded body string into a typed [`DapMessage`].
pub fn parse_message(body: &str) -> Option<DapMessage> {
    let v: Value = serde_json::from_str(body).ok()?;
    match v.get("type").and_then(|t| t.as_str()) {
        Some("response") => Some(DapMessage::Response {
            request_seq: v.get("request_seq").and_then(|x| x.as_i64()).unwrap_or(0),
            command: v.get("command").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            success: v.get("success").and_then(|x| x.as_bool()).unwrap_or(false),
            body: v.get("body").cloned().unwrap_or(Value::Null),
        }),
        Some("event") => Some(DapMessage::Event {
            event: v.get("event").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            body: v.get("body").cloned().unwrap_or(Value::Null),
        }),
        _ => Some(DapMessage::Other(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_content_length_frame() {
        let frame = encode_frame("{\"a\":1}");
        assert_eq!(frame, "Content-Length: 7\r\n\r\n{\"a\":1}");
    }

    #[test]
    fn decodes_single_frame() {
        let mut f = DapFramer::new();
        let bodies = f.feed(b"Content-Length: 7\r\n\r\n{\"a\":1}");
        assert_eq!(bodies, vec!["{\"a\":1}".to_string()]);
    }

    #[test]
    fn decodes_two_frames_in_one_chunk() {
        let mut f = DapFramer::new();
        let data = format!("{}{}", encode_frame("{\"x\":1}"), encode_frame("{\"y\":2}"));
        let bodies = f.feed(data.as_bytes());
        assert_eq!(bodies, vec!["{\"x\":1}".to_string(), "{\"y\":2}".to_string()]);
    }

    #[test]
    fn reassembles_frame_split_across_chunks() {
        let mut f = DapFramer::new();
        let frame = encode_frame("{\"hello\":true}");
        let mid = frame.len() / 2;
        assert!(f.feed(frame[..mid].as_bytes()).is_empty());
        let bodies = f.feed(frame[mid..].as_bytes());
        assert_eq!(bodies, vec!["{\"hello\":true}".to_string()]);
    }

    #[test]
    fn request_increments_seq_and_frames() {
        let mut c = DapClient::new();
        let frame = c.request("initialize", Some(serde_json::json!({"clientID": "matforge"})));
        assert_eq!(c.last_seq(), 1);
        assert!(frame.starts_with("Content-Length: "));
        assert!(frame.contains("\"command\":\"initialize\""));
        assert!(frame.contains("\"clientID\":\"matforge\""));
        c.request("launch", None);
        assert_eq!(c.last_seq(), 2);
    }

    #[test]
    fn parses_response_and_event() {
        let resp = r#"{"type":"response","request_seq":3,"command":"stackTrace","success":true,"body":{"x":1}}"#;
        match parse_message(resp).unwrap() {
            DapMessage::Response { request_seq, command, success, .. } => {
                assert_eq!(request_seq, 3);
                assert_eq!(command, "stackTrace");
                assert!(success);
            }
            _ => panic!("expected response"),
        }
        let ev = r#"{"type":"event","event":"stopped","body":{"reason":"breakpoint"}}"#;
        match parse_message(ev).unwrap() {
            DapMessage::Event { event, body } => {
                assert_eq!(event, "stopped");
                assert_eq!(body["reason"], "breakpoint");
            }
            _ => panic!("expected event"),
        }
    }

    #[test]
    fn skips_malformed_header() {
        let mut f = DapFramer::new();
        // No Content-Length — header dropped, following good frame still decodes.
        let data = format!("Bad-Header: x\r\n\r\n{}", encode_frame("{\"ok\":1}"));
        let bodies = f.feed(data.as_bytes());
        assert_eq!(bodies, vec!["{\"ok\":1}".to_string()]);
    }
}
