## ADDED Requirements

### Requirement: Author conditional, hit-count, and log-point breakpoints

The IDE SHALL let the user author a breakpoint's condition, hit count, and log message. Right-clicking the editor gutter at a line SHALL open a breakpoint editor with Condition, Log message, and Hit count fields pre-filled from that line's current breakpoint; applying SHALL store the values (creating the breakpoint if absent) and SHALL re-send `setBreakpoints` to a live session. The Breakpoints list SHALL mark conditional / log-point / hit-count breakpoints and SHALL offer the same editor.

#### Scenario: Set a condition on a line

- **WHEN** the user opens the gutter breakpoint editor and enters a condition
- **THEN** the line gets a conditional breakpoint and a live session receives the updated
  `setBreakpoints` including the `condition`

#### Scenario: Editor pre-fills existing settings

- **WHEN** the user opens the editor on a line that already has a log message or hit count
- **THEN** the editor shows the current values for editing

#### Scenario: Breakpoints list marks special breakpoints

- **WHEN** a breakpoint has a condition, hit count, or log message
- **THEN** the Breakpoints list visually distinguishes it from a plain breakpoint

### Requirement: Edit a local variable while paused

The IDE SHALL let the user edit a local variable's value while the debugger is paused. Committing a new value for a Locals row SHALL send a DAP `setVariable` using the current frame's Locals scope reference, and on success the IDE SHALL refresh the frame's variables so the Locals panel and the mirrored Workspace table show the new value.

#### Scenario: Set a local to a new value

- **WHEN** the user edits a Locals row's value and commits it while paused
- **THEN** the IDE sends `setVariable` for that variable and refreshes the Locals from the adapter

#### Scenario: Edit only while paused

- **WHEN** there is no paused debug session
- **THEN** committing a Locals edit has no effect (no `setVariable` is sent)
