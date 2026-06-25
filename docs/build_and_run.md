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

* For the **embedded 3-D Scene viewer** (the `scene3d` feature, **on by
  default**): WebKitGTK 6.0 development libraries (`libwebkitgtk-6.0-dev` on
  Debian/Ubuntu, which pulls in `libjavascriptcoregtk-6.0-dev` and
  `libsoup-3.0-dev`). Runtime needs `libwebkitgtk-6.0-4`. Build with
  `--no-default-features` to skip it (the viewer then falls back to the browser).

## Build & run

```sh
cargo build                 # whole workspace
cargo run -p matforge       # launch the IDE
```

### 3-D Scene viewer (`scene3d` feature, default-on)

mflowLink models that use the compiler's `signal_*3d` scene blocks can be rendered
as an interactive 3-D scene. The compiler emits a self-contained Babylon.js HTML
(`matlabc -emit-mflowlink-babylon`) with orbit/zoom/pan/play built in; the IDE
surfaces a **3-D Scene** button (flowchart toolbar and mflowLink window) for those
models. The blocks themselves load as named **World 3-D** / **Actor 3-D** / …
blocks (the IDE preserves these untyped kinds losslessly).

```sh
cargo run -p matforge                       # embedded 3-D window (default)
cargo run -p matforge --no-default-features # minimal: opens the scene in the browser
```

The embedded window needs `libwebkitgtk-6.0-dev` at build time. With
`--no-default-features` there is no WebKitGTK dependency and the **3-D Scene**
button opens the generated HTML in the system browser (`xdg-open`).

To render offline (no CDN), point `MATFORGE_BABYLON_INLINE` at a Babylon bundle to
inline; otherwise the generated HTML references Babylon from its CDN.

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
| `MATFORGE_BABYLON_INLINE=<bundle.js>` | inline a Babylon bundle into the generated 3-D scene so the embedded viewer renders offline |

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
