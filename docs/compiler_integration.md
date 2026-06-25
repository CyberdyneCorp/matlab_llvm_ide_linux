# Compiler integration

How MatForge drives the `matlabc` C++ compiler. All command construction is pure
and unit-tested; the process execution is covered by the env-gated integration
tests in `crates/core/tests/integration.rs`.

## Compile (emit a target)

The toolbar Target picker maps to a `matlabc -emit-*` flag via
[`CompilerTarget::matlabc_flag`](../crates/core/src/models/compiler.rs):

| Target | Flag |
|--------|------|
| C++ | `-emit-cpp` |
| C | `-emit-c` |
| LLVM IR | `-emit-llvm` |
| Python | `-emit-python` |
| TypeScript | `-emit-ts` |
| MLIR | `-emit-mlir` |
| SystemVerilog | `-emit-sv` |
| Verilog-A | *(none — run-to-emit lane)* |

[`CompilerInvocation::emit`](../crates/core/src/services/compiler.rs) builds the
argv: `matlabc <flag> [-O] <source.m>` (`-O` is added for any profile above O0).
The generated artifact arrives on **stdout** and is shown in the matching console
tab; **stderr** is streamed to the console and parsed for diagnostics.

## Diagnostics

`matlabc` emits clang-style diagnostics:

```
/tmp/test.m:1:11: error: undefined name 'y'
```

[`parse_diagnostic`](../crates/core/src/services/compiler.rs) turns each line into
a structured `Diagnostic { file, line, column, level, message }` for the PROBLEMS
pane (click-to-jump).

## Run

Linux has no `build_and_run.sh`; the Run pipeline reproduces
`matlab_llvm/docs/build_and_run.md` in [`RunPlan`](../crates/core/src/services/run.rs):

1. `matlabc -emit-llvm source.m > <stem>.ll`
2. `clang++ -std=c++20 -O2 -Wno-override-module <stem>.ll libMatlabRuntime.a -ldl -lpthread -Wl,-dead_strip -o <stem>`
3. `./<stem>` — stdout is streamed back through the REPL sentinel router so any
   emitted figures land in the Plots panel.

## mflowLink 3-D Scene (`matlabc -emit-mflowlink-babylon`)

Signal-flow models that use the compiler's `signal_*3d` scene blocks
(`signal_world3d`, `signal_actor3d`, `signal_light3d`, `signal_camera3d`,
`signal_sensor3d`, `signal_collision3d`) can be rendered as an interactive 3-D
scene. The lane is the `Babylon` variant of
[`ExportTarget`](../crates/core/src/services/codegen.rs); the IDE persists the
model and runs `matlabc -emit-mflowlink-babylon <model.mflow> -o <model.scene.html>`,
producing a self-contained Babylon.js viewer with orbit/zoom/pan/play built in.

The **3-D Scene** button (flowchart toolbar and mflowLink window) is shown only
when [`scene3d::document_has_scene3d`](../crates/core/src/services/scene3d.rs)
detects a scene block in the model. The generated HTML opens in an embedded
WebKitGTK window when the IDE is built with the `scene3d` feature, otherwise in the
system browser. `MATFORGE_BABYLON_INLINE=<bundle.js>` inlines a Babylon runtime so
the embedded viewer renders offline.

The six `signal_*3d` scene blocks (`SignalWorld3D`, `SignalActor3D`,
`SignalLight3D`, `SignalCamera3D`, `SignalSensor3D`, `SignalCollision3D`) are
first-class typed blocks: they appear in the signal-flow palette under a **3-D
Scene** category, the inspector shows their parameters with labels and
validation (`SignalFlowParamSpec`), and the actor exposes signal-driven
`translation`/`rotation`/`scale` input ports.

Any *other* untyped block kind still loads as `NodeKind::Unknown` with the
original `kind` tag preserved on the `FlowNode` (`raw_kind`) so a model
round-trips without losing blocks or parameters, rendering with its real name
via `pretty_kind_tag` (not a bare "Unknown Block"); the inspector surfaces its
stored params as free-form rows. The same fallback covers params present on a
typed 3-D block that aren't in its curated schema.

## sim3d scripts (`sim3d.export` → 3-D Scene viewer)

`sim3d` is the compiler's MATLAB command-line 3-D API — `sim3d.World()`,
`sim3d.Actor(name, shape)`, transform properties, `w.add/open/run/close`, and
`sim3d.export(w, 'scene.html')` — which writes the same self-contained Babylon.js
HTML as `-emit-mflowlink-babylon`, authored entirely from `.m` code (no `.mflow`).

When you **Run** a `.m` file that uses sim3d, the IDE opens the exported scene in
the embedded 3-D Scene viewer automatically:

1. The Run pipeline compiles + links + executes the program in a temp working
   directory; `sim3d.export(w, 'x.html')` writes `x.html` there.
2. `sim3d.export` prints no marker, so the IDE finds the output by reading the
   literal path(s) the script passes to `sim3d.export`
   ([`services/sim3d.rs`](../crates/core/src/services/sim3d.rs)); a call with no
   path uses the default `sim3d_scene.html`.
3. After the run, `runner.rs` resolves each path against the run directory, and if
   the file was freshly written, sets `MainViewModel::last_scene3d`. A subscription
   in `main.rs` opens it via [`scene3d_window`](../crates/app/src/scene3d_window.rs)
   — the same embedded WebKitGTK viewer the flowchart 3-D Scene action uses.

This mirrors the `VideoWriter` → `last_video` → `video_view` flow. Computed export
paths and the interactive REPL path are not auto-detected (a literal path in a
Run-ed file is); a future compiler-side `___MF_SCENE3D___ path=…` sentinel (like
`VideoWriter`'s `___MF_VID___`) would cover those too.

## REPL (`matlabc -repl`)

The live REPL is wired end-to-end. `app/src/process.rs::ReplSession` spawns
`matlabc -repl`, reads its stdout/stderr on background threads, and marshals each
line to the GTK main loop. Submitting a command also sends the workspace-sync
probe (`disp('___MF_WS_BEGIN___'); whos; disp('___MF_WS_END___')`). Output is
routed through the [`SentinelRouter`](../crates/core/src/services/sentinels.rs),
which separates console text from structured payloads wrapped in `___MF_WS___` /
`___MF_VAL___` / `___MF_FIG___` sentinels — so typing a command updates the
console **and** the Workspace table automatically.

## Debug (`matlabc -dap`)

DAP speaks JSON-RPC bodies in `Content-Length` frames over stdio. The pure framing
codec, sequence/request builder, and message parser live in
[`dap.rs`](../crates/core/src/services/dap.rs); the
[`DebugViewModel`](../crates/core/src/viewmodels/debug.rs) is the client-side state
machine (idle → launching → running → paused → terminated). The transport
(`app/src/process.rs::DapSession`) spawns `matlabc -dap`, de-frames responses, and
`app/src/app_state.rs` drives the protocol: `initialize → launch → setBreakpoints
→ configurationDone`, then on `stopped` fetches `stackTrace → scopes → variables`
to populate the call stack, locals, and the editor's execution-line marker.
Stepping (continue/pause/next/stepIn/stepOut/stepBack) and gutter-click
breakpoints are all wired.

> **Compiler-side blocker:** the shipped `matlabc -dap` currently **segfaults**
> before sending a `stopped` event (verified with a standalone JSON-RPC driver,
> not just the IDE), so pausing/locals can't be exercised yet. The IDE handles
> the adapter exiting gracefully (`DAP_EXIT` → tear down + status message). Once
> the adapter is fixed the existing client + UI work without changes.
