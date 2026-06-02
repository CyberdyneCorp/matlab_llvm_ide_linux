# Testing strategy

## Where the coverage lives

All logic lives in `matforge-core`, which has **no GTK dependency**, so it tests
without a display. The `matforge` binary is intentionally a thin layer of GTK
widget wiring and is not unit-tested (it is exercised manually + by the
integration tests through the shared core paths).

## Layers

* **Models** — pure value types; tested for construction, serde round-trips
  (the `.mflow` schema renames especially), and derived data. The 84-variant
  `NodeKind` table is exhaustively exercised so no match arm goes uncovered.
* **Services** — the pure logic (highlighter, `.mflow` codec, `whos`/`disp`
  parsers, sentinel router + base64, DAP framing, compiler argv + diagnostic
  parser, run command builder, settings resolution) is tested directly on
  fixtures. Trait-based services have an in-crate fake and the real impl is
  smoke-tested with a portable stand-in process (`echo`/`sh`) and a temp-dir.
* **View models** — driven through their verb methods with fake services; tests
  subscribe to the `Property`s and assert emitted state. This is the bulk of the
  suite and the composition root (`MainViewModel`) covers compile orchestration,
  REPL event routing, and the file/clipboard commands.

## Running

```sh
cargo test                                   # ~250 unit tests, no display
cargo llvm-cov --package matforge-core --summary-only

# Real-compiler integration (skips cleanly when matlabc is absent):
MATLABC_PATH=/path/to/matlabc cargo test -p matforge-core --test integration
```

## End-to-end (GTK)

Beyond unit + integration tests, `e2e/` drives the **real binary** with
synthesized X11 input and asserts on real app state (see
[`docs/e2e.md`](e2e.md)): `just e2e-setup` then `just e2e`. Covers gutter/F9
breakpoints and the live REPL → workspace round-trip.

## Coverage target

≥ 90% on `matforge-core`. Current measured: **~95% region / line / function**.
The `node.rs` exhaustive test and the real-impl smoke tests are what push the
service/model files to ~100%.

## Integration tests

`crates/core/tests/integration.rs` runs the real `matlabc`:

* `emit_cpp_produces_source` — `-emit-cpp` yields non-empty generated C++.
* `emit_llvm_contains_ir` — `-emit-llvm` output looks like LLVM IR.
* `diagnostics_surface_for_bad_source` — a bad source fails or emits a parseable
  diagnostic.

All three pass against the shipped compiler build.
