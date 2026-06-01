# MatForge IDE (Linux)

A Rust + GTK4 desktop IDE for the [`matlab_llvm`](../matlab_llvm) compiler — a
faithful Linux port of the macOS SwiftUI IDE, preserving the same UI/UX and a
strict **MVVM** architecture.

## Status

Under active construction. See [`docs/roadmap.md`](docs/roadmap.md) for the
phased build plan and current progress.

## Layout

```
crates/core/   matforge-core — GTK-free MVVM core (models, services, view models). Unit-tested ≥90%.
crates/app/    matforge      — GTK4 views + wiring (the binary).
docs/          architecture, UX spec, per-feature docs.
```

The hard split keeps all logic in `matforge-core` (no GTK dependency, fully
unit-testable); the `matforge` binary holds only thin GTK views bound to the
view models.

## Build & run

Requires Rust ≥ 1.80 and `libgtk-4-dev` (GTK 4.10+). The optional
`gtksourceview`/`libadwaita` packages are **not** used.

A [`Justfile`](Justfile) wraps the common workflows (`just --list` to see all):

```sh
just build            # build the workspace
just run              # launch the IDE
just demo             # launch with a sample project opened + compiled
just test             # unit tests (no display needed)
just test-integration # tests against the real matlabc
just coverage         # core coverage summary
just check            # fmt-check + clippy + tests (pre-commit gate)
just doctor           # show resolved matlabc / gtk / rustc
```

Or use Cargo directly:

```sh
cargo build
cargo run -p matforge
cargo test
cargo llvm-cov --package matforge-core   # coverage (needs cargo-llvm-cov)
```

### Compiler location

The IDE shells out to `matlabc`. It is resolved from `$MATLABC_PATH`, then
`~/.config/matforge/config.toml`, defaulting to
`/home/leonardo/work/matlab_llvm/build/matlabc`. The MATLAB runtime archive
(`libMatlabRuntime.a`) is expected alongside it for the Run pipeline.

## License

MIT.
