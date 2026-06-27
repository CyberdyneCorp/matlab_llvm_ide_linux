## Why

The matlab_llvm compiler (PR #420) added `sim3d.capture(world, actor)`, which pulls an actor's
recorded timeline back into the workspace as an `N`-by-7 matrix `[t, x, y, z, rx, ry, rz]` (and
`writematrix`/`csvwrite` to save it). The IDE can already surface such a variable in the Workspace
panel and inspect it as a heatmap, but it has no way to *view the trajectory*: the existing "Plot
As" actions flatten the whole matrix into one series, and "Plot Surface (3D)" treats it as a height
field — neither shows the path the actor travelled.

This change adds a focused **Plot Trajectory** action that draws the X–Y ground track from a
capture-shaped matrix, reusing the existing 2-D line renderer.

## What Changes

- Add `MatrixView::is_capture_trajectory()` and `MatrixView::capture_trajectory_xy()` — recognize an
  `N`-by-(≥4) `[t, x, y, z, …]` matrix and extract its X (column 2) and Y (column 3) position
  columns as `(xs, ys)`.
- Add a **Plot Trajectory (X–Y)** entry to the Workspace variable context menu. It captures the
  variable's value over the REPL (like the other plot actions) and, when the value is a
  capture-shaped matrix, adds a `Line2D` figure of the X–Y path to the Plots panel. A
  non-capture selection reports a status message instead of a misleading plot.
- Wire the deferred fulfilment in the view model: `request_trajectory_plot(name)` +
  `fulfil_pending_trajectory(name)` on the REPL value channel, mirroring the existing
  `request_plot` / `fulfil_pending_plot` flow.

## Capabilities

### Added Capabilities
- `workspace-inspector`: A Plot Trajectory action that visualizes a `sim3d.capture` matrix as its
  X–Y ground track.

## Impact

- **Modified code**:
  - `crates/core/src/models/workspace.rs` — `is_capture_trajectory()` / `capture_trajectory_xy()`
    on `MatrixView`.
  - `crates/core/src/viewmodels/main.rs` — `pending_trajectory` state, `request_trajectory_plot`,
    `fulfil_pending_trajectory`, called from the REPL value branch.
  - `crates/app/src/app_state.rs` — `plot_variable_trajectory(name)`.
  - `crates/app/src/ui.rs` — the Plot Trajectory menu entry.
- **Compiler dependency**: none beyond a `matlabc` that implements `sim3d.capture` (PR #420). The
  IDE only reads the resulting workspace matrix; it does not call the API.
- **Scope note**: this visualizes a capture matrix that is already in the workspace (the
  interpreted/REPL run path, where `whos` populates the panel). Surfacing run-path variables
  automatically and importing `writematrix` CSV files are out of scope here.
- **Testing**: unit tests for the `MatrixView` helpers and for the view-model fulfilment (a capture
  matrix yields an X–Y figure; a non-capture matrix yields none).
- **Docs**: `docs/roadmap.md` note.
