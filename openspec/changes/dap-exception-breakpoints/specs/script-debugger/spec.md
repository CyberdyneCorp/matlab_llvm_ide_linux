## ADDED Requirements

### Requirement: Pause on uncaught MATLAB errors

The script debugger SHALL let the user pause on uncaught MATLAB errors via the DAP `error` exception filter. When the "error" exception filter is enabled, the IDE SHALL send `setExceptionBreakpoints` with that filter so an uncaught `error()` pauses the run with a `stopped` event whose reason is `"exception"`.

#### Scenario: Error filter armed

- **WHEN** the user enables the "error" exception filter and starts a debug session
- **THEN** the IDE sends `setExceptionBreakpoints` including the `error` filter

#### Scenario: Run pauses on an uncaught error

- **WHEN** the debugged program raises an uncaught `error()` with the filter armed
- **THEN** the debugger pauses on the failing frame (a `stopped` event with reason `"exception"`)

### Requirement: Surface the exception identifier and message

The debugger SHALL surface the exception that the run paused on. On a `stopped` event with reason `"exception"`, the IDE SHALL request `exceptionInfo` and SHALL display the MATLAB error identifier and message (console, status bar, and a Debug-panel banner). The exception SHALL be cleared when the run resumes or terminates, and a normal (`breakpoint`/`step`) stop SHALL NOT show a stale exception.

#### Scenario: Exception details shown

- **WHEN** the debugger pauses on an exception
- **THEN** the IDE requests `exceptionInfo` and shows the error identifier and message

#### Scenario: Exception cleared on resume

- **WHEN** the user resumes or the session terminates
- **THEN** the exception display is cleared

#### Scenario: Normal stop shows no exception

- **WHEN** the debugger next pauses on a breakpoint or a step (not an exception)
- **THEN** no exception banner is shown
