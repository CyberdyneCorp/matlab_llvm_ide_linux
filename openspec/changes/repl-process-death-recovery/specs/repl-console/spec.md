## ADDED Requirements

### Requirement: Recover from REPL process exit

The IDE SHALL detect when the `matlabc -repl` process exits or crashes and SHALL recover gracefully. When the process's output pipes close, the IDE SHALL clear the dead session, mark the REPL as not running, and post a transcript note. The next submitted command SHALL transparently start a fresh REPL.

#### Scenario: REPL process crashes mid-session

- **WHEN** the `matlabc -repl` process exits or crashes
- **THEN** the IDE marks the REPL not running and posts a note that the process ended

#### Scenario: Next command restarts the REPL

- **WHEN** the user submits a command after the REPL process has ended
- **THEN** a fresh `matlabc -repl` is started and the command runs

#### Scenario: No silent dead state

- **WHEN** the REPL process has ended
- **THEN** the IDE does not keep reporting the REPL as running with commands silently failing
