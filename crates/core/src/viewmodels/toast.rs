//! Transient action feedback ("Saved", "Compiled", "Exported"). The view shows
//! `message` for a couple of seconds whenever `revision` bumps — the counter
//! lets the same message fire again (e.g. two saves in a row). Pure + tested.

use crate::observable::Property;

pub struct ToastViewModel {
    pub message: Property<Option<String>>,
    /// Bumped on every `show`, so identical consecutive messages re-trigger.
    pub revision: Property<u64>,
}

impl Default for ToastViewModel {
    fn default() -> Self {
        ToastViewModel::new()
    }
}

impl ToastViewModel {
    pub fn new() -> ToastViewModel {
        ToastViewModel { message: Property::new(None), revision: Property::new(0) }
    }

    /// Flash `message`.
    pub fn show(&self, message: impl Into<String>) {
        self.message.set(Some(message.into()));
        self.revision.update(|r| *r += 1);
    }

    pub fn clear(&self) {
        self.message.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_sets_message_and_bumps_each_time() {
        let vm = ToastViewModel::new();
        assert_eq!(vm.message.get(), None);
        vm.show("Saved");
        assert_eq!(vm.message.get().as_deref(), Some("Saved"));
        let r1 = vm.revision.get();
        vm.show("Saved"); // same text still re-fires
        assert!(vm.revision.get() > r1);
        vm.clear();
        assert_eq!(vm.message.get(), None);
    }
}
