## Why

The matlab_llvm compiler (PRs #386/#387) added **statement-level stepping** inside MATLAB
Function blocks during mflowLink simulation. Over the sim-DAP server, when the run is paused at a
function-body source-line breakpoint, the standard DAP `next`/`stepIn` commands replay the body one
statement at a time (emitting `stopped { reason: "step", description: "<blockId>:<line>" }` and
refreshed Locals), and `stepOut` finishes the body (`description: "function returned"`).

The IDE already wired function-body breakpoints + Locals (#85, compiler #354/#384/#385), but it
cannot *step* through the body:

- The transport's **Step** button always sends the custom `stepMajor` command, which the compiler
  always treats as a full major (solver) step — never a statement step.
- The viewmodel records a source-line stop only when `reason == "breakpoint"`. A statement step
  arrives as `reason == "step"` with a `"<blockId>:<line>"` description, so the execution-line
  marker and Locals do not advance across statement steps.
- There is no **Step Out** affordance to finish the function body.

So today a user can stop on a line and read locals once, but cannot walk the function line by line.
This change closes that gap.

## What Changes

- Add two sim-DAP requests in the client: `StepStatement` → DAP `next` and `StepOut` → DAP
  `stepOut` (both carrying `threadId: 1`, matching the compiler).
- Route the transport **Step** button by state: when paused inside a function body (a source-line
  stop is active) it sends `StepStatement`; otherwise it sends `StepMajor` as before. This logic
  lives in a unit-testable viewmodel method.
- Add a **Step Out** transport button, enabled only while paused inside a function body, that sends
  `StepOut`.
- Broaden the viewmodel's source-line-stop detection to accept both `reason == "breakpoint"` and
  `reason == "step"` (the two reasons the compiler uses for a `<blockId>:<line>` stop), so the line
  marker and Locals follow each statement step. A `"function returned"` (or any non-`<block>:<line>`)
  stop clears the source stop, returning the transport to major-step granularity.

## Capabilities

### Modified Capabilities
- `mflowlink-simulation`: The live (`--sim-dap`) transport supports statement-level stepping inside
  MATLAB Function blocks — Step advances one statement while stopped in a body, a new Step Out
  finishes the body, and the execution marker + Locals refresh on every statement step.

## Impact

- **Modified code**:
  - `crates/core/src/services/sim_dap.rs` — `SimRequest::StepStatement` (`next`) and
    `SimRequest::StepOut` (`stepOut`) + their framing tests.
  - `crates/core/src/viewmodels/mflowlink.rs` — broaden source-stop detection to `breakpoint|step`;
    add `live_step_request()` (Step routing) and `can_step_out()` helpers; tests.
  - `crates/app/src/mflowlink_window.rs` — Step button routes via `live_step_request()`; new
    Step Out button bound to `can_step_out()`; tooltips.
- **Compiler dependency**: requires a `matlabc` whose sim-DAP supports function-body `next`/`stepIn`/
  `stepOut` (PRs #386/#387). Older binaries still work for major/block stepping (the new verbs are
  only sent while stopped inside a body).
- **Testing**: unit tests for the two new request frames; viewmodel tests that a `step` stop with a
  `<block>:<line>` description sets the source stop (and `"function returned"` clears it) and that
  `live_step_request()`/`can_step_out()` reflect the body-stop state.
- **Docs**: update `docs/compiler_integration.md` (Debug/sim-DAP section) and `docs/roadmap.md`.
