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
a block **or a wire** (or clear selection on empty canvas), delete the selection
(block or wire), undo/redo edits, and save the document back to its `.mflow`.
Mutations target the currently-navigated flow. New nodes and wires SHALL receive
ids that do not collide with any node, edge, or flow id already in the loaded
model.

#### Scenario: Drop a block from the palette

- **WHEN** the user clicks a palette block
- **THEN** a node of that kind is added to the current flow and selected

#### Scenario: Move is undoable

- **WHEN** the user drags a block and then undoes
- **THEN** the block returns to its previous position

#### Scenario: Fit frames the diagram

- **WHEN** the user invokes Fit (and on open)
- **THEN** the viewport pans/zooms so all nodes are visible

#### Scenario: Select and delete a wire

- **WHEN** the user clicks a connection (no node under the cursor) and presses Delete
- **THEN** the wire is selected (drawn highlighted) and then removed

#### Scenario: Added ids never duplicate a loaded id

- **WHEN** the user adds a wire to a model loaded with edges already named `e1`/`e2`/`e3`
- **THEN** the new wire receives a fresh, non-colliding id so the model still compiles

### Requirement: Orthogonal wire routing

The editor SHALL route each wire as an orthogonal polyline that leaves the source
and enters the target along their port normals and avoids passing through node
bodies. A net that fans out from one output to several inputs SHALL draw a
junction dot at its branch point, and wires of different nets SHALL NOT share a
collinear run. Clicking a wire selects it along its routed path.

#### Scenario: A wire routes around a blocking node

- **WHEN** a node sits between a wire's source and target ports
- **THEN** the routed wire detours around the node instead of crossing it

#### Scenario: Fan-out shows a junction

- **WHEN** one output port feeds two or more input ports
- **THEN** a junction dot is drawn where the net's branches part ways

#### Scenario: Unrelated signals do not overlap

- **WHEN** two distinct nets both detour through the same region
- **THEN** their wires take separate lanes and share no collinear segment

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
