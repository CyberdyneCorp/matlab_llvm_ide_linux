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

    /// Cap on retained animation frames per figure (bounds memory for long runs;
    /// the oldest frames roll off).
    const MAX_FRAMES: usize = 1200;

    /// Insert or, for a re-emit of the same `runtime_id`, **accumulate** the new
    /// frame onto the existing figure so a streamed `drawnow` loop becomes a
    /// replayable animation instead of overwriting in place. A figure with no
    /// `runtime_id` always inserts.
    pub fn upsert_runtime(&self, figure: PlotFigure) {
        let mut selected = None;
        self.figures.update(|figs| {
            if let Some(rid) = figure.runtime_id {
                if let Some(existing) = figs.iter_mut().find(|f| f.runtime_id == Some(rid)) {
                    selected = Some(existing.id);
                    existing.png_data = figure.png_data;
                    existing.title = figure.title;
                    for frame in figure.frames {
                        existing.frames.push(frame);
                    }
                    let overflow = existing.frames.len().saturating_sub(Self::MAX_FRAMES);
                    if overflow > 0 {
                        existing.frames.drain(0..overflow);
                    }
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

    fn frame_fig(rid: i64, byte: u8) -> PlotFigure {
        let mut f = PlotFigure::series(1, "F", PlotKind::Rendered, vec![], vec![]);
        f.runtime_id = Some(rid);
        f.png_data = Some(vec![byte]);
        f.frames = vec![vec![byte]];
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
    fn upsert_accumulates_frames_for_same_runtime_id() {
        let vm = PlotsViewModel::new();
        vm.upsert_runtime(frame_fig(7, 1));
        let first_id = vm.figures.get()[0].id;
        vm.upsert_runtime(frame_fig(7, 2)); // same runtime id -> append frame
        vm.upsert_runtime(frame_fig(7, 3));
        let figs = vm.figures.get();
        assert_eq!(figs.len(), 1, "frames accumulate onto one figure");
        // The figure id is stable and the frame history is the full sequence.
        assert_eq!(figs[0].id, first_id);
        assert_eq!(figs[0].frames, vec![vec![1u8], vec![2], vec![3]]);
        assert!(figs[0].is_animated());
        // png_data mirrors the latest frame.
        assert_eq!(figs[0].png_data, Some(vec![3]));
        assert_eq!(vm.selected_id.get(), Some(first_id));
        vm.upsert_runtime(frame_fig(8, 9)); // new runtime id -> separate figure
        assert_eq!(vm.figures.get().len(), 2);
    }

    #[test]
    fn upsert_caps_frame_history() {
        let vm = PlotsViewModel::new();
        for i in 0..(PlotsViewModel::MAX_FRAMES + 50) {
            vm.upsert_runtime(frame_fig(1, (i % 251) as u8));
        }
        assert_eq!(vm.figures.get()[0].frames.len(), PlotsViewModel::MAX_FRAMES);
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
