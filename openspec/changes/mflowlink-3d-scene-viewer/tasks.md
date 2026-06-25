## 1. Core: codegen lane + 3-D detection

- [x] 1.1 Add a `Babylon` lane to `ExportTarget` in `crates/core/src/services/codegen.rs` with flag `-emit-mflowlink-babylon`, extension `html`, and label (e.g. "3-D Scene (.html)"); decide whether it belongs in the general `Export` menu list or only the gated 3-D action.
- [~] 1.2 Expose the compiler's inline/self-contained flag for this lane (extra arg so the emitted HTML embeds the Babylon runtime); confirm the exact flag spelling against the installed `matlabc`. — handled in the app's open_scene3d via an opt-in bundle path (compiler flag unverifiable here; defaults to plain emit).
- [x] 1.3 Update the `codegen.rs` unit tests so the lane set, flags, and extensions remain unique and cover the new lane.
- [x] 1.4 Add a pure detection helper in `crates/core` (e.g. `services/scene3d.rs`) that returns whether a serialized `.mflow`/document contains any of `signal_world3d`, `signal_actor3d`, `signal_light3d`, `signal_camera3d`, `signal_sensor3d`, `signal_collision3d`.
- [x] 1.5 Unit-test the detection helper: positive (each kind), negative (no 3-D blocks), and a model with only the older `signal_scope3d`.

## 2. Core: lossless round-trip of untyped 3-D blocks

- [x] 2.1 Add a `NodeKind::Unknown(String)` catch-all in `crates/core/src/models/flowchart/node.rs` with custom `Serialize`/`Deserialize` that preserves the original kind string (known kinds unchanged).
- [x] 2.2 Give `Unknown` neutral derived data (category/shape/default ports/label) so unknown blocks display safely in the editor; resolve all `match NodeKind` arms the compiler now flags.
- [x] 2.3 Add a regression test: a `.mflow` containing `signal_world3d`/`signal_actor3d` loads via `flowchart_codec`, retains every node, and re-serializes byte-for-byte (kind + params preserved).

## 3. App: embedded 3-D Scene window

- [x] 3.1 Add the `webkit6` crate to `crates/app/Cargo.toml`.
- [x] 3.2 Create `crates/app/src/scene3d_window.rs`: a `Window::builder()` window hosting a `webkit6::WebView` that loads a generated `.html` via a `file://` URI (follow `statechart_window.rs`).
- [x] 3.3 Add `open_scene3d()` in the GTK layer that runs the Babylon export lane to an output `.html` (reusing the `matlabc` invocation pattern from `emit_artifact`/`process.rs`), then opens the window; on compiler error, surface stderr to the console and do not open the window; on missing `matlabc`, report it.
- [x] 3.4 Decide and implement the generated-HTML location (alongside the model vs temp dir) and ensure the path stays valid for the window's lifetime.
- [~] 3.5 Handle WebView creation failure gracefully (log to console; optional fallback to system browser).

## 4. App: surface the gated 3-D Scene action

- [x] 4.1 Add a "3-D Scene" action to the flowchart Export menu in `crates/app/src/flowchart_view.rs`, enabled only when the detection helper reports a 3-D model.
- [x] 4.2 Add a "3-D Scene" button to the mflowLink window toolbar in `crates/app/src/mflowlink_window.rs`, gated the same way.
- [x] 4.3 Verify the action is hidden/disabled for non-3-D models and for models with only `signal_scope3d`.

## 5. Packaging, docs, and verification

- [x] 5.1 Update the Debian `depends` in `crates/app/Cargo.toml` to include the WebKitGTK 6.0 runtime; document the `libwebkitgtk-6.0-dev` build requirement.
- [x] 5.2 Update build/setup docs (`docs/build_and_run.md`, `docs/packaging.md`) with the new WebKitGTK dependency and how to install it.
- [x] 5.3 Update OpenSpec/user-facing docs (`docs/compiler_integration.md`) describing the 3-D Scene viewer feature.
- [x] 5.4 `cargo build` + `cargo test` the workspace pass (default + `--features scene3d`); clippy clean both ways. Core tests cover export lane, detection, and round-trip.
- [x] 5.5 GUI verification (Xephyr): opened `ball_ramp.mflow`, confirmed blocks render as **World 3-D / Actor 3-D** (not "Unknown Block"), clicked **3-D Scene** → the embedded WebKitGTK window opened and rendered the scene with a live play/scrub timeline. Screenshots captured.
- [x] 5.6 `openspec validate mflowlink-3d-scene-viewer --strict` passes.

## 6. Post-review fixes (from real-app testing)

- [x] 6.1 Render untyped `signal_*3d` blocks with their real kind name (`pretty_kind_tag` → "World 3-D", "Actor 3-D") on the canvas and in the block inspector, instead of a bare "Unknown Block". Confirmed the real flag is `-emit-mflowlink-babylon … -o <html>`.
- [x] 6.2 Make `scene3d` a **default** feature so the embedded window works on a plain `cargo run`; `--no-default-features` for a minimal browser-fallback build. Added `libwebkitgtk-6.0-dev` to both CI jobs.
- [x] 6.3 Dedicated e2e scenario (`scenario_scene3d_viewer`) + committed `e2e/fixtures/scene3d.mflow`: asserts the 3-D model loads, `has_scene3d`, the gated button shows, and clicking it generates `*.scene.html` (window suppressed under e2e). Full suite 42/42.
- [x] 6.4 Generic params editor for untyped blocks: the inspector surfaces `data.params` as editable rows for `Unknown` kinds (and any extra params on typed blocks not in their schema). Regression test added.

## 7. Typed 3-D scene blocks (authoring — was the deferred follow-up)

- [x] 7.1 Add six first-class `NodeKind` variants (`SignalWorld3D/Actor3D/Light3D/Camera3D/Sensor3D/Collision3D`) with serde tags, `display_name`, a new `NodeCategory::Signal3D` ("3-D Scene") in the signal-flow palette, ports (actor transform inputs; sensor/collision I/O), and port anchors.
- [x] 7.2 Param schemas (`SignalFlowParamSpec::fields`) per block matching the compiler's param names (shape/color/size, gravity/viewpoint/engine, light type, camera mode, sensor kind, collision radii) with enum dropdowns and number/integer validation.
- [x] 7.3 Tests: serde round-trip, category/palette membership, ports/anchors, param schemas; updated round-trip + integration tests now assert the 3-D blocks load typed (and keep the `Unknown` path for genuinely-unknown kinds). 475 core + 19 app + 7 integration green; e2e 42/42.
