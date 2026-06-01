//! Open editor tab + per-line breakpoint config. Mirrors `Models.swift`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::ids::next_id;

/// Editor surface a tab is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabKind {
    Text,
    Flowchart,
}

/// Per-line breakpoint settings sent to the DAP adapter. An all-`None` config
/// is a plain unconditional breakpoint.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BreakpointConfig {
    pub condition: Option<String>,
    pub log_message: Option<String>,
    pub hit_condition: Option<String>,
}

impl BreakpointConfig {
    /// Plain breakpoint — pause unconditionally.
    pub fn plain() -> BreakpointConfig {
        BreakpointConfig::default()
    }

    pub fn is_conditional(&self) -> bool {
        self.condition.as_deref().is_some_and(|c| !c.is_empty())
    }
    pub fn is_log_point(&self) -> bool {
        self.log_message.as_deref().is_some_and(|c| !c.is_empty())
    }
    pub fn has_hit_count(&self) -> bool {
        self.hit_condition.as_deref().is_some_and(|c| !c.is_empty())
    }
}

/// One open editor tab.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorTab {
    pub id: u64,
    pub name: String,
    pub language: String,
    pub contents: String,
    pub kind: TabKind,
    pub url: Option<PathBuf>,
    pub is_dirty: bool,
    /// Per-line breakpoints keyed by 1-indexed line. `BTreeMap` keeps gutter
    /// iteration ordered. Always empty for flowchart tabs.
    pub breakpoints: BTreeMap<usize, BreakpointConfig>,
    /// 1-indexed line where the runtime is currently paused (drives the ▶).
    pub execution_line: Option<usize>,
}

impl EditorTab {
    pub fn text(name: impl Into<String>, language: impl Into<String>, contents: impl Into<String>) -> EditorTab {
        EditorTab {
            id: next_id(),
            name: name.into(),
            language: language.into(),
            contents: contents.into(),
            kind: TabKind::Text,
            url: None,
            is_dirty: false,
            breakpoints: BTreeMap::new(),
            execution_line: None,
        }
    }

    pub fn flowchart(name: impl Into<String>) -> EditorTab {
        EditorTab {
            id: next_id(),
            name: name.into(),
            language: "Flowchart".into(),
            contents: String::new(),
            kind: TabKind::Flowchart,
            url: None,
            is_dirty: false,
            breakpoints: BTreeMap::new(),
            execution_line: None,
        }
    }

    pub fn with_url(mut self, url: PathBuf) -> EditorTab {
        self.url = Some(url);
        self
    }

    /// Toggle a plain breakpoint on `line`. Returns whether it is now set.
    pub fn toggle_breakpoint(&mut self, line: usize) -> bool {
        if self.breakpoints.remove(&line).is_some() {
            false
        } else {
            self.breakpoints.insert(line, BreakpointConfig::plain());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_tab_defaults() {
        let t = EditorTab::text("a.m", "Matlab", "x = 1;");
        assert_eq!(t.kind, TabKind::Text);
        assert!(!t.is_dirty);
        assert!(t.breakpoints.is_empty());
    }

    #[test]
    fn toggle_breakpoint_round_trips() {
        let mut t = EditorTab::text("a.m", "Matlab", "");
        assert!(t.toggle_breakpoint(3));
        assert!(t.breakpoints.contains_key(&3));
        assert!(!t.toggle_breakpoint(3));
        assert!(t.breakpoints.is_empty());
    }

    #[test]
    fn breakpoint_config_predicates() {
        let plain = BreakpointConfig::plain();
        assert!(!plain.is_conditional() && !plain.is_log_point() && !plain.has_hit_count());
        let cond = BreakpointConfig { condition: Some("x>1".into()), ..Default::default() };
        assert!(cond.is_conditional());
        let empty_cond = BreakpointConfig { condition: Some(String::new()), ..Default::default() };
        assert!(!empty_cond.is_conditional());
    }

    #[test]
    fn flowchart_tab_kind() {
        assert_eq!(EditorTab::flowchart("d.mflow").kind, TabKind::Flowchart);
    }
}
