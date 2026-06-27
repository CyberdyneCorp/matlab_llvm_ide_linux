## Why

The `matlabc -dap` MATLAB script debugger is now functional. The IDE's docs still flagged it as
blocked by a compiler segfault before the first `stopped` event; that is resolved — the compiler's
own DAP scenario suite (`test/Debug/run_dap_tests.py`) passes **68/68** against the binary the IDE
uses by default (`/home/leonardo/work/matlab_llvm/build/matlabc`).

The compiler also added **exception breakpoints + `exceptionInfo`** (matlab_llvm #404/#405): an
uncaught `error()` can pause the run with `stopped reason="exception"`, and `exceptionInfo` returns
the `MException` identifier + message. The IDE already sends `setExceptionBreakpoints` (a Debug-panel
"error" filter toggle) but **ignored the `stopped.reason`** and never requested `exceptionInfo`, so
the error that stopped the run was never shown.

## What Changes

- Update the docs: remove the stale "segfault blocker" note in `docs/compiler_integration.md` and
  record the verified working state + the exception flow.
- Handle `stopped` with `reason == "exception"`: request `exceptionInfo`; on any other reason, clear
  the prior exception.
- Parse the `exceptionInfo` response (`exceptionId`, `description`, `details.message`,
  `details.stackTrace`) into a new `DapException` model and store it on the `DebugViewModel`
  (`last_exception`), cleared on resume and terminate.
- Surface the exception: log it to the console (error level), show it in the status bar, and display
  a banner at the top of the Debug panel (the MATLAB identifier + message). Hidden when there is no
  active exception.

## Capabilities

### Added Capabilities
- `script-debugger`: Pausing on uncaught MATLAB errors via the DAP `error` exception filter and
  surfacing the exception identifier/message in the IDE.

## Impact

- **Modified code**:
  - `crates/core/src/models/debug.rs` — `DapException` model (+ `mod.rs` re-export).
  - `crates/core/src/viewmodels/debug.rs` — `last_exception` property, `set_exception`, cleared in
    `on_running`/`terminate`.
  - `crates/app/src/app_state.rs` — `stopped` reason handling, `exceptionInfo` request +
    `parse_exception_info`, console/status surfacing.
  - `crates/app/src/ui.rs` — Debug-panel exception banner bound to `last_exception`.
  - `docs/compiler_integration.md` — status + exception-flow docs.
- **Compiler dependency**: a `matlabc -dap` that advertises the `error` exception filter and
  implements `exceptionInfo` (matlab_llvm #404/#405). Older adapters simply never emit
  `reason="exception"`, so the path is dormant.
- **No protocol regressions**: the normal breakpoint/step flow is unchanged; the only added requests
  fire on an exception stop.
- **Testing**: view-model test (exception set then cleared on resume/terminate) and pure parser
  tests (`details.message` preferred, `description` fallback). The end-to-end adapter behavior is
  covered by the compiler's 68/68 scenario suite (incl. `scn_exception_info_and_filter`).
