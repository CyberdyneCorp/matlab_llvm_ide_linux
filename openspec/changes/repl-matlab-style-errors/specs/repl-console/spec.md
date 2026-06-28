## ADDED Requirements

### Requirement: MATLAB-style interpret-mode error presentation

The REPL transcript SHALL present interpret-mode runtime errors like MATLAB's command window. A line that is a compiler/runtime diagnostic — an `error:` or `warning:` token, optionally preceded by a `<location>:line:col:` position — SHALL be shown at the Error or Warning severity with the clang-style prefix and any leading position stripped, leaving the bare MATLAB message. Ordinary output SHALL be shown unchanged at Plain severity.

#### Scenario: Undefined name reads like MATLAB

- **WHEN** the interpreter emits `<repl:0>:1:5: error: Unrecognized function or variable 'foo'.`
- **THEN** the transcript shows `Unrecognized function or variable 'foo'.` at Error severity

#### Scenario: Runtime bounds error reads like MATLAB

- **WHEN** the interpreter emits `error: Index exceeds the number of array elements. Index must not exceed 3.`
- **THEN** the transcript shows `Index exceeds the number of array elements. Index must not exceed 3.` at Error severity

#### Scenario: Ordinary output is not misclassified

- **WHEN** a program prints an ordinary line that merely contains the word "error" (with no
  `error:` diagnostic token)
- **THEN** the transcript shows the line unchanged at Plain severity
