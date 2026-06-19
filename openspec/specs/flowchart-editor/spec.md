# flowchart-editor Specification

## Purpose

The shared block-diagram editing surface and the control-flow (`control_flow`)
`.mflow` dialect: a block palette, a Cairo canvas (select / drag / zoom / connect),
and a property inspector, all driven by the tested `FlowchartViewModel`. Control
charts lower to MATLAB via `matlabc -emit-matlab`. The signal-flow (mflowLink)
and state-chart (mStateflow) editors specialize this surface.

## Requirements

### Requirement: Diagram authoring

The editor SHALL build a diagram on the canvas: drop a dialect-appropriate block
from the palette, drag a block to move it, scroll to zoom, fit-to-content, select
a block (or clear selection on empty canvas), delete the selection, undo/redo
edits, and save the document back to its `.mflow`. Mutations target the
currently-navigated flow.

#### Scenario: Drop a block from the palette

- **WHEN** the user clicks a palette block
- **THEN** a node of that kind is added to the current flow and selected

#### Scenario: Move is undoable

- **WHEN** the user drags a block and then undoes
- **THEN** the block returns to its previous position

#### Scenario: Fit frames the diagram

- **WHEN** the user invokes Fit (and on open)
- **THEN** the viewport pans/zooms so all nodes are visible

### Requirement: Block inspector

The editor SHALL show a property inspector for the selected block that edits the
fields meaningful for its kind (label plus, per kind, assignment target/
expression, `if`/`while` condition, `for` loop variable/iterable, etc.) and
commits edits into the node.

#### Scenario: Editing a field updates the node and marks dirty

- **WHEN** the user edits an inspector field of the selected block
- **THEN** the node's field is updated and the document is marked dirty

### Requirement: Compile to MATLAB

The editor SHALL lower the chart to MATLAB via `matlabc -emit-matlab`, write the
generated `.m` beside the model, and open it in an editor tab.

#### Scenario: Compile generates and opens the source

- **WHEN** the user runs **Compile** with `matlabc` available
- **THEN** the generated `.m` is written next to the `.mflow` and opened

### Requirement: Structural execution step and breakpoints

The editor SHALL provide a structural execution-order walk that highlights each
block in order (depth-first from Start, no value evaluation), and SHALL toggle
per-block breakpoints on executable blocks only.

#### Scenario: Step highlights the next block in order

- **WHEN** the user presses **Step**
- **THEN** the next block in execution order is highlighted and selected

#### Scenario: Breakpoints apply only to executable blocks

- **WHEN** the user toggles a breakpoint on a non-executable block
- **THEN** no breakpoint is set and the action is a no-op
