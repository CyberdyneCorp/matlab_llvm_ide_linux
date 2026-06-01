# Build, run, and test

## Prerequisites

* Rust ≥ 1.80 (developed on 1.92) and Cargo.
* GTK 4.10+ development libraries (`libgtk-4-dev` on Debian/Ubuntu). GTK 4.14 is
  what the project is developed against.
* For Compile/Run against the compiler: the `matlabc` binary and
  `libMatlabRuntime.a` from the [`matlab_llvm`](../../matlab_llvm) build, plus
  `clang++` for the Run link step.

> `gtksourceview` and `libadwaita` are **not** required — the editor uses a
> custom in-crate highlighter and the theme is hand-written GTK4 CSS.

## Build & run

```sh
cargo build                 # whole workspace
cargo run -p matforge       # launch the IDE
```

### Pointing at the compiler

`matlabc` is resolved from, in order:

1. `$MATLABC_PATH`
2. `~/.config/matforge/config.toml` (future)
3. the built-in default `/home/leonardo/work/matlab_llvm/build/matlabc`

`libMatlabRuntime.a` is expected next to the `matlabc` binary. If the binary is
missing the IDE still runs; the status bar notes the missing path and Compile/Run
report the error.

```sh
MATLABC_PATH=/path/to/matlab_llvm/build/matlabc cargo run -p matforge
```

### Demo / verification env vars

`main` honours three optional startup variables used for screenshots and manual
verification:

| Variable | Effect |
|----------|--------|
| `MATFORGE_OPEN=<folder>` | open the folder in the Explorer on launch |
| `MATFORGE_FILE=<file>` | open the file in the editor (`.m`) or flowchart canvas (`.mflow`) |
| `MATFORGE_COMPILE=1` | compile the opened file once on launch |
| `MATFORGE_REPL=<cmd>` | start the live REPL and run `<cmd>` on launch |
| `MATFORGE_DEBUG=1` | start a debug session on the opened file |
| `MATFORGE_PLOT=1` | add a sample figure to the Plots panel |
| `MATFORGE_NEWFLOW=control\|signal` | open a demo flowchart on the canvas |

## Test

```sh
cargo test                                   # all unit tests (no display needed)
cargo test -p matforge-core                  # core only

# Integration tests against the real compiler (skip if matlabc is absent):
MATLABC_PATH=/path/to/matlabc \
    cargo test -p matforge-core --test integration
```

## Coverage

```sh
cargo install cargo-llvm-cov            # once
cargo llvm-cov --package matforge-core --summary-only
```

Coverage is enforced on `matforge-core` (the GTK views in `matforge` are thin
glue and excluded). Current: **~95% region / line** on the core crate.
