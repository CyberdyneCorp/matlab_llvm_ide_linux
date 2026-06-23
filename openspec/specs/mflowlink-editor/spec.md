# mflowlink-editor Specification

## Purpose

The signal-flow (mflowLink) block-diagram editor: authoring, validating, and
analyzing a `.mflow` signal diagram that lowers to MATLAB and simulates via
`matlabc`. Covers block-parameter validation, edit-time diagnostics, port-to-port
wiring, solver configuration, the control-block palette (incl. PID and MPC),
linear analysis, subsystem nesting/navigation, reusable library/masked blocks,
the MATLAB Function block's source editor, and per-wire signal breakpoints. The
editor's block vocabulary tracks the blocks the simulator implements.

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

#### Scenario: Multiple ports on one face are spread, not collapsed

- **WHEN** a block has several ports on the same face (e.g. an Integrator's
  `in`/`reset`/`init`, or a MATLAB Function block's `u1..uN` inputs)
- **THEN** the ports render at distinct points down that face rather than
  overlapping at one point

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

#### Scenario: Fixed-step odeN algorithms decode

- **WHEN** a model declares a Simulink fixed-step solver (`ode1`/`ode2`/`ode3`/
  `ode4`/`ode5`/`ode8`) or a variable-step / stiff solver (`ode113`, `ode23s`,
  `ode23t`, `ode23tb`), as the compiler examples do
- **THEN** the document decodes and the algorithm round-trips, and the Solver
  popover lists it

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

### Requirement: MATLAB Function block source and ports

The editor SHALL let the user view and edit a MATLAB Function block's source:
double-clicking the block opens a MATLAB-highlighted editor on its
`function … = name(u1, …) … end` body (seeded from the single-line `expression`
when no body exists). The block's input ports SHALL follow the function
signature — `u1..uN` for the arity of the header (or the highest `uN` referenced
in an expression) plus a single `out` — and edits to the body or expression
SHALL re-derive the ports, dropping wires to ports that disappear.

The source editor SHALL provide code-editor affordances: a line-number gutter,
a visible edit toolbar (undo / redo / cut / copy / paste backed by the text
view's built-in actions), a find bar (Ctrl+F), current-line highlighting, and
auto-indent that preserves the current line's leading whitespace on Enter.

#### Scenario: Double-click opens the source

- **WHEN** the user double-clicks a MATLAB Function block
- **THEN** an editor opens showing its function source (or one seeded from the
  block's expression), with a line-number gutter and an edit toolbar

#### Scenario: Ports follow the signature

- **WHEN** the user changes the function to take three inputs
- **THEN** the block exposes `u1`, `u2`, `u3` inputs and a single `out` output

#### Scenario: Shrinking the signature prunes wires

- **WHEN** the function arity is reduced and an input port disappears
- **THEN** the wire that targeted the removed port is dropped

#### Scenario: Find selects a match

- **WHEN** the user opens the find bar and enters a term that occurs in the body
- **THEN** the next matching occurrence is selected and scrolled into view

### Requirement: MATLAB Function block breakpoints

The MATLAB Function source editor SHALL support two breakpoint kinds. A
**source-line breakpoint** toggles by clicking its gutter line; it persists on
the node's `breakpoint_lines` param (round-tripping through `.mflow`), draws a
gutter marker, and is honored by the live simulation — the run pauses when the
body reaches that line and the IDE surfaces the body's locals (matlab_llvm
#354/#384/#385). A **break on output** control SHALL set or clear a signal
breakpoint on every wire leaving the block's output — and is disabled until the
output is wired.

#### Scenario: Toggle a source-line breakpoint

- **WHEN** the user clicks a line in the editor gutter
- **THEN** a breakpoint marker is drawn on that line and the line is recorded in
  the node's `breakpoint_lines`, persisting through save/reload

#### Scenario: A live run pauses on a source-line breakpoint with locals

- **WHEN** a model with a MATLAB Function block carrying a `breakpoint_lines`
  line is simulated live and the body reaches that line
- **THEN** the run pauses at `<block>:<line>` and the simulation window's Locals
  panel lists the body's variables (fetched via the DAP scopes/variables round-trip)

#### Scenario: Break on output sets the wire breakpoint

- **WHEN** the user enables "Break on output" with a condition and the block's
  output is wired
- **THEN** the condition is installed as a signal breakpoint on each output wire
  (cleared when disabled)

#### Scenario: Break on output needs a wired output

- **WHEN** the block's output is not connected to any wire
- **THEN** the "Break on output" control is disabled

### Requirement: Editor block library tracks the simulator

The editor's signal-flow block library SHALL offer every block kind the
simulator implements, including the MPC Controller (`signal_mpc_move`), From
Workspace, n-D Lookup Table, If / Switch-Case Action subsystems, the custom
block, and the toolbox families the simulator added under compiler issue #343 —
Communications (`signal_awgn`, PSK/QAM mod·demod, `signal_error_rate`), DSP &
image (`signal_fft`/`ifft`/`window`/`spectrum`/`biquad`/`lowpass`/`highpass`/
`dcblock`/`dwt`/`idwt`/`image_filter`/`color_space`/`threshold`), HDL sequential
(`signal_dff`/`tff`/`counter`/`jkff`/`srff`/`shift_register`/`ram`/`rom`), and
estimation / ML / control (`signal_kalman`, `signal_lqr`, `signal_running_stats`,
`signal_dnn_predict`, `signal_rl_agent`, `signal_rf_2port`,
`signal_pose_transform`, `signal_repeating_sequence`, `signal_image_source`) — so
models that use them open and are authorable in the editor. Each block SHALL
carry the ports the simulator reads (e.g. `signal_error_rate` `tx`/`rx`,
`signal_kalman` `z`/`u`, the HDL registers' `clk`/`reset`, `signal_ram`
`addr`/`data`/`we`/`clk`) and expose its parameters in the inspector.

The library SHALL group these into the **Communications**, **DSP & Image**, and
**HDL** palette sections alongside the existing Sources / Continuous / Discrete /
Math / Routing / Lookup / Sinks / Composite sections.

#### Scenario: A simulator block is placeable

- **WHEN** the user opens the Library
- **THEN** the MPC Controller block (and the other simulator-supported blocks,
  including the Communications / DSP / HDL families) appear and can be dropped
  onto the canvas

#### Scenario: A model using the block opens

- **WHEN** the user opens a model containing a `signal_mpc_move` (or any #343
  block such as `signal_kalman` or `signal_fft`)
- **THEN** the model decodes and renders without an unknown-kind error

#### Scenario: A new toolbox block has its simulator ports

- **WHEN** an Error Rate (`signal_error_rate`) or HDL register block is dropped
- **THEN** it exposes the simulator's named input ports (`tx`/`rx`,
  `clk`/`reset`, …), each anchored to a block face rather than collapsed

#### Scenario: From Workspace exposes its inline time-series

- **WHEN** the user inspects a From Workspace (`signal_from_workspace`) block
- **THEN** the inspector exposes its `data` time-series (`t v; …`) and an
  `interpolation` choice (`linear` / `zoh`), and a From Workspace → To Workspace
  model round-trips through the codec and simulates

### Requirement: Per-wire signal breakpoints

The editor SHALL attach a signal-breakpoint condition (`value > 0`,
`abs(value) >= 1`) to an individual wire, persisting it on the edge in the
`.mflow`. A breakpointed wire SHALL draw a marker, and the breakpoints SHALL be
installed (keyed by the wire's source block) when the model is simulated live.

#### Scenario: Set a breakpoint on a wire

- **WHEN** the user right-clicks a wire and enters a condition
- **THEN** the condition is stored on the edge and the wire shows a breakpoint marker

#### Scenario: Breakpoint persists and round-trips

- **WHEN** the model is saved and reloaded
- **THEN** the wire still carries its breakpoint condition

#### Scenario: Clearing removes the breakpoint

- **WHEN** the user clears the condition (or empties it)
- **THEN** the wire's breakpoint and its marker are removed

