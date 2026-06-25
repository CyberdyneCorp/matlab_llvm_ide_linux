//! Encode / decode `.mflow` JSON documents. Pure serde — validates the schema
//! string and major/minor version on decode so foreign files don't silently
//! load with the wrong field interpretation. Ported from `FlowchartCodec.swift`.

use std::error::Error;
use std::fmt;

use crate::models::flowchart::FlowchartDocument;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowchartCodecError {
    InvalidJson(String),
    UnsupportedSchema(String),
    UnsupportedVersion(String),
}

impl fmt::Display for FlowchartCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlowchartCodecError::InvalidJson(m) => write!(f, "Invalid flowchart JSON: {m}"),
            FlowchartCodecError::UnsupportedSchema(s) => {
                write!(f, "Unsupported flowchart schema: {s}")
            }
            FlowchartCodecError::UnsupportedVersion(v) => {
                write!(f, "Unsupported flowchart version: {v}")
            }
        }
    }
}

impl Error for FlowchartCodecError {}

/// Pretty-printed encode so saved files diff cleanly in git. Struct field order
/// and `BTreeMap` map fields make the output deterministic across runs.
pub fn encode_string(document: &FlowchartDocument) -> Result<String, FlowchartCodecError> {
    serde_json::to_string_pretty(document)
        .map_err(|e| FlowchartCodecError::InvalidJson(e.to_string()))
}

/// Decode + validate from a UTF-8 string.
pub fn decode_str(string: &str) -> Result<FlowchartDocument, FlowchartCodecError> {
    let doc: FlowchartDocument = serde_json::from_str(string)
        .map_err(|e| FlowchartCodecError::InvalidJson(e.to_string()))?;
    validate(&doc)?;
    Ok(doc)
}

pub fn validate(doc: &FlowchartDocument) -> Result<(), FlowchartCodecError> {
    if doc.schema != FlowchartDocument::CURRENT_SCHEMA {
        return Err(FlowchartCodecError::UnsupportedSchema(doc.schema.clone()));
    }
    if !is_compatible_version(&doc.version) {
        return Err(FlowchartCodecError::UnsupportedVersion(doc.version.clone()));
    }
    Ok(())
}

/// Pre-1.0 the IDE accepts 0.1.x and 0.2.x (the 0.2.0 mStateflow bump was
/// feature-additive). Once 1.0.0 ships, major-only matching applies.
pub fn is_compatible_version(version: &str) -> bool {
    let (doc_major, doc_minor) = major_minor(version);
    let (current_major, _) = major_minor(FlowchartDocument::CURRENT_VERSION);
    if current_major == "0" {
        return doc_major == current_major && (doc_minor == "1" || doc_minor == "2");
    }
    doc_major == current_major
}

