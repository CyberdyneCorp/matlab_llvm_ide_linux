## 1. Core: sim-DAP requests

- [x] 1.1 Add `SimRequest::StepStatement` → DAP `next` (with `threadId: 1`) in
  `crates/core/src/services/sim_dap.rs`.
- [x] 1.2 Add `SimRequest::StepOut` → DAP `stepOut` (with `threadId: 1`).
- [x] 1.3 Tests (`statement_step_uses_standard_dap_verbs`): both frames carry the right command
  + `threadId`.

## 2. Core: viewmodel

- [x] 2.1 Broaden the source-line-stop detection in `on_sim_event` to
  `reason == "breakpoint" || reason == "step"`.
- [x] 2.2 Add `live_step_request() -> SimRequest` (StepStatement when `source_stop` is set, else
  StepMajor).
- [x] 2.3 Add `can_step_out() -> bool` (= `source_stop.is_some()`).
- [x] 2.4 Test (`statement_step_tracks_source_line_and_locals`): a `step` stop with a
  `<block>:<line>` description sets `source_stop`; `"function returned"` clears it +locals;
  `live_step_request()` / `can_step_out()` reflect the state.

## 3. App: transport buttons

- [x] 3.1 Route the Step button for live sessions via `vm.live_step_request()`.
- [x] 3.2 Add a Step Out button (`⤴ Step Out`); bind sensitivity to `vm.can_step_out()` via
  `source_stop.bind`; send `SimRequest::StepOut` on click.
- [x] 3.3 Update tooltips (Step: statement step inside a MATLAB Function, else one major step;
  Step Out: finish the current body).

## 4. Build & docs

- [x] 4.1 `cargo test` (core 485+7, app 19) green; clippy clean.
- [x] 4.2 Update `docs/roadmap.md` (source-line breakpoints entry now covers statement stepping +
  Step Out). `docs/compiler_integration.md` has no sim-DAP section, so the roadmap is the living
  reference.
- [x] 4.3 `openspec validate mflowlink-fcn-statement-stepping --strict` passes.
