# mflowlink-editor Specification

## Purpose

The signal-flow (mflowLink) block-diagram editor: authoring, validating, and
analyzing a `.mflow` signal diagram that lowers to MATLAB and simulates via
`matlabc`. Covers block-parameter validation, edit-time diagnostics, port-to-port
wiring, solver configuration, the control-block palette (incl. PID), linear
analysis, subsystem nesting/navigation, and reusable library/masked blocks.

## Requirements
### Requirement: Block parameter validation

The signal-flow block inspector SHALL validate each parameter value against the
constraint declared for its `SignalFlowParamSpec` field and SHALL NOT commit an
invalid value into the block's `params` map.

Constraints: Number (finite real), Integer (whole number with an optional
minimum), CoeffList (comma/space-separated reals), Matrix (MATLAB matrix literal
or scalar), Signs (a string of `+`/`-`), Enum (one of a fixed set), Text (free).

#### Scenario: Numeric field rejects non-numeric input
- **WHEN** the user types `abc` into a Number parameter (e.g. Gain)
- **THEN** an inline error is shown and the stored parameter is unchanged

#### Scenario: Coefficient list accepts polynomial coefficients
- **WHEN** the user types `1, 2, 3` into a CoeffList parameter (e.g. Denominator)
- **THEN** the value validates and is committed

#### Scenario: Integer count rejects fractional or below-minimum values
- **WHEN** the user types `0` into "Number of Inputs" (minimum 1)
- **THEN** an inline error is shown and the stored parameter is unchanged

#### Scenario: Clearing a field is valid
- **WHEN** the user empties a parameter field
- **THEN** no error is shown and the parameter is removed from the block

#### Scenario: Default parameter values are valid
- **WHEN** any block is created with its default parameters
- **THEN** every default value validates against its constraint

### Requirement: Algebraic-loop diagnostics

The editor SHALL identify the set of blocks lying on an algebraic loop — a cycle
of data edges in which no block on the cycle breaks direct feedthrough (e.g.
Integrator, Unit Delay, Zero-Order Hold) — and SHALL surface it to the user.

#### Scenario: Direct-feedthrough cycle is flagged
- **WHEN** a Sum feeds a Gain whose output feeds back into the Sum
- **THEN** both blocks are reported as algebraic-loop nodes and outlined on the canvas

#### Scenario: A state block breaks the loop
- **WHEN** the same cycle routes through an Integrator
- **THEN** no blocks are reported as algebraic-loop nodes

#### Scenario: Acyclic diagram has no loop
- **WHEN** the diagram has no data-edge cycle
- **THEN** the algebraic-loop set is empty

### Requirement: Port-to-port wiring

The editor SHALL connect blocks by dragging from an output port to an input
port, rendering ports as visible markers. A connection is allowed only when the
destination input port is free (each input takes a single source; outputs may
fan out) and is not a self-connection.

#### Scenario: A valid wire is committed

- **WHEN** the user drags from a free output port to a free input port of another
  block
- **THEN** a data edge is created between those ports

#### Scenario: An occupied input rejects a second wire

- **WHEN** the user drops a wire onto an input port that already has a source
- **THEN** the connection is rejected and the existing wire is unchanged

### Requirement: Solver configuration

The editor SHALL persist per-document solver settings — type (fixed/variable
step), algorithm, start/stop time, step bounds, tolerances, zero-crossing, and
algebraic-loop method — in `settings.solver`, edited through a Solver popover and
round-tripping through the `.mflow` codec.

#### Scenario: Default solver and round-trip

- **WHEN** a signal-flow document has no solver set
- **THEN** the effective solver is variable-step `ode45`; and setting a
  fixed-step Euler config with a stop time persists, re-reads, and round-trips
  through the codec

#### Scenario: Undo restores the previous solver

- **WHEN** the user changes the solver and then undoes
- **THEN** the previous solver settings are restored

### Requirement: PID controller block

The editor SHALL provide a `signal_pid` palette block whose P / I / D / N
parameters are validated in the inspector, lowering to a two-state
direct-feedthrough controller.

#### Scenario: PID block exposes validated gains

- **WHEN** a PID Controller block is added
- **THEN** its inspector exposes P/I/D/N parameters and its default values
  validate against their constraints

### Requirement: Transfer-function analysis

For a block carrying a transfer function (e.g. Transfer Fcn), the editor SHALL
compute its Bode magnitude/phase, step response, and Nyquist locus from the
block's coefficients and plot them into the Plots panel.

#### Scenario: Analyze adds Bode / step / Nyquist

- **WHEN** the user runs **Analyze** on a transfer-function block
- **THEN** Bode (magnitude + phase), step-response, and Nyquist figures are added

### Requirement: Subsystem nesting and navigation

The editor SHALL let a `signal_subsystem` block reference a sub-flow. Double-click
descends into the sub-flow with a breadcrumb (with an Up action); an
extract-to-subsystem action moves the selected blocks into a fresh sub-flow,
rerouting boundary wires through inport/outport blocks and leaving a linked
subsystem node. All editor operations target the currently-navigated flow.

#### Scenario: Double-click enters a subsystem

- **WHEN** the user double-clicks a `signal_subsystem` block
- **THEN** the editor shows that block's sub-flow with a breadcrumb to navigate
  back

#### Scenario: Extract-to-subsystem reroutes boundary wires

- **WHEN** the user extracts a selected block into a subsystem
- **THEN** the block moves to a new sub-flow, a subsystem node replaces it at the
  root, and crossing wires are rerouted through inport/outport blocks

#### Scenario: Edits target the navigated sub-flow

- **WHEN** the user is inside a sub-flow and adds or edits a block
- **THEN** the change lands in that sub-flow and is absent from the root flow

### Requirement: Library and masked blocks

The editor SHALL recognize `kind: library` flows and instantiate one as a masked
block — a subsystem node referencing the library via `library_id` whose `${name}`
mask parameters are editable in the inspector with a live `${name}` → value
expansion preview. `library_id` and `mask_params` round-trip through `.mflow`.

#### Scenario: Instantiate a library flow as a masked block

- **WHEN** the user inserts a library flow from the Library menu
- **THEN** a subsystem node linked to that library is created with its discovered
  mask parameters

#### Scenario: Mask parameters drive the expansion preview

- **WHEN** the user sets a mask parameter value
- **THEN** the inspector's preview substitutes `${name}` with that value; and the
  masked instance round-trips through the codec
</content>

