## Why

The matlab_llvm compiler now emits a fully interactive 3-D scene from mflowLink models
(`matlabc -emit-mflowlink-babylon` → a self-contained Babylon.js HTML with a scene graph,
a per-step transform timeline, and built-in orbit/zoom/pan/play/scrub, viewer-side physics,
sensors, and URDF joints). Today the IDE can only show a flat Cairo x–y projection of the
`signal_scope3d` trajectory columns — it cannot show the actual 3-D scene. Users want to
view, orbit, zoom, and pan the real 3-D scene inside the IDE, the same way they already
view 2-D scopes.

## What Changes

- Add an in-IDE **3-D Scene** window that renders the compiler's Babylon.js HTML inside an
  embedded WebKitGTK 6.0 `WebView`, giving orbit / zoom / pan / play / scrub and full feature
  parity with the standalone viewer (because the viewer *is* the compiler's HTML).
- Add a `matlabc -emit-mflowlink-babylon` codegen lane that produces the scene HTML, generated
  with the compiler's inline/self-contained option so the embedded WebView renders **offline**
  (no CDN/network dependency).
- Surface a **3-D Scene** action in the mflowLink window toolbar and the flowchart **Export**
  menu. The action is enabled only when the open model actually contains 3-D scene blocks.
- Detect 3-D scene blocks (`signal_world3d`, `signal_actor3d`, `signal_light3d`,
  `signal_camera3d`, `signal_sensor3d`, `signal_collision3d`) by scanning the persisted
  `.mflow`, since these block kinds are not yet typed in the IDE's node model.
- Ensure opening a `.mflow` that contains these (currently untyped) 3-D blocks does **not**
  break the editor — round-trip them losslessly via an `Unknown` node fallback if needed.
- New build/runtime dependency on **WebKitGTK 6.0** (`webkit6` crate + `libwebkitgtk-6.0`),
  reflected in the Debian package `depends` and the build documentation.

Out of scope (follow-up): authoring the `signal_*3d` scene-graph blocks in the IDE palette /
`NodeKind`. This change delivers the **viewer** only.

## Capabilities

### New Capabilities
- `mflowlink-3d-scene-viewer`: Generating the Babylon 3-D scene HTML from an mflowLink model
  and rendering it in an embedded, interactive in-IDE window (orbit/zoom/pan/play), including
  detection/gating of 3-D scene models and offline self-contained rendering.

### Modified Capabilities
- `mflowlink-editor`: The flowchart Export menu and mflowLink toolbar gain a 3-D Scene action,
  and the editor must round-trip currently-untyped 3-D scene blocks without data loss.

## Impact

- **New code**: `crates/app/src/scene3d_window.rs` (GTK `Window` + `webkit6::WebView`); a
  3-D-scene-block detection helper and a new `ExportTarget` lane in
  `crates/core/src/services/codegen.rs`.
- **Modified code**: `crates/app/src/flowchart_view.rs` (Export menu / `emit_artifact`),
  `crates/app/src/mflowlink_window.rs` (toolbar action), `crates/core/src/models/flowchart/node.rs`
  (unknown-node round-trip if required).
- **Dependencies**: adds `webkit6` (Cargo) and `libwebkitgtk-6.0-dev` (build) /
  `libwebkitgtk-6.0-4` (runtime); updates `crates/app/Cargo.toml` Debian `depends` and build docs.
  Currently only the GTK3 `webkit2gtk-4.1` runtime is present, so `webkitgtk-6.0` must be installed.
- **Compiler dependency**: requires a `matlabc` that supports `-emit-mflowlink-babylon` and its
  inline flag.
- **Testing**: core unit tests for the new export lane metadata and the detection helper; a
  regression test that a 3-D-block `.mflow` round-trips. The WebKitGTK render is integration-level
  and not unit-tested headlessly.
