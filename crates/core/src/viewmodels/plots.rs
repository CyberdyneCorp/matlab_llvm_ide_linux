//! Plots panel state. Mirrors `PlotsViewModel`: the figure list, current
//! selection, and an unread-arrival counter that drives the LIVE badge while
//! the panel is hidden. Runtime figures upsert by `runtime_id` so a re-emit of
//! the same figure replaces it in place instead of stacking duplicates.

use crate::models::PlotFigure;
use crate::observable::Property;

pub struct PlotsViewModel {
    pub figures: Property<Vec<PlotFigure>>,
    pub selected_id: Property<Option<u64>>,
    pub unread_count: Property<usize>,
}

impl Default for PlotsViewModel {
    fn default() -> Self {
        PlotsViewModel::new()
    }
}

impl PlotsViewModel {
    pub fn new() -> PlotsViewModel {
        PlotsViewModel {
            figures: Property::new(Vec::new()),
            selected_id: Property::new(None),
            unread_count: Property::new(0),
        }
    }

    /// Add a figure and select it, bumping the unread counter.
    pub fn add(&self, figure: PlotFigure) {
        let id = figure.id;
        self.figures.update(|f| f.push(figure));
        self.selected_id.set(Some(id));
        self.unread_count.update(|c| *c += 1);
    }

    /// Insert or replace by runtime id (figures from the emit protocol). A
    /// figure with no `runtime_id` always inserts.
    pub fn upsert_runtime(&self, figure: PlotFigure) {
        let mut selected = None;
        self.figures.update(|figs| {
            if let Some(rid) = figure.runtime_id {
                if let Some(existing) = figs.iter_mut().find(|f| f.runtime_id == Some(rid)) {
                    let id = existing.id;
                    *existing = figure;
                    selected = Some(id);
                    return;
                }
            }
            selected = Some(figure.id);
            figs.push(figure);
        });
        self.selected_id.set(selected);
        self.unread_count.update(|c| *c += 1);
    }

    pub fn select(&self, id: u64) {
        self.selected_id.set(Some(id));
    }

    pub fn remove(&self, id: u64) {
        self.figures.update(|f| f.retain(|fig| fig.id != id));
        if self.selected_id.get() == Some(id) {
            let next = self.figures.with(|f| f.last().map(|fig| fig.id));
            self.selected_id.set(next);
        }
    }

    pub fn remove_all(&self) {
        self.figures.set(Vec::new());
        self.selected_id.set(None);
    }

    pub fn mark_read(&self) {
        self.unread_count.set_if_changed(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PlotKind;

    fn fig(rid: Option<i64>) -> PlotFigure {
        let mut f = PlotFigure::series(1, "F", PlotKind::Line2D, vec![0.0], vec![1.0]);
        f.runtime_id = rid;
        f
    }

    #[test]
    fn add_selects_and_counts() {
        let vm = PlotsViewModel::new();
        let f = fig(None);
        let id = f.id;
        vm.add(f);
        assert_eq!(vm.figures.get().len(), 1);
        assert_eq!(vm.selected_id.get(), Some(id));
        assert_eq!(vm.unread_count.get(), 1);
    }

    #[test]
    fn upsert_replaces_same_runtime_id() {
        let vm = PlotsViewModel::new();
        vm.upsert_runtime(fig(Some(7)));
        let first_id = vm.figures.get()[0].id;
        vm.upsert_runtime(fig(Some(7))); // same runtime id -> replace
        assert_eq!(vm.figures.get().len(), 1);
        // selection points at the in-place id
        assert_eq!(vm.selected_id.get(), Some(first_id));
        vm.upsert_runtime(fig(Some(8))); // new id -> append
        assert_eq!(vm.figures.get().len(), 2);
    }

    #[test]
    fn remove_picks_new_selection() {
        let vm = PlotsViewModel::new();
        let a = fig(None);
        let a_id = a.id;
        vm.add(a);
        let b = fig(None);
        let b_id = b.id;
        vm.add(b);
        vm.remove(b_id);
        assert_eq!(vm.selected_id.get(), Some(a_id));
        vm.remove(a_id);
        assert!(vm.selected_id.get().is_none());
    }

    #[test]
    fn mark_read_resets_counter() {
        let vm = PlotsViewModel::new();
        vm.add(fig(None));
        vm.mark_read();
        assert_eq!(vm.unread_count.get(), 0);
    }

    #[test]
    fn remove_all_clears() {
        let vm = PlotsViewModel::new();
        vm.add(fig(None));
        vm.remove_all();
        assert!(vm.figures.get().is_empty());
        assert!(vm.selected_id.get().is_none());
    }
}
