# Roadmap & status

The goal is a faithful full port of the macOS IDE. The architecture is built so
every deferred feature slots into an existing, tested layer. Status as of the
current build:

## Complete

| Phase | What | State |
|-------|------|-------|
| P0 | Cargo workspace, `Property<T>` reactivity, dark CSS theme, runnable GTK shell | ✅ tested |
| P1 | All models — project tree, editor tabs, **full `.mflow` schema** (84 node kinds, signal-flow + state-chart), plots, console, compiler config, DAP types | ✅ ~100% |
| P2 | Services — syntax highlighter (8 languages), `.mflow` codec, `whos`/`disp` parsers, sentinel router + base64, DAP framing, compiler argv + diagnostics, run pipeline, file system, settings, clipboard/picker (traits + fakes) | ✅ ~95% |
| P3 | View models — main (composition root), editor, explorer, console, workspace, plots, debug, repl, layout, search, breakpoints, toolbar, status, activity bar, **flowchart (with undo/redo)** | ✅ ~95% |
| P4–P7 | GTK views — main window layout, 3-row toolbar with target/opt pickers, activity bar, Explorer tree, editor tabs with live syntax highlighting + cursor→status, console + artifact tabs + REPL input, workspace table, status bar | ✅ runnable |
| P8 | Compile → artifact tab; Run pipeline (emit-llvm → clang → exec); diagnostics → PROBLEMS; **live `matlabc -repl`** with workspace sync; **DAP debugger UI** (Debug panel: stepping toolbar, call stack, locals; editor gutter with line numbers, breakpoint dots, ▶ exec marker, click-to-toggle) | ✅ in-app + integration tests |
| P9 | Flowchart editor canvas — Cairo shapes (ellipse/diamond/hexagon/parallelogram/rect), orthogonal edge routing, BLOCKS palette, select/drag/zoom, undo/redo, opens `.mflow` | ✅ in-app |
| P10 | Plots panel — Cairo line/multi-line/scatter/bar/area/histogram, figure list, auto-switch on new figure | ✅ in-app |
| P13 | Integration tests vs. real `matlabc`; `docs/` | ✅ |

The app builds, runs, opens folders/files with highlighting, compiles through the
real `matlabc` to an artifact tab, runs programs, evaluates live REPL commands
with workspace sync, renders flowchart `.mflow` documents on a Cairo canvas, and
draws plots — all driven by the tested MVVM core.

> **`.m` debugger (`matlabc -dap`) — live.** The earlier launch crash
> (`matlabc -dap` aborting before the first `stopped` event, from duplicate
> sibling-`.m` symbols) is fixed upstream. The DAP client now drives
> `initialize → launch → setBreakpoints → configurationDone` to a verified
> breakpoint and a `stopped` event end-to-end, so stepping / call stack /
> locals work. Covered by the `dap_reaches_stopped_at_breakpoint` integration
> test (gated on a real `matlabc`).

## Recently shipped (signal-flow / mflowLink)

* **mflowLink editor & simulation** — `▶ Simulate` window with live `--sim-dap`
  transport (Play / Pause / **Step** / **Step Back** / Restart), the production
  overlay scope, and snapshot step-back that truncates the live trace correctly.
* **Wire routing** — orthogonal routing that avoids node bodies, fan-out junction
  dots, and per-net lanes so unrelated signals never overlap; click-to-select and
  Delete a wire.
* **MATLAB Function block** — double-click opens a MATLAB source editor; ports
  follow the function signature (`u1..uN` → `out`). The editor has a line-number
  gutter, an edit toolbar (undo/redo/clipboard), find (Ctrl+F), current-line
  highlight, and auto-indent. Gutter clicks set persisted source-line
  breakpoints (markers only — not yet honored by the simulator), and a **Break on
  output** control sets the signal breakpoint the simulator does honor.
* **Editor block library** tracks the simulator, including the **MPC Controller**
  and the From Workspace / n-D Lookup / If / Switch-Case Action / custom blocks.
* **Breakpoints** — per-wire signal breakpoints (persist on the edge, marker on
  the canvas, installed against the source block on a live run).
* **Robustness** — new wires/blocks get collision-free ids (no duplicate-edge-id
  on save), and multi-input ports spread along a block face instead of collapsing.

## Remaining (architecture in place; UI to build)

These have their **models + view models complete and tested**; what remains is
the GTK view + transport wiring:

| Phase | What | Foundation ready |
|-------|------|------------------|
| P10+ | Plots: heatmap + 3D surface, runtime-PNG blit (needs cairo `png` feature), drag-workspace-var→figure | `PlotsViewModel`, `MatrixView` |
| — | Watch box, function/data breakpoint panels (DAP plumbing done), Save As / find-results UI | respective view models |

## Editor refinements (deferred)

* Gutter with line numbers, breakpoint dots, and the yellow ▶ execution marker
  (a custom Cairo gutter alongside `GtkTextView`).
* Save As / new-file dialogs, find-in-files results UI, multi-root projects.
* Off-thread process streaming (the build-request / apply-result split is already
  in place to make this a wiring change, not a rewrite).
