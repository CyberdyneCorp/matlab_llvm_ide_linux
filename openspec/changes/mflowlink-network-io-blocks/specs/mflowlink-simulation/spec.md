## ADDED Requirements

### Requirement: Network receivers break algebraic loops

The editor's algebraic-loop detection SHALL treat `signal_udp_recv` and `signal_tcp_recv` as
loop-breakers, because a receiver's output does not depend on the current step's input. A receiver
placed in a feedback path SHALL NOT be reported as part of an algebraic loop. The send blocks
(`signal_udp_send`, `signal_tcp_send`) are direct feedthrough and SHALL NOT break loops.

#### Scenario: Receiver in a feedback path is not flagged

- **WHEN** a model wires a `signal_udp_recv` (or `signal_tcp_recv`) into a feedback path that would
  otherwise be an algebraic loop
- **THEN** the editor does not flag the receiver or that path as an algebraic loop

#### Scenario: Send block does not break a loop

- **WHEN** a model forms a direct-feedthrough cycle through a `signal_udp_send` (or
  `signal_tcp_send`) block
- **THEN** the editor still reports the cycle as an algebraic loop
