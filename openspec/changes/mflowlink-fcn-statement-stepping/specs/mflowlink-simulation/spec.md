## ADDED Requirements

### Requirement: Statement stepping inside MATLAB Function blocks

The live (`--sim-dap`) transport SHALL support statement-level stepping inside MATLAB Function blocks. While the simulation is paused inside a function body (a source-line stop is active), the Step action SHALL advance exactly one statement (DAP `next`); otherwise the Step action SHALL advance one major (solver) step. A Step Out action SHALL finish the current body (DAP `stepOut`) and SHALL be available only while paused inside a body.

#### Scenario: Step advances one statement inside a body

- **WHEN** the simulation is paused at a MATLAB Function source-line breakpoint and the user invokes
  Step
- **THEN** the simulation advances to the next statement of the body and pauses there

#### Scenario: Step advances a major step outside a body

- **WHEN** the simulation is paused but not inside a MATLAB Function body and the user invokes Step
- **THEN** the simulation advances one major (solver) step

#### Scenario: Step Out finishes the body

- **WHEN** the simulation is paused inside a MATLAB Function body and the user invokes Step Out
- **THEN** the body runs to completion and the simulation pauses with no source-line stop active

#### Scenario: Step Out is unavailable outside a body

- **WHEN** the simulation is not paused inside a MATLAB Function body
- **THEN** the Step Out action is disabled

### Requirement: Execution marker and Locals follow statement steps

The editor SHALL keep the active source-line marker and the body Locals in sync with each statement step. When a statement step completes (`stopped` with `reason == "step"` and a `"<blockId>:<line>"` description), the editor SHALL move the marker to the new line and refresh the Locals for that line. When the body returns (`description == "function returned"`) or any non-source stop occurs, the editor SHALL clear the source-line marker and Locals.

#### Scenario: Locals refresh after a statement step

- **WHEN** the user steps from one body line to the next
- **THEN** the execution marker moves to the new line and the Locals panel shows the variables as of
  that line

#### Scenario: Marker clears when the body returns

- **WHEN** the body returns after the final statement (or Step Out)
- **THEN** the source-line marker and Locals are cleared and the transport returns to major-step
  granularity
