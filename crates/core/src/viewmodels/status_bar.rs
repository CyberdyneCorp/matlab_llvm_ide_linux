//! Status-bar view model — wraps `StatusBarState` with targeted setters so the
//! editor can push cursor position and the compile pipeline can push messages
//! without clobbering unrelated fields. Mirrors the status slice of
//! `SmallViewModels.swift`.

use crate::models::StatusBarState;
use crate::observable::Property;

pub struct StatusBarViewModel {
    pub state: Property<StatusBarState>,
}

impl Default for StatusBarViewModel {
    fn default() -> Self {
        StatusBarViewModel::new()
    }
}

impl StatusBarViewModel {
    pub fn new() -> StatusBarViewModel {
        StatusBarViewModel { state: Property::new(StatusBarState::default()) }
    }

    pub fn set_cursor(&self, line: usize, column: usize) {
        self.state.update(|s| {
            s.line = line;
            s.column = column;
        });
    }

    pub fn set_message(&self, message: impl Into<String>) {
        let message = message.into();
        self.state.update(|s| s.message = message);
    }

    pub fn set_language(&self, language: impl Into<String>) {
        let language = language.into();
        self.state.update(|s| s.language = language);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_update_preserves_other_fields() {
        let vm = StatusBarViewModel::new();
        vm.set_message("Compiling…");
        vm.set_cursor(12, 4);
        let s = vm.state.get();
        assert_eq!((s.line, s.column), (12, 4));
        assert_eq!(s.message, "Compiling…"); // not clobbered
    }

    #[test]
    fn language_update() {
        let vm = StatusBarViewModel::new();
        vm.set_language("C++");
        assert_eq!(vm.state.get().language, "C++");
    }
}
