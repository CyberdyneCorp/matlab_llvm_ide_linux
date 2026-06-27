## 1. Unblock + validate

- [x] 1.1 Verify `matlabc -dap` reaches `stopped`/locals against the IDE's default binary
  (`test/Debug/run_dap_tests.py` → 68/68).
- [x] 1.2 Remove the stale "segfault blocker" note in `docs/compiler_integration.md`; document the
  verified working state.

## 2. Exception model + view model

- [x] 2.1 Add `DapException { exception_id, message, stack_trace }` in
  `crates/core/src/models/debug.rs` (+ `mod.rs` re-export).
- [x] 2.2 Add `last_exception: Property<Option<DapException>>` + `set_exception` to
  `DebugViewModel`; clear it in `on_running` and `terminate`.
- [x] 2.3 Test: exception set, then cleared on resume and on terminate.

## 3. App: protocol + display

- [x] 3.1 In the `stopped` handler, request `exceptionInfo` when `reason == "exception"`; clear the
  prior exception otherwise.
- [x] 3.2 Add `parse_exception_info` (prefers `details.message`, falls back to `description`) and an
  `exceptionInfo` response handler that logs to console + status bar + `set_exception`.
- [x] 3.3 Tests for `parse_exception_info` (details vs description).
- [x] 3.4 Debug-panel exception banner bound to `last_exception` (red, hidden when none).

## 4. Build & docs

- [x] 4.1 `cargo test` (core 490+7, app 21) green; clippy clean.
- [x] 4.2 `docs/compiler_integration.md` exception-flow section.
- [x] 4.3 `openspec validate dap-exception-breakpoints --strict`.
