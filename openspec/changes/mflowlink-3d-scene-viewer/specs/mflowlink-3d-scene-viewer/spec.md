## ADDED Requirements

### Requirement: Generate the Babylon 3-D scene from an mflowLink model

The IDE SHALL generate an interactive 3-D scene for an mflowLink model by invoking the
configured `matlabc` with the `-emit-mflowlink-babylon` codegen lane, producing a single
self-contained HTML artifact. The IDE SHALL request the compiler's inline/self-contained
output so the artifact embeds the Babylon runtime and requires no network/CDN access at
view time.

#### Scenario: Successful scene generation

- **WHEN** the user triggers the 3-D Scene action on a model whose `.mflow` contains 3-D
  scene blocks and `matlabc` exits successfully
- **THEN** the IDE writes the generated self-contained HTML artifact to disk and proceeds
  to display it

#### Scenario: Compiler reports an error

- **WHEN** `matlabc -emit-mflowlink-babylon` exits non-zero
- **THEN** the IDE surfaces the compiler's stderr in the console and does not open a
  3-D Scene window

#### Scenario: Compiler not configured

- **WHEN** the 3-D Scene action is triggered but the configured `matlabc` path does not exist
- **THEN** the IDE reports that `matlabc` is not found and does not open a 3-D Scene window

### Requirement: Render the 3-D scene in an embedded interactive window

The IDE SHALL render the generated scene HTML inside an embedded WebKitGTK 6.0 `WebView`
hosted in a dedicated in-IDE window. The window SHALL support the interactions provided by
the Babylon viewer — orbiting, zooming, panning, and timeline play/scrub — without leaving
the IDE.

#### Scenario: Open and interact with the scene

- **WHEN** scene generation succeeds
- **THEN** the IDE opens a 3-D Scene window that loads the generated HTML and lets the user
  orbit, zoom, and pan the scene and play/scrub its timeline

#### Scenario: Offline rendering

- **WHEN** the 3-D Scene window renders a scene generated with the inline/self-contained
  option and the machine has no network access
- **THEN** the scene still renders and remains fully interactive

#### Scenario: Closing the window releases resources

- **WHEN** the user closes the 3-D Scene window
- **THEN** the embedded WebView and its window are disposed and no orphaned process or
  view remains

### Requirement: Gate the 3-D Scene action on scene presence

The IDE SHALL offer the 3-D Scene action only for models that actually contain 3-D scene
blocks. Detection SHALL scan the persisted `.mflow` for the 3-D scene block kinds
(`signal_world3d`, `signal_actor3d`, `signal_light3d`, `signal_camera3d`,
`signal_sensor3d`, `signal_collision3d`) rather than relying on the IDE's typed node model.

#### Scenario: Model contains 3-D scene blocks

- **WHEN** the open model's `.mflow` contains at least one 3-D scene block kind
- **THEN** the 3-D Scene action is enabled

#### Scenario: Model has no 3-D scene blocks

- **WHEN** the open model's `.mflow` contains no 3-D scene block kinds
- **THEN** the 3-D Scene action is hidden or disabled
