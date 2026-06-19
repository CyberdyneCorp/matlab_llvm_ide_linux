# mstateflow-editor Specification

## Purpose

The mStateflow state-chart editor: authoring states, transitions, hierarchy,
symbols, and actions in a state-chart `.mflow` document, with codegen export.
All editing goes through the tested `FlowchartViewModel`; chart structure and
lints live in the GTK-free core.

## Requirements

### Requirement: Chart symbols editor

The editor SHALL edit the chart's symbol table — data, events, and messages,
each with a name and optional scope / type / units / trigger / initial — through
a Symbols editor, persisting into `flows[0].symbols`. An all-empty table is
stored as `None` so it serializes away.

#### Scenario: Add a data symbol and round-trip

- **WHEN** the user adds a data symbol with a name and type and applies
- **THEN** the symbol is stored and survives encode → decode of the `.mflow`

#### Scenario: An empty symbol table is omitted

- **WHEN** every symbol row is cleared
- **THEN** the encoded document contains no `symbols` object

### Requirement: State action editors with lint

The editor SHALL edit a state's `entry`, `during`, and `exit` actions plus
per-event `on EVENT:` actions as multi-line, MATLAB-highlighted editors. Each
snippet SHALL be lint-checked for balanced `()`/`[]`/`{}` (string-, comment-, and
transpose-aware), surfacing an unbalanced snippet inline and as a halo on the
state in the canvas. On-event actions round-trip through a canonical
`EVENT: code` line form.

#### Scenario: Entry/during/exit/on-event are editable

- **WHEN** a state is selected
- **THEN** the inspector shows four multi-line action editors and edits commit to
  the node

#### Scenario: Unbalanced brackets are flagged

- **WHEN** an action snippet has a dropped closing paren (e.g. `y = gain * (x + 1`)
- **THEN** an inline error is shown and the state is haloed on the canvas

#### Scenario: On-event actions use the canonical form

- **WHEN** the user enters `Tick: x = x + 1` in the on-event editor
- **THEN** it is stored under event `Tick` and re-renders as the canonical
  `EVENT: code` line

### Requirement: Hierarchy authoring

The editor SHALL support nested compound states: drag-to-reparent (rejecting a
drop into the state's own descendant — no cycles), autosizing a compound to wrap
its children, OR/AND decomposition with execution-order badges on AND children,
and a history-junction toggle. It SHALL lint a history junction on an AND state
and duplicate execution orders among AND siblings.

#### Scenario: Reparent nests a state and rejects cycles

- **WHEN** a state is dropped onto another that is not its descendant
- **THEN** it is nested under that state; and a drop into its own descendant is
  rejected

#### Scenario: AND decomposition shows execution order

- **WHEN** a compound state is set to AND decomposition
- **THEN** its children render numbered execution-order badges

#### Scenario: History on an AND state is linted

- **WHEN** a compound AND state has its history junction enabled
- **THEN** a hierarchy lint flags it inline and on the canvas

### Requirement: State-transition table

The editor SHALL provide a tabular alternative to drawing transitions: one row
per transition with source × dest × event × guard × cond-action × trans-action ×
priority. Edits write back to the chart edges with edge ids preserved, so the
table and canvas stay in sync.

#### Scenario: Edit a transition through the table

- **WHEN** the user edits a row's event/guard/action/priority
- **THEN** the underlying transition edge updates in place (id preserved) and its
  canonical `event[guard]{cond}/trans` label is written

#### Scenario: Rows reflect the chart's transitions

- **WHEN** the chart has transition edges
- **THEN** the table lists one row per transition, ordered by priority then id

### Requirement: Codegen export and live preview

The editor SHALL expose an Export menu over the compiler's codegen lanes —
`-emit-matlab`, `-dump-chart`, `-emit-c`, `-emit-cpp`, `-emit-llvm`,
`-emit-systemverilog` — each writing its artifact beside the model and opening
it, plus a toggleable live `-emit-matlab` preview pane refreshed (debounced) on
edits.

#### Scenario: Each lane writes a distinct artifact

- **WHEN** the user picks an export lane
- **THEN** `matlabc` runs with that lane's flag and the result is written with the
  lane's extension and opened

#### Scenario: Live preview re-runs on edits

- **WHEN** the preview pane is open and the chart changes
- **THEN** the `-emit-matlab` output is regenerated after a short debounce
