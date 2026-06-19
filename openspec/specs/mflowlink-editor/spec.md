# mflowlink-editor Specification

## Purpose
TBD - created by archiving change add-mflowlink-block-inspector. Update Purpose after archive.
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
</content>

