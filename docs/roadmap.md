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

> **Known external blocker — `matlabc -dap`:** the DAP debugger client is
> complete and performs the correct `initialize → launch → setBreakpoints →
> configurationDone` handshake, but the shipped `matlabc -dap` **segfaults**
> before emitting a `stopped` event (and references a stale source path), so
> stepping/locals can't yet be exercised end-to-end. The IDE detects the adapter
> crash and tears the session down gracefully. This is a compiler-side bug, not
> an IDE issue; once `matlabc -dap` is fixed, the existing UI works unchanged.

## Remaining (architecture in place; UI to build)

These have their **models + view models complete and tested**; what remains is
the GTK view + transport wiring:

| Phase | What | Foundation ready |
|-------|------|------------------|
| P9+ | Flowchart inspector (per-kind fields), emitted-MATLAB preview, edge-drawing by drag, per-node breakpoint toggling | `FlowchartViewModel`, `SignalFlowParamSpec`, codec |
| P10+ | Plots: heatmap + 3D surface, runtime-PNG blit (needs cairo `png` feature), drag-workspace-var→figure | `PlotsViewModel`, `MatrixView` |
| P11 | mflowLink signal-flow standalone window (simulation transport, scopes) | signal-flow node kinds, solver/snapshot config models |
| P12 | mStateflow state-chart standalone window | state/junction/chart node kinds, chart symbols model |
| — | Watch box, function/data breakpoint panels (DAP plumbing done), Save As / find-results UI | respective view models |

## Editor refinements (deferred)

* Gutter with line numbers, breakpoint dots, and the yellow ▶ execution marker
  (a custom Cairo gutter alongside `GtkTextView`).
* Save As / new-file dialogs, find-in-files results UI, multi-root projects.
* Off-thread process streaming (the build-request / apply-result split is already
  in place to make this a wiring change, not a rewrite).
