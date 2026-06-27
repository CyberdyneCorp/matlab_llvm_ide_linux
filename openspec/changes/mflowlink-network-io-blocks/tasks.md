## 1. Core model: node kinds

- [x] 1.1 Add four `NodeKind` variants in `crates/core/src/models/flowchart/node.rs` with
  `#[serde(rename)]`: `SignalUdpSend` → `signal_udp_send`, `SignalUdpRecv` → `signal_udp_recv`,
  `SignalTcpSend` → `signal_tcp_send`, `SignalTcpRecv` → `signal_tcp_recv` (new "Network I/O"
  section in the enum).
- [x] 1.2 Register the four kinds in `NodeKind::ALL` (length 136 → 140).
- [x] 1.3 Map them to the new `NodeCategory::SignalNetwork` in `category()`.
- [x] 1.4 Add display labels: "UDP Send", "UDP Receive", "TCP Send", "TCP Receive".
- [x] 1.5 Define `FlowPorts`: send = inputs `[in]`, outputs `[out]`; recv = inputs `[]`,
  outputs `[out]`.
- [x] 1.6 Define port-side directions to match (send `in` Left + `out` Right; recv `out` Right).
- [x] 1.7 Add `SignalUdpRecv` and `SignalTcpRecv` to `breaks_algebraic_loop()`.

## 2. Core model: palette taxonomy

- [x] 2.1 Add `SignalNetwork` to the `NodeCategory` enum in
  `crates/core/src/models/flowchart/palette.rs`.
- [x] 2.2 Add its `label()` ("Network I/O"), `accent()`, `is_signal_flow()` membership, and a slot
  in `signal_flow_order()` (after HDL); bump the array length (12 → 13).
- [x] 2.3 Add `SignalFlowParamSpec` lists: send = `host` (text "127.0.0.1"), `port` (int 5000);
  recv = `host`, `port`, `initialValue` (number 0.0).

## 3. Tests

- [x] 3.1 Add `network_io_blocks_are_exposed_with_ports_and_params` (mirrors the #343 test):
  serde string, category, ports, anchors, params, and library membership for all four blocks.
- [x] 3.2 Assert `SignalUdpRecv` / `SignalTcpRecv` break algebraic loops and the send blocks do not.
- [x] 3.3 Add `network_io_blocks_round_trip_losslessly` in `flowchart_codec.rs` (a model with all
  four blocks loads, retains every node, and re-serializes with kind + params preserved).
- [x] 3.4 Updated the pre-existing `display_orders_are_complete` and
  `every_category_has_label_accent_and_one_dialect` tests for the new category.

## 4. Build & docs

- [x] 4.1 `cargo check` (core + app) and `cargo test -p matforge-core` green.
- [x] 4.2 Update `docs/roadmap.md` block-library entry with the Network I/O family.
- [x] 4.3 `openspec validate mflowlink-network-io-blocks --strict` passes.
