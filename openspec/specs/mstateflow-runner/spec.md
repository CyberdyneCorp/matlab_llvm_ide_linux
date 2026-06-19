# mstateflow-runner Specification

## Purpose

The mStateflow live-trace window: runs a chart through `matlabc -emit-trace`
(one-shot) or `--sim-dap` (live `stateChart/*`) and introspects the active-state
set and the streamed event log. The tested `StateChartViewModel` folds events;
the window is GTK glue plus the subprocess.

## Requirements

### Requirement: Active-state hierarchy pane

The window SHALL show the chart's state hierarchy as an indented tree with a
live active marker per state and an OR/AND decomposition badge per compound,
updated as states enter and exit.

#### Scenario: Active states are marked in the tree

- **WHEN** a state becomes active during a run
- **THEN** its tree row shows the active marker, and an inactive state shows the
  inactive marker

#### Scenario: The tree nests by parent

- **WHEN** the chart has nested states
- **THEN** the pane renders them indented under their parent with the parent's
  OR/AND badge

### Requirement: Event log with sim-time, reveal, and CSV

The window SHALL show one row per chart event prefixed with its super-step index
("sim time"). Clicking a row reveals that event's state on the canvas. The log
exports to CSV as `step,kind,detail`.

#### Scenario: Events carry a super-step index

- **WHEN** events arrive between super-step boundaries
- **THEN** each logged event is tagged with the current super-step index and the
  row shows `[i] …`

#### Scenario: Clicking a row reveals its state

- **WHEN** the user clicks an event-log row
- **THEN** the row's state is highlighted on the chart canvas

#### Scenario: Export the event log to CSV

- **WHEN** the user exports the log
- **THEN** a `step,kind,detail` CSV is written beside the model file

### Requirement: Active-state halos on the canvas

The window's chart canvas SHALL halo every currently-active state, with the
event-log "reveal" state highlighted distinctly.

#### Scenario: Active states are haloed

- **WHEN** one or more states are active
- **THEN** each is haloed on the canvas; and the revealed state gets a distinct
  highlight on top

### Requirement: Live super-step / transition stepping

In live `--sim-dap` mode the window SHALL step the chart by a full super-step or
by a single transition.

#### Scenario: Step a super-step or a transition

- **WHEN** the user presses **Step Super-Step** or **Step Transition** during a
  live run
- **THEN** the runner advances by a quiescent super-step or exactly one
  transition respectively
