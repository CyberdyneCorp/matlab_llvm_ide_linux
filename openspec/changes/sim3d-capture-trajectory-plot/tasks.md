## 1. Core: capture-matrix helpers

- [x] 1.1 Add `MatrixView::is_capture_trajectory()` (rows ≥ 2, cols ≥ 4) and
  `MatrixView::capture_trajectory_xy() -> Option<(Vec<f64>, Vec<f64>)>` (columns x=1, y=2) in
  `crates/core/src/models/workspace.rs`.
- [x] 1.2 Tests: extracts X/Y columns from an N×7 capture matrix; rejects thin/single-frame shapes.

## 2. Core: view-model fulfilment

- [x] 2.1 Add `pending_trajectory: Property<Option<String>>` + `request_trajectory_plot(name)`.
- [x] 2.2 Add `fulfil_pending_trajectory(name)` building a `Line2D` X–Y figure from the inspected
  capture matrix; status message on a non-capture selection.
- [x] 2.3 Call it from the REPL value branch alongside `fulfil_pending_plot`.
- [x] 2.4 Tests: a capture matrix yields an X–Y figure; a non-capture matrix yields none.

## 3. App: menu entry

- [x] 3.1 Add `AppState::plot_variable_trajectory(name)`.
- [x] 3.2 Add a "Plot Trajectory (X–Y)" item to the Workspace variable context menu in `ui.rs`.

## 4. Build & docs

- [x] 4.1 `cargo test` (core 489+7) green; clippy clean; app builds.
- [x] 4.2 Update `docs/roadmap.md`.
- [x] 4.3 `openspec validate sim3d-capture-trajectory-plot --strict`.
