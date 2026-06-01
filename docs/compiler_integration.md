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

## REPL (`matlabc -repl`)

The REPL view model records history/transcript and routes stdout through the
[`SentinelRouter`](../crates/core/src/services/sentinels.rs), which separates
console text from structured payloads wrapped in `___MF_WS___` / `___MF_VAL___` /
`___MF_FIG___` sentinels (workspace tables, value matrices, rendered figures).
The live subprocess streaming is wired in a later phase; the routing + parsing are
complete and tested.

## Debug (`matlabc -dap`)

DAP speaks JSON-RPC bodies in `Content-Length` frames over stdio. The pure framing
codec, sequence/request builder, and message parser live in
[`dap.rs`](../crates/core/src/services/dap.rs); the
[`DebugViewModel`](../crates/core/src/viewmodels/debug.rs) is the client-side state
machine (idle → launching → running → paused → terminated) driven by decoded
events. The transport process wiring is a later phase.
