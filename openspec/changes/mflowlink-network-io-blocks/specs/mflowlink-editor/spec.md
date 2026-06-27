## ADDED Requirements

### Requirement: Network I/O block family

The mflowLink signal-flow editor SHALL provide four authorable network-I/O blocks under a
**Network I/O** palette category: UDP Send (`signal_udp_send`), UDP Receive (`signal_udp_recv`),
TCP Send (`signal_tcp_send`), and TCP Receive (`signal_tcp_recv`). The serialized block kind for
each SHALL match the compiler's name exactly, so models round-trip with `matlabc`.

#### Scenario: Network I/O category appears in the signal-flow palette

- **WHEN** the user opens the block library for a signal-flow model
- **THEN** a "Network I/O" category is listed containing UDP Send, UDP Receive, TCP Send, and
  TCP Receive

#### Scenario: Send block has a pass-through port shape

- **WHEN** the user adds a UDP Send or TCP Send block
- **THEN** the block has one input port `in` and one output port `out` (the input passed through
  for chaining/logging)

#### Scenario: Receive block is a source

- **WHEN** the user adds a UDP Receive or TCP Receive block
- **THEN** the block has a single output port `out` and no input ports

#### Scenario: Block round-trips through .mflow

- **WHEN** the user saves a model containing any network-I/O block and reopens it
- **THEN** the block is restored as the same typed kind with its parameters unchanged

### Requirement: Network I/O block parameters

The inspector SHALL expose each network-I/O block's parameters with the compiler's keys and
defaults, and SHALL validate them. Send blocks SHALL expose `host` (default `127.0.0.1`) and
`port` (default `5000`). Receive blocks SHALL additionally expose `initialValue` (default `0.0`),
the value held before the first packet arrives. `port` SHALL be validated as a non-negative
integer.

#### Scenario: Editing a send block's endpoint

- **WHEN** the user edits a UDP Send block's `host` and `port`
- **THEN** the inspector accepts a hostname/IP for `host` and a non-negative integer for `port`,
  and persists both to the `.mflow`

#### Scenario: Invalid port is rejected

- **WHEN** the user enters a non-integer or negative `port`
- **THEN** the inspector flags the value as invalid at edit time
