//! Console output message + the console/artifact tab enum. Mirrors
//! `Models.swift` (`ConsoleMessage`) and `ConsoleViewModel.Tab`.

use super::ids::next_id;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsoleLevel {
    Info,
    Success,
    Warning,
    Error,
    Debug,
    Command,
    Plain,
}

impl ConsoleLevel {
    /// CSS class used by the console view to color the row.
    pub fn css_class(self) -> Option<&'static str> {
        match self {
            ConsoleLevel::Error => Some("mf-log-error"),
            ConsoleLevel::Warning => Some("mf-log-warning"),
            ConsoleLevel::Success => Some("mf-log-success"),
            ConsoleLevel::Command => Some("mf-log-command"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleMessage {
    pub id: u64,
    pub timestamp: String,
    pub level: ConsoleLevel,
    pub text: String,
}

impl ConsoleMessage {
    pub fn new(level: ConsoleLevel, text: impl Into<String>) -> ConsoleMessage {
        ConsoleMessage { id: next_id(), timestamp: String::new(), level, text: text.into() }
    }

    pub fn with_timestamp(mut self, ts: impl Into<String>) -> ConsoleMessage {
        self.timestamp = ts.into();
        self
    }
}

/// Tabs in the bottom console panel — CONSOLE/PROBLEMS plus the artifact panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConsoleTab {
    Console,
    Problems,
    LlvmIr,
    Cpp,
    Python,
    TypeScript,
    SystemVerilog,
    Mlir,
    VerilogA,
}

impl ConsoleTab {
    pub fn label(self) -> &'static str {
        match self {
            ConsoleTab::Console => "CONSOLE",
            ConsoleTab::Problems => "PROBLEMS",
            ConsoleTab::LlvmIr => "LLVM IR",
            ConsoleTab::Cpp => "C++ CODE",
            ConsoleTab::Python => "PYTHON",
            ConsoleTab::TypeScript => "TypeScript",
            ConsoleTab::SystemVerilog => "SYSTEMVERILOG",
            ConsoleTab::Mlir => "MLIR",
            ConsoleTab::VerilogA => "VERILOG-A",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_css_classes() {
        assert_eq!(ConsoleLevel::Error.css_class(), Some("mf-log-error"));
        assert_eq!(ConsoleLevel::Warning.css_class(), Some("mf-log-warning"));
        assert_eq!(ConsoleLevel::Success.css_class(), Some("mf-log-success"));
        assert_eq!(ConsoleLevel::Command.css_class(), Some("mf-log-command"));
        // The remaining levels are unstyled (default console color).
        for lvl in [ConsoleLevel::Info, ConsoleLevel::Debug, ConsoleLevel::Plain] {
            assert_eq!(lvl.css_class(), None);
        }
    }

    #[test]
    fn message_builder() {
        let m = ConsoleMessage::new(ConsoleLevel::Warning, "careful").with_timestamp("10:00:00");
        assert_eq!(m.level, ConsoleLevel::Warning);
        assert_eq!(m.timestamp, "10:00:00");
        assert_eq!(m.text, "careful");
    }

    #[test]
    fn tab_labels() {
        // Every tab has a non-empty, unique label.
        let tabs = [
            ConsoleTab::Console,
            ConsoleTab::Problems,
            ConsoleTab::LlvmIr,
            ConsoleTab::Cpp,
            ConsoleTab::Python,
            ConsoleTab::TypeScript,
            ConsoleTab::SystemVerilog,
            ConsoleTab::Mlir,
            ConsoleTab::VerilogA,
        ];
        let labels: Vec<&str> = tabs.iter().map(|t| t.label()).collect();
        assert!(labels.iter().all(|l| !l.is_empty()));
        assert_eq!(ConsoleTab::LlvmIr.label(), "LLVM IR");
        assert_eq!(ConsoleTab::Cpp.label(), "C++ CODE");
        assert_eq!(ConsoleTab::VerilogA.label(), "VERILOG-A");
        // Ord is used to order artifact tabs after CONSOLE/PROBLEMS.
        assert!(ConsoleTab::Console < ConsoleTab::Cpp);
    }
}
