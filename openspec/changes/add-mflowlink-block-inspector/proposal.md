# Add mflowLink block inspector validation + algebraic-loop diagnostics

## Why

The signal-flow inspector accepts any text into block parameters via
`ParamValue::parse`, so a malformed transfer-function coefficient list or a
non-numeric gain is stored silently and only fails later inside `matlabc`.
There is also no edit-time feedback for algebraic loops — direct-feedthrough
cycles the solver cannot resolve — even though the compiler rejects them.

This closes IDE issue #29 (per-kind block inspector with validation +
algebraic-loop diagnostics), the first foundational slice of the mflowLink
production epic (#25).

## What Changes

- `SignalFlowParamSpec` gains a per-field `ParamConstraint` (Number, Integer,
  CoeffList, Matrix, Signs, Enum, Text) and a `validate` method, plus a
  `validate_field(kind, key, input)` entry point.
- New `flowchart::analysis::algebraic_loop_nodes` computes the set of blocks on
  an algebraic loop (Tarjan SCC over data edges, dropping the out-edges of
  loop-breaker blocks such as Integrator / Unit Delay / ZOH), exposed through
  the document and the flowchart view model.
- The inspector shows an inline error under any invalid parameter field and
  refuses to commit invalid values; it notes when the selected block lies on an
  algebraic loop.
- The canvas outlines algebraic-loop blocks in amber and redraws as edges change.

## Impact

- Affected specs: `mflowlink-editor` (new capability).
- Affected code: `crates/core/src/models/flowchart/{palette,analysis,document,mod}.rs`,
  `crates/core/src/viewmodels/flowchart.rs`,
  `crates/app/src/{flowchart_view,flow_render}.rs`, `crates/app/resources/theme.css`.
- No schema change; `.mflow` round-trips unchanged.
</content>
