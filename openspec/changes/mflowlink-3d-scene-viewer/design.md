## Context

The IDE is a Rust + GTK4 + Cairo desktop app (`crates/core` = GTK-free MVVM, `crates/app` =
GTK layer). It drives the `matlab_llvm` compiler (`matlabc`) over `std::process::Command`
(`crates/app/src/process.rs`), with the binary path in `app.settings.matlabc_path`.

`matlabc` now supports `-emit-mflowlink-babylon`, which records the simulation and emits a
single self-contained Babylon.js HTML: a scene graph (actors/lights/cameras) plus a per-step
transform timeline, with orbit/zoom/pan/play/scrub, viewer-side physics, sensors, and URDF
joints all implemented in the HTML's inline JavaScript. The scene is authored with the new
`signal_world3d` / `signal_actor3d` / `signal_light3d` / `signal_camera3d` / `signal_sensor3d`
/ `signal_collision3d` block family.

Current IDE limits:
- It can only draw a flat Cairo x–y projection of the older `signal_scope3d` trajectory
  columns — no real 3-D scene.
- `NodeKind` (`crates/core/src/models/flowchart/node.rs`) is an externally-tagged enum with
  **no catch-all**, and the codec (`crates/core/src/services/flowchart_codec.rs:42`) does a
  single hard `serde_json::from_str`. A `.mflow` containing the untyped 3-D scene blocks
  therefore fails to load entirely. `FlowNode.data` is a generic field-bag, so block
  parameters already round-trip; only the `kind` discriminant lacks a lossless fallback.
- The GTK4 build has no embedded browser. Only the GTK3 `webkit2gtk-4.1` runtime is installed;
  the GTK4-compatible `webkitgtk-6.0` is not.

## Goals / Non-Goals

**Goals:**
- Render the compiler's real 3-D scene inside an in-IDE window with orbit/zoom/pan/play.
- Reuse the compiler's HTML viewer verbatim so feature parity (physics, sensors, URDF,
  cameras, recording) is automatic.
- Render offline — no CDN/network dependency at view time.
- Open/save 3-D `.mflow` models without data loss, despite untyped blocks.

**Non-Goals:**
- Authoring `signal_*3d` scene-graph blocks in the IDE palette / `NodeKind` (follow-up).
- A native GTK/OpenGL 3-D renderer.
- Live/streaming 3-D during simulation — the compiler's scene is a recorded, deterministic
  timeline; this change displays that recording.

## Decisions

### D1 — Embed the compiler's Babylon HTML in a WebKitGTK 6.0 WebView

Render by loading the generated HTML into a `webkit6::WebView` hosted in a `Window::builder()`
secondary window (`crates/app/src/scene3d_window.rs`), following the existing
`statechart_window.rs` / `mflowlink_window.rs` pattern. The window loads the artifact via a
`file://` URI.

- **Why:** The compiler already ships a complete, interactive viewer; embedding it gives full
  parity for ~one window of glue code. Babylon's camera provides orbit/zoom/pan/play natively.
- **Alternatives:** (a) Native `gtk::GLArea` + custom OpenGL renderer — re-implements a 3-D
  engine, won't match the compiler's output, no mesh/URDF/sensors; rejected as months of work
  for an inferior result. (b) Open in the system browser via `xdg-open` — trivial but not
  "in a window in the IDE", which is the explicit requirement; kept only as an implicit
  fallback if the WebView fails to initialize.

### D2 — Generate with the compiler's inline/self-contained option

Run `matlabc -emit-mflowlink-babylon` with the inline flag so the Babylon runtime is embedded
in the HTML rather than referenced from a CDN.

- **Why:** The embedded WebView must render with no network access (offline dev machines, CI,
  air-gapped use). This matches the compiler's `--babylon-inline` design intent.

### D3 — New `ExportTarget` lane + a dedicated open path

Add a Babylon lane to `ExportTarget` (`crates/core/src/services/codegen.rs`) so the flag and
`.html` extension live with the other codegen metadata and are unit-tested alongside them.
Because the 3-D Scene action both generates *and* opens a window (unlike the text artifacts
that `emit_artifact()` opens in the editor), the GTK layer adds a small `open_scene3d()` path
that runs the lane to an output `.html`, then hands it to `scene3d_window.rs`.

- **Why:** Reuses the established export metadata/test pattern while keeping the
  open-in-window behavior separate from the open-in-editor behavior.

### D4 — Detect 3-D models by scanning the persisted `.mflow`

Add a pure helper in `crates/core` that scans the serialized `.mflow` (or the loaded
document's raw kinds) for the six 3-D scene block kind strings and returns whether the model
is a 3-D scene. The toolbar/Export action is gated on this.

- **Why:** The IDE doesn't type these blocks, so a string-level scan is authoritative and
  testable without expanding `NodeKind`. Keeping it in `core` makes it unit-testable headlessly.

### D5 — Lossless round-trip via a `NodeKind::Unknown(String)` catch-all

Add an `Unknown(String)` variant to `NodeKind` with custom `Serialize`/`Deserialize`: unknown
kind strings deserialize into `Unknown(s)` and serialize back to the original `s`; known kinds
are unchanged. Combined with the existing generic `FlowNode.data` bag, this makes 3-D blocks
round-trip byte-for-byte. Unknown blocks render with a neutral default category/shape/ports in
the editor.

- **Why:** Without this, opening any 3-D `.mflow` fails outright, so the in-editor 3-D Scene
  action would be unreachable. `#[serde(other)]` alone is insufficient — it yields a unit
  variant that discards the tag string and breaks lossless save.
- **Alternative:** Make the codec skip unparseable nodes — rejected: silently drops blocks and
  corrupts the model on save.

## Risks / Trade-offs

- **New system dependency (`webkitgtk-6.0`)** → Gate it as cleanly as possible: update the
  Debian `depends` in `crates/app/Cargo.toml` and the build docs; document the
  `libwebkitgtk-6.0-dev` build requirement. Document graceful degradation if the WebView
  can't be created (report to console, optionally fall back to system browser).
- **Compiler must support the babylon lane** → If `matlabc` lacks `-emit-mflowlink-babylon`,
  surface its stderr cleanly rather than crashing.
- **WebKitGTK render is hard to unit-test headlessly** → Cover the testable seams in `core`
  (export-lane metadata, 3-D detection, `Unknown` round-trip); treat the actual render as a
  manual/integration check.
- **`NodeKind::Unknown` touches a widely-matched enum** → Many `match` arms over `NodeKind`
  must handle the new variant; rely on the compiler's exhaustiveness checks and add a neutral
  default for category/shape/ports so unknown blocks display safely.
- **Temp/output HTML lifetime** → Write the generated `.html` next to the model (or a temp dir)
  and ensure the WebView keeps a valid `file://` path for the window's lifetime.

## Migration Plan

1. Land `core` changes (export lane, detection helper, `Unknown` round-trip) with unit tests —
   no behavior change to existing models.
2. Add `webkit6` dependency and `scene3d_window.rs`; wire the gated toolbar/Export action.
3. Update Debian `depends` and build docs.
4. Rollback: revert the GTK wiring + dependency; the `core` `Unknown` round-trip is safe to
   keep on its own (pure robustness improvement).

## Open Questions

- Exact compiler flag spelling for the inline/self-contained option (`--babylon-inline` vs an
  equivalent) — confirm against the installed `matlabc`.
- Where to write the generated `.html` (alongside the model vs a temp dir) and whether to cache
  by model mtime to avoid regenerating on every open.
