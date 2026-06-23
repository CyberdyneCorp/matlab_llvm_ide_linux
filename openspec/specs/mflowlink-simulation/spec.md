# mflowlink-simulation Specification

## Purpose

The mflowLink simulation window: runs a signal-flow model through `matlabc`
(one-shot `-simulate` CSV or live `-simulate --sim-dap`) and visualizes the
streamed `SimTrace` on production-grade scopes, with playback transport. All
scope math lives in the GTK-free `services::scope` core.

## Requirements

### Requirement: Production overlay scope

The simulation window SHALL render all logged signals on a single overlay scope
with a legend, stable per-signal colors, grid, and numeric axis ticks, and SHALL
let the user inspect and reframe the trace. Hovering shows a crosshair with the
cursor time and each signal's nearest-sample value. The view is driven entirely
by the `SimTrace` — no compiler involvement.

#### Scenario: Signals overlay with a legend and stable colors

- **WHEN** a run logs more than one signal
- **THEN** every signal is drawn on one set of axes with a legend, each keeping a
  fixed palette color across redraws

#### Scenario: Box-zoom pins the visible window

- **WHEN** the user left-drags a rectangle over the plot
- **THEN** the scope zooms to that data window (both axes pinned)

#### Scenario: Autoscale and manual Y range

- **WHEN** the user clicks **Auto**
- **THEN** the scope reframes to fit all data; and **WHEN** the user enters a
  valid **Y min** < **Y max**, the Y axis is pinned to that range

#### Scenario: Export the visible trace and the tile

- **WHEN** the user exports CSV with the X axis pinned to a window
- **THEN** only the rows whose time falls inside that window are written; and the
  PNG export writes the rendered scope beside the model file

### Requirement: Image-signal visualization

The simulation window SHALL reconstruct 2-D and rank-3 colour **image signals**
from the flat per-element scope traces the simulator logs (`base[i,j]` /
`base[i,j,k]`, 1-based) and render each as a heatmap tile (grayscale for a 2-D
image, RGB for a rank-3 colour image) instead of N unreadable pixel traces. The
strip is shown only when the trace carries image-shaped signals, and the tile
reflects the frame at the current playback cursor. A 1-D vector signal SHALL NOT
be treated as an image.

#### Scenario: A 2-D image signal renders as a heatmap

- **WHEN** a run logs an image block's pixels as `base[i,j]` element traces
- **THEN** the simulation window draws a `rows×cols` grayscale heatmap tile for
  that image

#### Scenario: A colour image renders RGB

- **WHEN** the pixels carry a third subscript (`base[i,j,k]`, `k` = channel)
- **THEN** the tile renders in colour using the three channels per pixel

#### Scenario: Scalar / vector signals are not images

- **WHEN** the trace has only scalar or 1-D `v[i]` signals
- **THEN** no image tile is shown and the signals appear on the overlay scope

### Requirement: To Workspace publishes into the REPL workspace

When a run finishes, the simulation window SHALL publish each `signal_to_workspace`
(To Workspace) sink's logged series into the live REPL workspace as a column
vector named by the block's `variableName`, so the outputs appear in the
Workspace panel and `whos` (and can be inspected / plotted like any variable).
Sinks with no logged column are skipped.

#### Scenario: To Workspace outputs become workspace variables

- **WHEN** a model with To Workspace sinks (`simout`, `held`) finishes simulating
- **THEN** `simout` and `held` appear in the Workspace panel and `whos` lists them

### Requirement: Playback transport

The simulation window SHALL provide play / pause / step / step-back / restart
controls. A finished one-shot run replays by scrubbing a playback cursor through
the trace; a live `--sim-dap` session steps the solver.

#### Scenario: Play animates the playback cursor

- **WHEN** the user presses **Play** on a finished one-shot trace
- **THEN** the cursor advances through the samples and the scope redraws to it

#### Scenario: Step advances one sample / major step

- **WHEN** the user presses **Step**
- **THEN** a one-shot replay advances one sample, and a live session requests one
  major step

#### Scenario: Step-back rewinds the live trace

- **WHEN** the user steps a live run backward
- **THEN** the solver restores the previous major step and the scope trace is
  truncated to that time, with no samples left past the rewound cursor

### Requirement: Live streaming and breakpoints

In live `--sim-dap` mode the window SHALL stream `signalSample` events into the
scopes as the solver runs, halo the currently-active block on the model canvas,
and honor signal-value and simulation-time breakpoints with a snapshot
indicator. Signal breakpoints are keyed by the watched signal's source block;
any per-wire breakpoints carried by the model are installed automatically when
the live session starts.

#### Scenario: Streamed samples appear live

- **WHEN** the solver emits signal samples during a live run
- **THEN** the scopes extend with each streamed sample without re-running

#### Scenario: A breakpoint pauses the run

- **WHEN** a configured signal-value or simulation-time breakpoint is hit (e.g.
  on a MATLAB Function block's output)
- **THEN** the run pauses on that block and the transport reflects the paused state

#### Scenario: Model wire breakpoints are installed on start

- **WHEN** a live session starts for a model whose wires carry breakpoints
- **THEN** each wire's condition is installed against its source block's output
