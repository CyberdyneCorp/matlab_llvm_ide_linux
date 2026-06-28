## Why

The matlab_llvm compiler now raises **MATLAB-style runtime errors in interpret mode** (`-repl`,
PRs #423–#430) instead of silently returning `0`/empty:

- `Unrecognized function or variable 'foo'.` (undefined name / undefined callee)
- `Index exceeds the number of array elements. Index must not exceed N.`
- `Array indices must be positive integers or logical values.`
- `Index in position K exceeds array bounds. Index must not exceed N.`
- `Index exceeds the number of elements in the cell array. …`
- `Arrays have incompatible sizes for this operation.` (binop / element-wise / assignment / concat)

These are printed to stderr as clang-style diagnostics, e.g. `error: Index exceeds …` or, for an
undefined name, `<repl:0>:1:5: error: Unrecognized function or variable 'foo'.` followed by a
source-echo + caret. The IDE captures stderr and already colors any line containing "error" red, so
the errors *appear* — but with a clang-style `error:` prefix and a `<repl:0>:line:col:` position
that MATLAB's command window never shows. The substring classifier is also fragile (it matches an
ordinary line that merely contains the word "error", and would miss a message that didn't).

This change makes interpret-mode errors read like MATLAB.

## What Changes

- Recognize a compiler/runtime diagnostic line in the REPL transcript: an `error:` / `warning:`
  token, optionally preceded by a `<loc>:line:col:` position, classifies the line as Error /
  Warning and strips that clang-style prefix so the displayed message is the bare MATLAB text
  (e.g. `Unrecognized function or variable 'foo'.`), shown in the Error color.
- Ordinary output is unchanged (Plain). The source-echo/caret context lines that follow an undefined
  -name diagnostic are left as-is.
- This covers both the live `-repl` path and compiled run-execution output (both feed the REPL
  transcript). Compile-time build diagnostics (the Problems pane) keep their `file:line:col`
  location and are untouched.

## Capabilities

### Added Capabilities
- `repl-console`: MATLAB-style presentation of interpret-mode runtime errors in the REPL transcript.

## Impact

- **Modified code**: `crates/core/src/viewmodels/repl.rs` — replace the substring `classify` with a
  diagnostic-aware `matlab_style_line` (severity + prefix-stripped message); `feed_line` uses it.
- **Compiler dependency**: a `matlabc` whose interpreter raises these errors (#423–#430). Older
  binaries silently return wrong values — unaffected by this presentation change.
- **No behavior change** for non-error output; purely how diagnostics are classified and shown.
- **Testing**: unit tests over the exact strings the compiler emits (verified live against the
  binary), incl. the located undefined-name form, the bare runtime-raise forms, and false-positive
  guards (an ordinary line mentioning "error").
