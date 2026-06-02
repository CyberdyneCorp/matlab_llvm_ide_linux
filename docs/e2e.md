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

| Scenario | Drives | Asserts |
|----------|--------|---------|
| find in files | `Ctrl+F`, types `disp` + Enter | `search_results` becomes non-zero |
| gutter breakpoint | clicks the gutter at a line | `active_breakpoints` gains/loses that line |
| F9 breakpoint | focuses the editor, presses F9 | a breakpoint is set at the cursor |
| live REPL | types `x = [1 2 3]` in the REPL + Enter | the Workspace gains variable `x` (real `matlabc -repl`) |

The REPL scenario is skipped if `matlabc` isn't found.

## Adding a scenario

Add a function to `run_e2e.py` using the `App` helpers (`wait_for`,
`wait_rect`, `click_window`, `key`, `type_text`) and `check(name, cond)`. To
drive a new widget robustly, record its rect via `e2e::set_*` in the app and add
it to the snapshot in `crates/app/src/e2e.rs`.
