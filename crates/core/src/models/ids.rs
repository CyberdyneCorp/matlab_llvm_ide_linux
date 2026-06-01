//! Process-wide monotonic id generator. Stands in for the macOS reference's
//! `let id = UUID()` synthetic identity on non-serialized model structs
//! (`EditorTab`, `ProjectNode`, `PlotFigure`, …). Serialized flowchart nodes
//! keep their schema string ids instead and never use this.

use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

/// Next unique id. Never returns 0 (so `0` can mean "none" where needed).
pub fn next_id() -> u64 {
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_monotonic() {
        let a = next_id();
        let b = next_id();
        let c = next_id();
        assert!(a < b && b < c);
        assert_ne!(a, b);
    }

    #[test]
    fn ids_are_never_zero() {
        assert!(next_id() > 0);
    }
}
