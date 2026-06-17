# Project — matlab_llvm_ide_linux

A GTK4 + Rust port of the MATLAB-LLVM IDE. Strict MVVM: all logic lives in the
GTK-free `matforge-core` crate (`crates/core`); GTK views + wiring live in
`crates/app`. The IDE drives the `matlabc` compiler (`/home/leonardo/work/matlab_llvm`)
as a subprocess.

## Key surfaces

- **mflowLink** — signal-flow (Simulink-like) block-diagram editor + one-shot
  `matlabc -simulate` CSV simulation window.
- **mStateflow** — state-chart editor + `matlabc -emit-trace` live event window.
- **Flowchart** — control-flow `.mflow` editor that lowers via `matlabc -emit-matlab`.

## Conventions

- Logic + data live in `crates/core` and must be unit-testable without GTK.
- Every block parameter is pinned by `SignalFlowParamSpec` (single source of
  truth with the compiler's `docs/mflowlink_blocks.md`).
- New behaviour ships with tests; bug fixes ship with a regression test.
- Run `cargo fmt`, `cargo clippy`, `cargo test` before calling work done.

## Workflow

- Use OpenSpec for non-trivial features: propose → specs → implement → archive.
</content>
