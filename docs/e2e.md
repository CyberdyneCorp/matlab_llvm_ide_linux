# End-to-end testing

The view-model logic is covered by the GTK-free unit tests in `matforge-core`
(~95%). The **e2e** layer covers the thin "is the GTK wiring actually connected"
question by driving the **real binary** and asserting on **real application
state** — not pixels.

## How it works

This is the practical, dependency-light alternative to Playwright for a native
GTK app (the other option being AT-SPI/`dogtail`, which needs the a11y bus):

1. **Input** — the harness synthesizes real X11 pointer/keyboard events
   (`python-xlib` XTEST), so clicks and key presses flow through the actual GTK
   event handlers.
2. **State, not pixels** — when `$MATFORGE_E2E_STATE` is set, the app writes a
   periodic JSON snapshot of testable state (active tab, breakpoints, workspace
   variables, plots, panel visibility, status) to that path. Assertions read it.
   This code (`crates/app/src/e2e.rs`) is zero-cost unless the env var is set.
3. **Robust targets** — the snapshot also includes the on-screen rectangles of
   the drive targets (editor gutter, REPL entry), via `compute_point` to the
   window, so the harness clicks real coordinates instead of guessing.

```
e2e/
  harness.py     App launch + XTEST input + state polling + assert helpers
  run_e2e.py     the scenarios
  requirements.txt
```

## Run it

Needs a running X display and the harness dependency:

```sh
just e2e-setup      # pip install --user python-xlib   (no sudo)
just e2e            # builds, then runs the scenarios
```

Headless / CI: wrap with Xvfb — `xvfb-run -a just e2e`.

## Scenarios

| Scenario | Drives | Asserts | Needs `matlabc` |
|----------|--------|---------|:---:|
| find in files | `Ctrl+F`, types `disp` + Enter | `search_results` becomes non-zero | |
| problems pane | launches with a bad file + compile | `problems` (diagnostics) becomes non-zero | ✓ |
| gutter breakpoint | clicks the gutter at a line | `active_breakpoints` gains/loses that line | |
| F9 breakpoint | focuses the editor, presses F9 | a breakpoint is set at the cursor | |
| explorer double-click | single- then double-clicks a tree row | single click selects only; double click opens the tab | |
| flowchart editor | opens a demo chart, clicks a BLOCKS palette row | the chart loads with nodes/edges; the palette adds a node | |
| mflowLink simulate | opens the signal-flow window (autostart) | the simulation streams samples and reaches `Finished` | ✓ |
| mStateflow trace | opens the state-chart window (autostart) | the trace streams events and activates a state | ✓ |
| live REPL | types `x = [1 2 3]` in the REPL + Enter | the Workspace gains variable `x` | ✓ |
| inspect + plot | inspects a workspace var, clicks Plots `+` | the value table shows; a figure is added | ✓ |
| REPL plot | types `plot([...])` + Enter | a figure is added | ✓ |
| plot animation | inspects + plots a vector, clicks play | the figure is scrub-able (`plot_anim > 1`) and survives playback | ✓ |
| debug session | pauses at a breakpoint, steps, watches, continues | `debug_state` cycles Paused → stepped line → watch result → Terminated | ✓ |

Scenarios marked **Needs `matlabc`** skip cleanly when the compiler isn't found.
The mflowLink / mStateflow scenarios drive standalone windows: their env hooks
(`MATFORGE_SIMULATE` / `MATFORGE_STATECHART`) open the window and autostart the
run, so the harness only reads the published state — no input into the separate
window is required. They use the bundled `e2e/fixtures/{signal,chart}.mflow`.

> **Local display note:** the `Ctrl+F` find-in-files scenario relies on a
> keyboard *accelerator*, which needs a window manager to route focus; it passes
> under CI's `xvfb` but can fail under a bare nested X server (e.g. `Xephyr`).
> Plain keys (F9) and all click-driven scenarios are unaffected.

## Adding a scenario

Add a function to `run_e2e.py` using the `App` helpers (`wait_for`,
`wait_rect`, `click_window`, `key`, `type_text`) and `check(name, cond)`. To
drive a new widget robustly, record its rect via `e2e::set_*` in the app and add
it to the snapshot in `crates/app/src/e2e.rs`.
