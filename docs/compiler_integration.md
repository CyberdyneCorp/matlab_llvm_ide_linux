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

Because the IDE does not yet type the `signal_*3d` block kinds, they load as
`NodeKind::Unknown` with the original `kind` tag preserved on the `FlowNode`
(`raw_kind`), so 3-D models authored elsewhere round-trip through the editor
without losing blocks or parameters.

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
