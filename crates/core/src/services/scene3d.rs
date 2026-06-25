//! Detect whether an mflowLink model carries a 3-D scene — i.e. uses any of the
//! `signal_*3d` scene-graph blocks the compiler turns into a Babylon.js viewer
//! via `matlabc -emit-mflowlink-babylon`.
//!
//! These block kinds are not (yet) typed in [`NodeKind`](crate::models::flowchart::node::NodeKind),
//! so detection scans the persisted `.mflow` text for the kind tags rather than
//! relying on the typed node model. This keeps gating of the 3-D Scene action
//! correct even for models authored outside the IDE.

use crate::models::flowchart::FlowchartDocument;
use crate::services::flowchart_codec;

/// The scene-graph block kinds that make a model a 3-D scene. A model needs at
/// least a `signal_world3d` to render, but any of these signals 3-D intent.
pub const SCENE3D_KINDS: [&str; 6] = [
    "signal_world3d",
    "signal_actor3d",
    "signal_light3d",
    "signal_camera3d",
    "signal_sensor3d",
    "signal_collision3d",
];

/// Whether `tag` is one of the 3-D scene block kinds.
pub fn is_scene3d_kind(tag: &str) -> bool {
    SCENE3D_KINDS.contains(&tag)
}

/// Whether the serialized `.mflow` text references any 3-D scene block kind.
///
/// Matches the JSON `"kind": "<tag>"` form (whitespace-insensitive after the
/// colon) so a stray substring elsewhere cannot produce a false positive.
pub fn source_has_scene3d(mflow: &str) -> bool {
    SCENE3D_KINDS.iter().any(|kind| source_mentions_kind(mflow, kind))
}

/// Whether a loaded document contains a 3-D scene block. Re-encodes the document
/// and scans the result, so it works regardless of whether the blocks are typed.
pub fn document_has_scene3d(doc: &FlowchartDocument) -> bool {
    match flowchart_codec::encode_string(doc) {
        Ok(text) => source_has_scene3d(&text),
        Err(_) => false,
    }
}

/// True when `mflow` contains a `"kind"` field whose value equals `tag`.
fn source_mentions_kind(mflow: &str, tag: &str) -> bool {
    let needle = format!("\"{tag}\"");
    let mut from = 0;
    while let Some(rel) = mflow[from..].find(&needle) {
        let at = from + rel;
        if is_kind_value(&mflow[..at]) {
            return true;
        }
        from = at + needle.len();
    }
    false
}

/// Whether the text immediately preceding a quoted value is a `"kind":` key,
/// allowing arbitrary whitespace between the colon and the value.
fn is_kind_value(before: &str) -> bool {
    let trimmed = before.trim_end();
    let Some(rest) = trimmed.strip_suffix(':') else {
        return false;
    };
    rest.trim_end().ends_with("\"kind\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_every_scene3d_kind() {
        for kind in SCENE3D_KINDS {
            assert!(is_scene3d_kind(kind), "{kind} should be a 3-D scene kind");
            let mflow = format!(r#"{{"nodes":[{{"id":"a","kind":"{kind}"}}]}}"#);
            assert!(source_has_scene3d(&mflow), "{kind} should be detected");
        }
    }

    #[test]
    fn ignores_models_without_scene3d() {
        let mflow = r#"{"kind":"signal_flow","nodes":[
            {"id":"a","kind":"signal_constant"},
            {"id":"b","kind":"signal_scope"}
        ]}"#;
        assert!(!source_has_scene3d(mflow));
    }

    #[test]
    fn the_flat_trajectory_scope_is_not_a_3d_scene() {
        // signal_scope3d is the older flat x-y trajectory scope, not a scene.
        let mflow = r#"{"nodes":[{"id":"a","kind":"signal_scope3d"}]}"#;
        assert!(!source_has_scene3d(mflow));
        assert!(!is_scene3d_kind("signal_scope3d"));
    }

    #[test]
    fn tolerates_whitespace_after_the_kind_colon() {
        let mflow = "{\"kind\"  :  \"signal_world3d\"}";
        assert!(source_has_scene3d(mflow));
    }

    #[test]
    fn substring_in_a_non_kind_field_is_not_a_false_positive() {
        let mflow = r#"{"label":"signal_world3d demo","kind":"signal_constant"}"#;
        assert!(!source_has_scene3d(mflow));
    }
}
