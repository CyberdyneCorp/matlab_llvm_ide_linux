//! REPL command window view model. Mirrors `ReplViewModel`: running state, the
//! input buffer with ↑/↓ history recall, the transcript, and routing of stdout
//! through the [`SentinelRouter`] so console text lands in the transcript while
//! structured workspace/value/figure payloads are returned to the caller (the
//! `MainViewModel`) to forward to the workspace/plots view models.

use std::cell::{Cell, RefCell};

use crate::models::{ConsoleLevel, ConsoleMessage};
use crate::observable::Property;
use crate::services::sentinels::{ReplEvent, SentinelRouter};

pub struct ReplViewModel {
    pub is_running: Property<bool>,
    pub input: Property<String>,
    pub history: Property<Vec<String>>,
    pub transcript: Property<Vec<ConsoleMessage>>,
    router: RefCell<SentinelRouter>,
    /// Index into `history` during recall; `None` = editing the live input.
    history_cursor: Cell<Option<usize>>,
}

impl Default for ReplViewModel {
    fn default() -> Self {
        ReplViewModel::new()
    }
}

impl ReplViewModel {
    pub fn new() -> ReplViewModel {
        ReplViewModel {
            is_running: Property::new(false),
            input: Property::new(String::new()),
            history: Property::new(Vec::new()),
            transcript: Property::new(Vec::new()),
            router: RefCell::new(SentinelRouter::new()),
            history_cursor: Cell::new(None),
        }
    }

    pub fn set_running(&self, running: bool) {
        if !running {
            self.router.borrow_mut().reset();
        }
        self.is_running.set_if_changed(running);
    }

    /// Submit the current input: record it in history + transcript, clear the
    /// input, and return the command for the caller to write to the process.
    /// Returns `None` for an empty/whitespace command.
    pub fn submit(&self) -> Option<String> {
        let command = self.input.get();
        let trimmed = command.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        self.history.update(|h| h.push(trimmed.clone()));
        self.transcript.update(|t| t.push(ConsoleMessage::new(ConsoleLevel::Command, format!(">> {trimmed}"))));
        self.input.set(String::new());
        self.history_cursor.set(None);
        Some(trimmed)
    }

    /// Route one stdout line. Console text is appended to the transcript and
    /// `None` is returned; structured payloads are returned for forwarding.
    pub fn feed_line(&self, line: &str) -> Option<ReplEvent> {
        match self.router.borrow_mut().consume(line) {
            Some(ReplEvent::Console(text)) => {
                let level = classify(&text);
                self.transcript.update(|t| t.push(ConsoleMessage::new(level, text)));
                None
            }
            other => other,
        }
    }

    /// ↑ — recall an older history entry into the input.
    pub fn recall_previous(&self) {
        let len = self.history.with(|h| h.len());
        if len == 0 {
            return;
        }
        let next = match self.history_cursor.get() {
            None => len - 1,
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_cursor.set(Some(next));
        self.input.set(self.history.with(|h| h[next].clone()));
    }

    /// ↓ — move toward newer history; past the newest returns to a blank input.
    pub fn recall_next(&self) {
        let len = self.history.with(|h| h.len());
        match self.history_cursor.get() {
            Some(i) if i + 1 < len => {
                self.history_cursor.set(Some(i + 1));
                self.input.set(self.history.with(|h| h[i + 1].clone()));
            }
            Some(_) => {
                self.history_cursor.set(None);
                self.input.set(String::new());
            }
            None => {}
        }
    }
}

fn classify(text: &str) -> ConsoleLevel {
    let lower = text.to_lowercase();
    if lower.contains("error") {
        ConsoleLevel::Error
    } else if lower.contains("warning") {
        ConsoleLevel::Warning
    } else {
        ConsoleLevel::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::sentinels::{VAL_BEGIN, VAL_END, WS_BEGIN, WS_END};

    #[test]
    fn submit_records_history_and_clears_input() {
        let vm = ReplViewModel::new();
        vm.input.set("1 + 2".into());
        assert_eq!(vm.submit().as_deref(), Some("1 + 2"));
        assert_eq!(vm.history.get(), vec!["1 + 2".to_string()]);
        assert!(vm.input.get().is_empty());
        assert_eq!(vm.transcript.get().len(), 1);
    }

    #[test]
    fn submit_ignores_blank() {
        let vm = ReplViewModel::new();
        vm.input.set("   ".into());
        assert!(vm.submit().is_none());
        assert!(vm.history.get().is_empty());
    }

    #[test]
    fn feed_console_line_appends_transcript() {
        let vm = ReplViewModel::new();
        assert!(vm.feed_line("ans = 3").is_none());
        assert_eq!(vm.transcript.get().last().unwrap().text, "ans = 3");
    }

    #[test]
    fn feed_classifies_error_lines() {
        let vm = ReplViewModel::new();
        vm.feed_line("error: undefined");
        assert_eq!(vm.transcript.get().last().unwrap().level, ConsoleLevel::Error);
    }

    #[test]
    fn workspace_block_is_returned_not_transcripted() {
        let vm = ReplViewModel::new();
        assert!(vm.feed_line(WS_BEGIN).is_none());
        assert!(vm.feed_line("a  1x1  double").is_none());
        let ev = vm.feed_line(WS_END);
        assert_eq!(ev, Some(ReplEvent::Workspace("a  1x1  double".into())));
        // the whos lines did not pollute the transcript
        assert!(vm.transcript.get().is_empty());
    }

    #[test]
    fn value_block_returned() {
        let vm = ReplViewModel::new();
        vm.feed_line(VAL_BEGIN);
        vm.feed_line("1 2 3");
        assert_eq!(vm.feed_line(VAL_END), Some(ReplEvent::Value("1 2 3".into())));
    }

    #[test]
    fn history_recall_up_and_down() {
        let vm = ReplViewModel::new();
        for cmd in ["a", "b", "c"] {
            vm.input.set(cmd.into());
            vm.submit();
        }
        vm.recall_previous();
        assert_eq!(vm.input.get(), "c");
        vm.recall_previous();
        assert_eq!(vm.input.get(), "b");
        vm.recall_next();
        assert_eq!(vm.input.get(), "c");
        vm.recall_next(); // past newest -> blank
        assert_eq!(vm.input.get(), "");
    }

    #[test]
    fn recall_clamps_at_oldest() {
        let vm = ReplViewModel::new();
        vm.input.set("only".into());
        vm.submit();
        vm.recall_previous();
        vm.recall_previous(); // stays at oldest
        assert_eq!(vm.input.get(), "only");
    }

    #[test]
    fn stopping_resets_router() {
        let vm = ReplViewModel::new();
        vm.feed_line(WS_BEGIN); // enter a block
        vm.set_running(false); // resets router mid-block
        assert!(vm.feed_line("plain").is_none());
        assert_eq!(vm.transcript.get().last().unwrap().text, "plain");
    }
}
