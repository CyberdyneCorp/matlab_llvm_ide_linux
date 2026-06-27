## Context

The IDE keeps a typed model of every mflowLink block in
`crates/core/src/models/flowchart/node.rs` (the `NodeKind` enum) plus a parameter/palette
taxonomy in `crates/core/src/models/flowchart/palette.rs`. Serialization to/from `.mflow` is
driven by each variant's `#[serde(rename = "...")]`; the string MUST equal the compiler's block
kind. Adding a block is a fixed checklist of edits across these two files; `cargo check` enforces
exhaustiveness on the `match` arms so nothing is missed.

The compiler (PR #421, `docs/mflowlink_blocks.md` lines 334–337) defines the four blocks:

| Kind | Ports | Params | Loop-breaker |
|---|---|---|---|
| `signal_udp_send` | in → out (pass-through) | `host`, `port` | no (direct feedthrough) |
| `signal_udp_recv` | out only | `host`, `port`, `initialValue` | **yes** |
| `signal_tcp_send` | in → out (pass-through) | `host`, `port` | no |
| `signal_tcp_recv` | out only | `host`, `port`, `initialValue` | **yes** |

## Goals / Non-Goals

**Goals**
- Author, edit, validate, and round-trip the four blocks in the IDE.
- Match the compiler's serde names, parameter keys, defaults, and port shapes exactly.
- Correct algebraic-loop analysis for the receivers (loop-breakers).

**Non-Goals**
- No changes to simulation, code-gen, or runtime — those live in the compiler.
- No new compiler invocation flags; `-simulate` already understands these blocks.
- No transport/socket logic in the IDE.

## Decisions

### New `SignalNetwork` category vs. reuse `SignalComms`
Decision: **new category "Network I/O"**. The compiler documents network-I/O as its own family,
and the IDE already gives Comms / DSP / HDL / 3-D their own categories. A dedicated category keeps
the palette legible and matches the established pattern from #76. Placed in `signal_flow_order`
after HDL (so transport blocks sit near other peripheral/IO-style families). Accent reuses an
existing palette color (`ACCENT_CYAN`); the palette already shares accent colors across categories.

### Ports: send blocks pass through
The compiler states the send block "passes the input through to the output port for
chaining/logging". So send blocks expose `in` (Left) and `out` (Right). Receive blocks expose only
`out` (Right). This mirrors `SignalSensor3D` (out-only) and the pass-through pattern.

### Receivers are loop-breakers
`signal_udp_recv` / `signal_tcp_recv` outputs do not depend on the current step's input, so they
are added to `NodeKind::breaks_algebraic_loop()`. Without this, a receiver in a feedback path would
be falsely flagged by `analysis::algebraic_loop_nodes`. Send blocks are direct feedthrough and are
**not** loop-breakers.

### Parameter types
`host` is free-form `Text`; `port` is `Integer { min: 0 }`; `initialValue` is a `Number`. Defaults
copy the compiler doc (`127.0.0.1`, `5000`, `0.0`). The receive param order follows the compiler
doc (`port`, `host`, `initialValue`) but the inspector order is cosmetic — we keep `host`, `port`,
`initialValue` for consistency with the send blocks.

## Risks / Trade-offs

- **Serde-name drift**: if the compiler renames a block, `.mflow` files won't load. Mitigated by a
  unit test pinning the exact serde strings against the compiler doc.
- **Loop-breaker correctness**: a wrong classification causes either false algebraic-loop warnings
  (recv not marked) or missed real loops (send wrongly marked). Covered by a dedicated assertion.

## Migration

None. Existing `.mflow` files are unaffected; previously these blocks (if present) round-tripped as
unknown nodes and now resolve to typed blocks.
