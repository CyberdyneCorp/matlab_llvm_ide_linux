//! Console / problems / artifact panel state. Mirrors `ConsoleViewModel`:
//! a transcript of messages, a diagnostics list, per-target artifact buffers,
//! and the dynamic visible-tab set (CONSOLE + PROBLEMS always; an artifact tab
//! appears once its buffer is populated).

use std::collections::BTreeMap;

use crate::models::{CompilerTarget, ConsoleLevel, ConsoleMessage, ConsoleTab};
use crate::observable::Property;
use crate::services::compiler::Diagnostic;

pub struct ConsoleViewModel {
    pub messages: Property<Vec<ConsoleMessage>>,
    pub active_tab: Property<ConsoleTab>,
    pub problems: Property<Vec<Diagnostic>>,
    /// Generated-artifact text keyed by its tab.
    pub artifacts: Property<BTreeMap<ConsoleTab, String>>,
}

impl Default for ConsoleViewModel {
    fn default() -> Self {
        ConsoleViewModel::new()
    }
}

impl ConsoleViewModel {
    pub fn new() -> ConsoleViewModel {
        ConsoleViewModel {
            messages: Property::new(Vec::new()),
            active_tab: Property::new(ConsoleTab::Console),
            problems: Property::new(Vec::new()),
            artifacts: Property::new(BTreeMap::new()),
        }
    }

    pub fn append(&self, message: ConsoleMessage) {
        self.messages.update(|m| m.push(message));
    }

    pub fn log(&self, level: ConsoleLevel, text: impl Into<String>) {
        self.append(ConsoleMessage::new(level, text));
    }

    pub fn clear(&self) {
        self.messages.update(|m| m.clear());
    }

    pub fn set_active_tab(&self, tab: ConsoleTab) {
        self.active_tab.set_if_changed(tab);
    }

    pub fn set_problems(&self, problems: Vec<Diagnostic>) {
        self.problems.set(problems);
    }

    /// Map a compile target onto its artifact tab.
    pub fn tab_for_target(target: CompilerTarget) -> ConsoleTab {
        match target {
            CompilerTarget::Cpp => ConsoleTab::Cpp,
            CompilerTarget::C => ConsoleTab::Cpp,
            CompilerTarget::Llvm => ConsoleTab::LlvmIr,
            CompilerTarget::Python => ConsoleTab::Python,
            CompilerTarget::TypeScript => ConsoleTab::TypeScript,
            CompilerTarget::Mlir => ConsoleTab::Mlir,
            CompilerTarget::Sv => ConsoleTab::SystemVerilog,
            CompilerTarget::Va => ConsoleTab::VerilogA,
        }
    }

    /// Store an artifact for `target` and switch focus to its tab.
    pub fn set_artifact(&self, target: CompilerTarget, text: impl Into<String>) {
        let tab = Self::tab_for_target(target);
        let text = text.into();
        self.artifacts.update(|a| {
            a.insert(tab, text);
        });
        self.set_active_tab(tab);
    }

    /// The tabs currently visible: CONSOLE + PROBLEMS, then any artifact tabs
    /// that have content (in a stable order).
    pub fn visible_tabs(&self) -> Vec<ConsoleTab> {
        let order = [
            ConsoleTab::LlvmIr,
            ConsoleTab::Cpp,
            ConsoleTab::Python,
            ConsoleTab::TypeScript,
            ConsoleTab::SystemVerilog,
            ConsoleTab::Mlir,
            ConsoleTab::VerilogA,
        ];
        let mut tabs = vec![ConsoleTab::Console, ConsoleTab::Problems];
        self.artifacts.with(|a| {
            for t in order {
                if a.contains_key(&t) {
                    tabs.push(t);
                }
            }
        });
        tabs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_clear() {
        let vm = ConsoleViewModel::new();
        vm.log(ConsoleLevel::Info, "ready");
        vm.log(ConsoleLevel::Error, "boom");
        assert_eq!(vm.messages.get().len(), 2);
        vm.clear();
        assert!(vm.messages.get().is_empty());
    }

    #[test]
    fn artifact_sets_buffer_and_focuses_tab() {
        let vm = ConsoleViewModel::new();
        vm.set_artifact(CompilerTarget::Llvm, "; ir");
        assert_eq!(vm.active_tab.get(), ConsoleTab::LlvmIr);
        assert_eq!(vm.artifacts.get().get(&ConsoleTab::LlvmIr).unwrap(), "; ir");
    }

    #[test]
    fn visible_tabs_grow_with_artifacts() {
        let vm = ConsoleViewModel::new();
        assert_eq!(vm.visible_tabs(), vec![ConsoleTab::Console, ConsoleTab::Problems]);
        vm.set_artifact(CompilerTarget::Cpp, "int main(){}");
        vm.set_artifact(CompilerTarget::Python, "print(1)");
        let tabs = vm.visible_tabs();
        assert!(tabs.contains(&ConsoleTab::Cpp));
        assert!(tabs.contains(&ConsoleTab::Python));
        // C and C++ share the Cpp tab
        assert_eq!(ConsoleViewModel::tab_for_target(CompilerTarget::C), ConsoleTab::Cpp);
    }

    #[test]
    fn problems_set() {
        let vm = ConsoleViewModel::new();
        vm.set_problems(vec![Diagnostic {
            file: "a.m".into(),
            line: 1,
            column: 2,
            level: crate::services::compiler::DiagnosticLevel::Error,
            message: "bad".into(),
        }]);
        assert_eq!(vm.problems.get().len(), 1);
    }
}
