## Why

The `matlabc -dap` debugger is functional (68/68 scenarios) and supports **conditional / hit-count /
log-point breakpoints** and **`setVariable`** (editing a local while paused). The IDE already *sends*
the `condition` / `logMessage` / `hitCondition` fields on `setBreakpoints`, but there is no UI to
author them — every breakpoint is plain. And the Locals panel is read-only, even though the adapter
can set a local's value.

This change adds the two missing authoring surfaces.

## What Changes

- **Breakpoint authoring**:
  - Right-click the editor gutter at a line to open a Breakpoint editor popover with Condition,
    Log message, and Hit count fields (pre-filled from the line's current config). Apply updates the
    breakpoint (creating it if absent); Remove clears it.
  - The Debug panel's Breakpoints list shows a marker for conditional / log-point / hit-count
    breakpoints and the condition text, with an Edit button that opens the same popover.
  - Editing re-sends `setBreakpoints` to a live session so the change takes effect immediately.
- **Edit a local (`setVariable`)**:
  - Each Locals row becomes editable: the value is an entry; pressing Enter sends `setVariable`
    for that local using the current frame's Locals scope reference.
  - On the response the IDE re-reads the frame's variables so the panel (and the mirrored Workspace
    table) reflect the new value.

## Capabilities

### Added Capabilities
- `script-debugger`: Authoring conditional / hit-count / log-point breakpoints, and editing a local
  variable's value while paused.

## Impact

- **Modified code**:
  - `crates/core/src/models/editor.rs` — `EditorTab::set_breakpoint_config(line, cfg)`.
  - `crates/core/src/viewmodels/editor.rs` — `set_breakpoint_config(id, line, cfg)`.
  - `crates/app/src/app_state.rs` — store the Locals scope reference; `set_debug_variable`;
    `setVariable` response handler (re-fetch variables).
  - `crates/app/src/editor_view.rs` — gutter right-click → breakpoint editor popover.
  - `crates/app/src/ui.rs` — breakpoint-editor popover helper; Breakpoints-list markers + Edit;
    editable Locals rows.
- **Compiler dependency**: a `matlabc -dap` that honors `condition`/`hitCondition`/`logMessage` and
  implements `setVariable` (already shipped; verified by the compiler's DAP suite). Older adapters
  ignore the extra breakpoint fields and would reject `setVariable` (handled gracefully).
- **Testing**: model/VM tests for `set_breakpoint_config`; the protocol round-trips are covered by
  the compiler's `scn_conditional_breakpoint` / `scn_hit_count_breakpoint` / `scn_log_point` /
  `scn_set_variable` scenarios.
