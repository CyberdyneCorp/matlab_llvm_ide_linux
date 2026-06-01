# Architecture

MatForge IDE (Linux) is a faithful Rust + GTK4 port of the macOS SwiftUI IDE for
the `matlab_llvm` compiler. It preserves the reference's strict **MVVM** layering
and UI/UX while making all application logic GTK-free and unit-testable.

## Workspace layout

```
crates/core/   matforge-core — models · services · view models (NO gtk dependency)
crates/app/    matforge      — GTK4 views + wiring (the binary)
docs/          this documentation
```

The hard crate split is the central architectural decision: **every line of
logic lives in `matforge-core` and is unit-tested** (≥90% coverage); the
`matforge` binary holds only thin GTK views that subscribe to view-model state
and call verb methods. Nothing in `core` imports `gtk`.

## The four MVVM layers

| Layer | Location | Responsibility | Imports |
|-------|----------|----------------|---------|
| **Models** | `core/src/models` | Pure value types (project tree, editor tabs, flowchart documents, plots, compiler config, DAP types). | `serde` only |
| **Services** | `core/src/services` | Side effects behind traits (compiler/REPL/DAP processes, file system, clipboard) + pure logic (syntax highlighter, `.mflow` codec, output parsers, DAP framing). | std + `serde` |
| **View Models** | `core/src/viewmodels` | Reactive state + verb methods. Depend only on service *traits*. | core only |
| **Views** | `app/src` | GTK widgets bound to view-model state. | `gtk4` + core |

This mirrors the reference's `Models/` → `Services/` → `ViewModels/` → `Views/`
directories one-to-one (see `docs/ui_ux.md` for the panel mapping).

## Reactivity: `Property<T>`

SwiftUI's `@Published` is replaced by [`observable::Property<T>`](../crates/core/src/observable.rs):
a single-threaded, push-based observable holding a value and a list of subscriber
closures. View models expose `Property`s; views call `property.bind(closure)` to
update a widget now and on every change, and call verb methods on user input.

* **Single-threaded** — the whole UI runs on the GTK main loop, matching the
  reference's `@MainActor` isolation. `Rc`/`RefCell`, no locks.
* **Re-entrancy safe** — subscribers are snapshotted before notification, so a
  subscriber may `set`/`subscribe`/`unsubscribe` without a borrow panic.
* **Testable** — view-model tests subscribe to a `Property` and assert the
  emitted values; no GTK, no display.

## Services: traits + real impl + fake

Each side-effecting service is a **trait** with a real implementation and an
in-crate **fake**. View models hold trait objects, so unit tests inject fakes
(`FakeFileSystem`, `FakeCompilerService`, `FakeFilePicker`, `FakeClipboard`).
The pure logic of each service (argv builders, parsers, framing, the highlighter)
is plain functions carrying most of the coverage. Real process impls are covered
by the env-gated integration tests.

## Concurrency model

The compiler and run pipeline are invoked synchronously from the toolbar (small
programs compile fast). The view-model command logic is deliberately split into
a pure **build-request** step (`compile_invocation`) and an **apply-result** step
(`apply_compile_result`) so the work can be moved to a worker thread and the
result marshalled back to the main loop without changing the view models. Live
REPL/DAP streaming uses this same split (see `docs/roadmap.md`).

## Composition root

[`MainViewModel`](../crates/core/src/viewmodels/main.rs) owns every sub view
model and the service handles, and implements the cross-cutting commands
(compile, run, open, REPL event routing). The GTK `main` constructs it with the
real services (`RealFileSystem`, `GtkClipboard`) and hands it to `ui::build`.
