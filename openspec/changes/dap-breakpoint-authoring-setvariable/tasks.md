## 1. Core: breakpoint config model + VM

- [x] 1.1 `EditorTab::set_breakpoint_config(line, cfg)` — insert/replace the config at `line`
  (creating the breakpoint).
- [x] 1.2 `EditorViewModel::set_breakpoint_config(id, line, cfg)`.
- [x] 1.3 Tests: setting a condition on a line creates a conditional breakpoint; clearing fields
  leaves a plain breakpoint.

## 2. App: breakpoint authoring UI

- [x] 2.1 A shared `open_breakpoint_editor(app, parent, rect, tab_id, line)` popover with Condition /
  Log message / Hit count entries (pre-filled), Apply + Remove.
- [x] 2.2 Gutter right-click (secondary button) → resolve line → open the editor popover.
- [x] 2.3 Breakpoints list rows: marker for conditional/log/hit + condition text; an Edit button
  opening the popover. Apply re-sends via `refresh_breakpoints`.

## 3. App: setVariable

- [x] 3.1 Store the Locals scope `variablesReference` (`dbg_locals_ref`) when handling `scopes`.
- [x] 3.2 `set_debug_variable(name, value)` → send `setVariable` with the stored ref.
- [x] 3.3 Handle the `setVariable` response → re-send `variables` to refresh locals + Workspace
  mirror.
- [x] 3.4 Editable Locals rows (value entry, Enter commits); type hint as tooltip.

## 4. Build & docs

- [x] 4.1 `cargo test` (core + app) green; clippy clean.
- [x] 4.2 `docs/compiler_integration.md` debugger section: note breakpoint authoring + setVariable.
- [x] 4.3 `openspec validate dap-breakpoint-authoring-setvariable --strict`.
