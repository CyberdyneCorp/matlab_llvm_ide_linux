## Why

If the `matlabc -repl` process exits or crashes mid-session, the IDE does not notice. The
stdout/stderr reader threads hit EOF and break silently; the `ReplSession` is left in place, the REPL
view model still reports `is_running = true`, and the user gets no notification. Every subsequent
command then writes to a dead pipe and only logs "REPL write failed" — with no recovery. (The DAP
path already handles this via a synthetic `DAP_EXIT` line; the REPL path has no equivalent.)

This was surfaced while verifying the compiler's new interpret-mode errors: a sequence of
error-raising statements can crash `matlabc -repl`, after which the IDE's REPL silently stops
working. The fix is independent of that compiler bug — any REPL exit (crash, `exit`, kill) should be
handled gracefully.

## What Changes

- Emit a synthetic `REPL_EXIT` line when the `matlabc -repl` process's pipes close (both reader
  threads finished), mirroring the existing `DAP_EXIT` mechanism.
- On `REPL_EXIT`, the IDE clears the dead session, sets the REPL view model to not-running, and posts
  a transcript note ("REPL process ended — it will restart on the next command.").
- Because the session is cleared, the next command transparently starts a fresh `matlabc -repl`
  (the existing `ensure_repl` lazy-start path).

## Capabilities

### Modified Capabilities
- `repl-console`: The REPL detects its backing process exiting/crashing, notifies the user, and
  recovers on the next command instead of going silently dead.

## Impact

- **Modified code**:
  - `crates/app/src/process.rs` — `REPL_EXIT` constant; a joiner thread that sends it once both REPL
    readers reach EOF.
  - `crates/app/src/app_state.rs` — the REPL `on_line` closure intercepts `REPL_EXIT` and clears the
    session.
  - `crates/core/src/viewmodels/repl.rs` — `on_process_exit()` (set not-running + transcript note).
- **No compiler dependency**: purely IDE-side robustness; applies to any REPL exit.
- **Testing**: a view-model unit test for `on_process_exit` (running flips to false; a note is added).
  The process-level EOF→`REPL_EXIT` plumbing mirrors the tested `DAP_EXIT` path.
