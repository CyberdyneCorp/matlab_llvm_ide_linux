## Why

The matlab_llvm compiler (PR #421) added a **network-I/O** block family to mflowLink:
`signal_udp_send`, `signal_udp_recv`, `signal_tcp_send`, `signal_tcp_recv`. These let a model
stream signals to/from external peers over UDP/TCP, once per major solver step, deterministically
(never inside an RK4 minor stage). The compiler can simulate and code-gen these blocks today, but
the IDE's node model does not know them: they are absent from the palette, have no parameter
schema, and (for the receivers) are not registered as algebraic-loop breakers. A user authoring a
hardware-in-the-loop or co-simulation model in the IDE therefore cannot drop these blocks on the
canvas — they would only round-trip as opaque unknown nodes.

This change makes the four network-I/O blocks first-class IDE blocks, the same way #76 exposed the
36 compiler #343 toolbox blocks.

## What Changes

- Add four typed signal-flow block kinds matching the compiler's serde names exactly:
  `signal_udp_send`, `signal_udp_recv`, `signal_tcp_send`, `signal_tcp_recv`.
- Add a new **Network I/O** palette category (`SignalNetwork`) grouping the four blocks, with an
  accent color and a slot in the signal-flow palette display order.
- Give each block its inspector parameter schema, matching the compiler's camelCase keys and
  defaults:
  - send blocks: `host` (default `127.0.0.1`), `port` (default `5000`).
  - receive blocks: `host`, `port`, and `initialValue` (default `0.0`, the held value before the
    first packet).
- Wire ports to match compiler semantics: send blocks have one input `in` (passed through to an
  output `out` for chaining/logging); receive blocks have only an output `out`.
- Register the two **receive** blocks as algebraic-loop breakers, so a receiver sitting in a
  feedback path is not flagged as an algebraic loop by the editor's static analysis (matching the
  compiler's loop-breaker semantics).
- Ensure all four blocks round-trip losslessly through `.mflow` (kind + params preserved).

## Capabilities

### Modified Capabilities
- `mflowlink-editor`: The signal-flow palette gains a Network I/O category with four authorable
  TCP/UDP blocks, each with a parameter schema and ports; the inspector validates their parameters.
- `mflowlink-simulation`: The editor's algebraic-loop detection treats `signal_udp_recv` and
  `signal_tcp_recv` as loop-breakers, consistent with the compiler.

## Impact

- **Modified code**:
  - `crates/core/src/models/flowchart/node.rs` — four `NodeKind` variants (with `#[serde(rename)]`),
    `ALL` registry entries, `category()`, display label, `FlowPorts`, port-side direction, and
    `breaks_algebraic_loop()` for the receivers.
  - `crates/core/src/models/flowchart/palette.rs` — `SignalNetwork` category (`label`, `accent`,
    `is_signal_flow`, `signal_flow_order`) and the four `SignalFlowParamSpec` lists.
- **Compiler dependency**: requires a `matlabc` that supports the network-I/O blocks (PR #421).
  The IDE only authors/serializes the blocks; simulation and code-gen happen in the compiler.
- **Testing**: a unit test (mirroring the #343 block test) asserting each block's serde string,
  category, ports, and params; a loop-breaker assertion for the receivers; a `.mflow` round-trip
  regression test.
- **Docs**: update `docs/compiler_integration.md` / the block inventory to list the network-I/O
  category.
