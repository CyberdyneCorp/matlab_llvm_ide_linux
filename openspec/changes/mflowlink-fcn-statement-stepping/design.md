## Context

The IDE drives the compiler's mflowLink sim-DAP server (`matlabc -simulate --sim-dap`). The client
model lives in `crates/core/src/services/sim_dap.rs` (`SimRequest` → DAP command, `SimEvent` ←
events); the transport state lives in `crates/core/src/viewmodels/mflowlink.rs`; the GTK transport
bar lives in `crates/app/src/mflowlink_window.rs`.

The compiler's statement-stepping surface (PRs #386/#387, verified against
`test/Flowchart/SimulateDap/run_source_breakpoints.py`):

- Stepping commands are the **standard DAP verbs** `next` / `stepIn` / `stepOut`, each with
  `{ "threadId": 1 }`. (`stepMajor` is a separate custom verb that *always* does a major step.)
- `next`/`stepIn` do a statement replay **only while the run is paused inside a function body**
  (`isFcnStepping()`); otherwise they fall through to one major step.
- A statement step emits `stopped { reason: "step", description: "<blockId>:<line>" }`. When the
  body is exhausted (or `stepOut` is sent) it emits `stopped { reason: "step", description:
  "function returned" }`.
- The first hit on a body breakpoint is `stopped { reason: "breakpoint", description:
  "<blockId>:<line>" }`. Locals are the `Locals` scope (`variablesReference` from `scopes`),
  populated only while stopped in a body.

The IDE already parses `"<blockId>:<line>"` (`parse_source_loc`) and fetches Locals on a source
stop — but only for `reason == "breakpoint"`, and the Step button only sends `stepMajor`.

## Goals / Non-Goals

**Goals**
- Step one statement at a time through a MATLAB Function body from the transport.
- Step out of the body.
- Keep the execution-line marker and Locals in sync on every statement step.
- Preserve existing major/block stepping behavior outside a function body.

**Non-Goals**
- No descent into nested function calls (the compiler does not yet expose step-in targets; `stepIn`
  behaves like `next` for a flat body, so the IDE exposes a single Step verb, not a separate
  Step Into).
- No step-*back* at statement granularity (compiler exposes step-back only at major/block level).
- No compiler-side changes.

## Decisions

### Route the Step button in the viewmodel, not the GTK layer
Add `MflowLinkViewModel::live_step_request() -> SimRequest`: returns `StepStatement` when
`source_stop` is set (paused inside a body), else `StepMajor`. This mirrors the compiler's own
routing of `next`, but doing it in the IDE keeps it explicit and unit-testable (GTK click handlers
are not). The Step button calls this for live sessions; CSV replay is unchanged.

Alternative considered: always send DAP `next` and let the compiler route. Rejected — it couples the
button's meaning to compiler internals and loses an explicit "advance one major step" verb; the
IDE already tracks `source_stop`, so routing here is trivial and testable.

### Broaden source-stop detection to `breakpoint | step`
`on_sim_event` currently records a source stop only for `reason == "breakpoint"`. The compiler uses
`reason == "step"` for statement steps, so the gate becomes `reason == "breakpoint" || reason ==
"step"`. Detection still hinges on the description parsing to a `<block>:<line>` pair, so non-source
stops (`"function returned"`, `"stopTime reached"`, signal-breakpoint descriptions, major steps with
no description) clear the source stop exactly as before. This naturally makes Locals re-fetch fire
after each statement step (the existing `source_stop.is_some()` → `scopes` path).

### Step Out button gated on body state
Add `can_step_out() -> bool` (= `source_stop.is_some()`) and a transport button bound to it
(insensitive otherwise), sending `StepOut`. Keeps the toolbar honest: Step Out only appears
actionable while inside a body.

## Risks / Trade-offs

- **Compiler-version coupling**: the new verbs require a sim-DAP that handles function-body
  `next`/`stepOut`. They are only sent while stopped inside a body (a state only reachable with a
  compiler that emits `<block>:<line>` source stops), so an older binary degrades gracefully —
  the Step button keeps doing major steps.
- **Description-shape reliance**: source-stop detection depends on the `"<block>:<line>"` shape.
  This is already the contract (`parse_source_loc`); the change only widens the `reason` set, and
  `parse_source_loc` remains strict (rejects descriptions without a trailing `:<positive-int>`).

## Migration

None. Existing models and major/block stepping are unaffected.
