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
| P8 (partial) | Compile → artifact tab, Run pipeline (emit-llvm → clang → exec), diagnostics → PROBLEMS | ✅ verified in-app + integration tests |
| P13 | Integration tests vs. real `matlabc`; `docs/` | ✅ |

The app builds, runs, opens folders/files with highlighting, compiles through the
real `matlabc` to an artifact tab, and runs programs — all driven by the tested
MVVM core.

## Remaining (architecture in place; UI to build)

These have their **models + view models complete and tested**; what remains is
the GTK view + transport wiring:

| Phase | What | Foundation ready |
|-------|------|------------------|
| P8 | Live REPL + DAP debugger UI (set/step/locals/watch, exec-line gutter marker) | `ReplViewModel`, `DebugViewModel`, sentinel router, DAP framing all tested |
| P9 | Flowchart editor canvas (Cairo shapes, orthogonal edge routing, palette, inspector, pan/zoom) | `FlowchartViewModel`, full node/edge model, codec, palette specs |
| P10 | Plots panel (Cairo line/scatter/bar/area/histogram/heatmap, drag-from-workspace) | `PlotsViewModel`, `PlotFigure`/`MatrixView` models |
| P11 | mflowLink signal-flow standalone window (simulation transport, scopes) | signal-flow node kinds, solver/snapshot config models |
| P12 | mStateflow state-chart standalone window | state/junction/chart node kinds, chart symbols model |

## Editor refinements (deferred)

* Gutter with line numbers, breakpoint dots, and the yellow ▶ execution marker
  (a custom Cairo gutter alongside `GtkTextView`).
* Save As / new-file dialogs, find-in-files results UI, multi-root projects.
* Off-thread process streaming (the build-request / apply-result split is already
  in place to make this a wiring change, not a rewrite).
