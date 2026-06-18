//! Library + masked-block support. A flow with `kind: "library"` is a reusable
//! definition whose node fields may carry `${name}` mask placeholders; a masked
//! instance is an ordinary node referencing that flow (via `data.library_id`)
//! plus a `mask_params` map of values. This module discovers a library's mask
//! parameters and previews the `${name}` → value substitution. Pure functions.

use std::collections::BTreeMap;

use super::document::Flow;
use super::node::{FlowNode, ParamValue};

/// The text-bearing fields of a node that may hold `${name}` placeholders, as
/// `(field label, text)` pairs (skipping empties).
fn node_texts(node: &FlowNode) -> Vec<(String, String)> {
    let d = &node.data;
    let mut out = Vec::new();
    let mut push = |label: &str, value: &Option<String>| {
        if let Some(v) = value {
            if !v.is_empty() {
                out.push((label.to_string(), v.clone()));
            }
        }
    };
    push("value", &d.value);
    push("expression", &d.expression);
    push("rhs", &d.rhs);
    push("args", &d.args);
    push("cond", &d.cond);
    push("iter", &d.iter);
    push("sample_time", &d.sample_time);
    if let Some(params) = &d.params {
        for (k, v) in params {
            if let ParamValue::Str(s) = v {
                if !s.is_empty() {
                    out.push((k.clone(), s.clone()));
                }
            }
        }
    }
    out
}

/// Mask-parameter names referenced as `${name}` anywhere in `flow`'s nodes, in
/// first-seen order with duplicates removed.
pub fn mask_param_names(flow: &Flow) -> Vec<String> {
    let mut seen = Vec::new();
    for node in &flow.nodes {
        for (_, text) in node_texts(node) {
            for name in placeholders(&text) {
                if !seen.contains(&name) {
                    seen.push(name);
                }
            }
        }
    }
    seen
}

/// Every `${name}` placeholder name in `text`, in order (may repeat).
fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else { break };
        let name = after[..end].trim();
        if !name.is_empty() {
            out.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

/// Replace every `${name}` in `text` with its value from `values`; unknown names
/// are left verbatim (so an unset mask param is visible in the preview).
pub fn substitute(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str("${");
            rest = after;
            continue;
        };
        let name = after[..end].trim();
        match values.get(name) {
            Some(v) => out.push_str(v),
            None => out.push_str(&rest[start..start + 2 + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A human-readable preview of the library expanded with `values`: one line per
/// node field that contains a placeholder, `node.field = substituted`.
pub fn mask_preview(flow: &Flow, values: &BTreeMap<String, String>) -> String {
    let mut lines = Vec::new();
    for node in &flow.nodes {
        let label = if node.label.is_empty() {
            node.id.clone()
        } else {
            node.label.clone()
        };
        for (field, text) in node_texts(node) {
            if !placeholders(&text).is_empty() {
                lines.push(format!("{label}.{field} = {}", substitute(&text, values)));
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::{
        FlowKind, FlowNode, FlowPosition, FlowSignature, FlowUi, NodeData, NodeKind,
    };

    fn lib_flow() -> Flow {
        let mut gain = FlowNode::new(
            "g",
            NodeKind::SignalGain,
            "Gain",
            NodeKind::SignalGain.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x: 0.0, y: 0.0 }),
        );
        gain.data.params = Some(BTreeMap::from([(
            "gain".to_string(),
            ParamValue::Str("${k}".to_string()),
        )]));
        let mut cst = FlowNode::new(
            "c",
            NodeKind::SignalConstant,
            "Const",
            NodeKind::SignalConstant.default_ports(),
            NodeData::default(),
            FlowUi::at(FlowPosition { x: 0.0, y: 0.0 }),
        );
        cst.data.value = Some("${k} + ${b}".to_string());
        Flow::new(
            "lib_lp",
            FlowKind::Library,
            "Lowpass",
            FlowSignature::default(),
            vec![gain, cst],
            vec![],
            None,
        )
    }

    #[test]
    fn discovers_mask_params_in_first_seen_order() {
        let names = mask_param_names(&lib_flow());
        assert_eq!(names, vec!["k".to_string(), "b".to_string()]);
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let vals = BTreeMap::from([("k".to_string(), "2.5".to_string())]);
        assert_eq!(substitute("${k} + ${b}", &vals), "2.5 + ${b}");
        assert_eq!(substitute("no placeholders", &vals), "no placeholders");
    }

    #[test]
    fn preview_lists_substituted_fields() {
        let vals = BTreeMap::from([
            ("k".to_string(), "3".to_string()),
            ("b".to_string(), "1".to_string()),
        ]);
        let preview = mask_preview(&lib_flow(), &vals);
        assert!(preview.contains("Gain.gain = 3"));
        assert!(preview.contains("Const.value = 3 + 1"));
    }
}
