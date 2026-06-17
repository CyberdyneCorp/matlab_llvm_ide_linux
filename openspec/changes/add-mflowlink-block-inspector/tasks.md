# Tasks

## 1. Core — parameter constraints + validation
- [x] 1.1 Add `ParamConstraint` enum and a `constraint` field to `SignalFlowParamSpec`
- [x] 1.2 Tag each per-kind field with its constraint (counts → Integer, coeffs → CoeffList, A/B/C → Matrix, signs → Signs, op/method/distribution → Enum)
- [x] 1.3 Add `SignalFlowParamSpec::validate(input) -> Result<(), String>` and `validate_field(kind, key, input)`
- [x] 1.4 Unit tests: valid/invalid per constraint; empty clears; every default value validates

## 2. Core — algebraic-loop detection
- [x] 2.1 `flowchart::analysis::algebraic_loop_nodes(flow)` (reachability over data edges minus loop-breaker out-edges)
- [x] 2.2 `FlowchartDocument::algebraic_loop_nodes()` + `FlowchartViewModel::algebraic_loop_nodes()`
- [x] 2.3 Unit tests: feedthrough loop flagged; integrator/unit-delay in loop clears it; acyclic empty; self-loop flagged

## 3. App — inspector validation UI
- [x] 3.1 Per-param inline error label + `mf-field-error`; commit only valid values
- [x] 3.2 Inspector note when the selected block sits on an algebraic loop
- [x] 3.3 `mf-field-error` style in theme.css

## 4. App — canvas diagnostic
- [x] 4.1 Pass the algebraic-loop set into `draw_document`; amber dashed outline on those nodes (editor + sim windows)
- [x] 4.2 Redraw hook so the outline updates as edges change (existing `Hook::Doc` covers it)

## 5. Verify
- [x] 5.1 `cargo test` (382 tests green), `cargo fmt`, `cargo clippy` (changed files clean)
- [ ] 5.2 Manual smoke: bad coeff → inline error; feedthrough loop → amber outline
</content>
