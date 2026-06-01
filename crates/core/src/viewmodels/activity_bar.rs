//! Activity-bar selection (the 56px vertical icon strip). Mirrors
//! `ActivityBarViewModel` — one selected item at a time.

use crate::observable::Property;

/// The eight activity-bar destinations (matches the reference order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActivityItem {
    Explorer,
    Search,
    Run,
    Compiler,
    Hdl,
    Debug,
    Docs,
    Flowchart,
}

impl ActivityItem {
    pub const ALL: [ActivityItem; 8] = [
        ActivityItem::Explorer,
        ActivityItem::Search,
        ActivityItem::Run,
        ActivityItem::Compiler,
        ActivityItem::Hdl,
        ActivityItem::Debug,
        ActivityItem::Docs,
        ActivityItem::Flowchart,
    ];

    /// Caption shown under the icon.
    pub fn caption(self) -> &'static str {
        match self {
            ActivityItem::Explorer => "Explorer",
            ActivityItem::Search => "Search",
            ActivityItem::Run => "Run",
            ActivityItem::Compiler => "Compiler",
            ActivityItem::Hdl => "HDL",
            ActivityItem::Debug => "Debug",
            ActivityItem::Docs => "Docs",
            ActivityItem::Flowchart => "Flowchart",
        }
    }
}

pub struct ActivityBarViewModel {
    pub selected: Property<ActivityItem>,
}

impl Default for ActivityBarViewModel {
    fn default() -> Self {
        ActivityBarViewModel::new()
    }
}

impl ActivityBarViewModel {
    pub fn new() -> ActivityBarViewModel {
        ActivityBarViewModel { selected: Property::new(ActivityItem::Explorer) }
    }

    pub fn select(&self, item: ActivityItem) {
        self.selected.set_if_changed(item);
    }

    pub fn is_selected(&self, item: ActivityItem) -> bool {
        self.selected.get() == item
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn defaults_to_explorer() {
        let vm = ActivityBarViewModel::new();
        assert!(vm.is_selected(ActivityItem::Explorer));
    }

    #[test]
    fn select_changes_and_notifies_once() {
        let vm = ActivityBarViewModel::new();
        let count = Rc::new(Cell::new(0));
        let c2 = Rc::clone(&count);
        vm.selected.subscribe(move |_| c2.set(c2.get() + 1));
        vm.select(ActivityItem::Debug);
        assert!(vm.is_selected(ActivityItem::Debug));
        vm.select(ActivityItem::Debug); // no-op, no extra notification
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn captions_present() {
        for item in ActivityItem::ALL {
            assert!(!item.caption().is_empty());
        }
    }
}