fn major_minor(version: &str) -> (&str, &str) {
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or(version);
    let minor = parts.next().unwrap_or("");
    (major, minor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::SchemaKind;

    #[test]
    fn round_trips_an_empty_control_flow_doc() {
        let doc = FlowchartDocument::empty("Demo", SchemaKind::ControlFlow);
        let json = encode_string(&doc).unwrap();
        let back = decode_str(&json).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn round_trips_a_signal_flow_doc() {
        let doc = FlowchartDocument::empty("Sig", SchemaKind::SignalFlow);
        let json = encode_string(&doc).unwrap();
        let back = decode_str(&json).unwrap();
        assert_eq!(doc, back);
        assert_eq!(back.schema_kind(), SchemaKind::SignalFlow);
    }

    #[test]
    fn encode_is_deterministic() {
        let doc = FlowchartDocument::empty("Demo", SchemaKind::ControlFlow);
        assert_eq!(encode_string(&doc).unwrap(), encode_string(&doc).unwrap());
    }

    #[test]
    fn rejects_foreign_schema() {
        let json = r#"{"schema":"other.schema","version":"0.2.0","flows":[]}"#;
        assert_eq!(
            decode_str(json),
            Err(FlowchartCodecError::UnsupportedSchema(
                "other.schema".into()
            ))
        );
    }

    #[test]
    fn rejects_incompatible_version() {
        let json = r#"{"schema":"matforge.flowchart","version":"1.0.0","flows":[]}"#;
        assert_eq!(
            decode_str(json),
            Err(FlowchartCodecError::UnsupportedVersion("1.0.0".into()))
        );
    }

    #[test]
    fn accepts_legacy_minor_versions() {
        assert!(is_compatible_version("0.1.0"));
        assert!(is_compatible_version("0.2.5"));
        assert!(!is_compatible_version("0.3.0"));
        assert!(!is_compatible_version("1.2.0"));
    }

    #[test]
    fn invalid_json_is_reported() {
        let err = decode_str("{not json").unwrap_err();
        assert!(matches!(err, FlowchartCodecError::InvalidJson(_)));
        assert!(err.to_string().contains("Invalid flowchart JSON"));
    }

    #[test]
    fn three_d_scene_and_unknown_blocks_round_trip_losslessly() {
        // The compiler's signal_*3d scene blocks are typed; a genuinely-unknown
        // kind still loads (as Unknown) without dropping nodes or losing params,
        // and the whole document re-saves verbatim.
        use crate::models::flowchart::NodeKind;

        let json = r#"{
            "schema":"matforge.flowchart","version":"0.2.0",
            "flows":[{"id":"f","kind":"program","name":"scene",
                "nodes":[
                    {"id":"w","kind":"signal_world3d","data":{"params":{"gravity":"-9.81"}}},
                    {"id":"a","kind":"signal_actor3d","label":"Cube","data":{"params":{"shape":"box"}}},
                    {"id":"x","kind":"signal_future_widget","data":{"params":{"foo":"bar"}}},
                    {"id":"g","kind":"signal_gain"}
                ],"edges":[]}]
        }"#;

        let doc = decode_str(json).expect("3-D model should load");
        let nodes = &doc.flows[0].nodes;
        assert_eq!(nodes.len(), 4, "no node should be dropped");

        // The 3-D scene blocks are now first-class typed kinds.
        assert_eq!(nodes[0].kind, NodeKind::SignalWorld3D);
        assert_eq!(nodes[1].kind, NodeKind::SignalActor3D);
        // A kind the IDE does not type still loads as Unknown, tag preserved.
        assert_eq!(nodes[2].kind, NodeKind::Unknown);
        assert_eq!(nodes[2].kind_tag(), "signal_future_widget");
        assert_eq!(nodes[3].kind, NodeKind::SignalGain);

        // Re-encode → every kind and param survives verbatim.
        let encoded = encode_string(&doc).unwrap();
        assert!(encoded.contains("\"signal_world3d\""));
        assert!(encoded.contains("\"signal_actor3d\""));
        assert!(encoded.contains("\"signal_future_widget\""));
        assert!(encoded.contains("\"shape\""));
        assert!(!encoded.contains("\"unknown\""));

        // Decoding the re-encoded text yields an identical document (stable).
        let again = decode_str(&encoded).unwrap();
        assert_eq!(doc, again);
    }

    #[test]
    fn decodes_hand_authored_minimal_doc() {
        // Only id + kind on the node; codec fills defaults.
        let json = r#"{
            "schema":"matforge.flowchart","version":"0.1.0",
            "flows":[{"id":"f","kind":"program","name":"main",
                      "nodes":[{"id":"n1","kind":"assignment"}],"edges":[]}]
        }"#;
        let doc = decode_str(json).unwrap();
        let node = &doc.flows[0].nodes[0];
        assert_eq!(node.kind, crate::models::flowchart::NodeKind::Assignment);
        assert_eq!(node.label, "");
        assert_eq!(node.ui.position.x, 0.0);
    }
}
