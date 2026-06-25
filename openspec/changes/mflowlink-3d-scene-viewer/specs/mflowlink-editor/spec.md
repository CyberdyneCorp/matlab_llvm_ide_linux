## ADDED Requirements

### Requirement: 3-D Scene action in the editor

The mflowLink editor SHALL expose a 3-D Scene action — from the flowchart Export menu and
the mflowLink window toolbar — that generates and opens the interactive 3-D scene for the
current model. The action SHALL be available only when the current model contains 3-D scene
blocks.

#### Scenario: Action offered for a 3-D model

- **WHEN** the user opens an mflowLink model that contains 3-D scene blocks
- **THEN** the Export menu and mflowLink toolbar present an enabled 3-D Scene action

#### Scenario: Action withheld for a non-3-D model

- **WHEN** the user opens an mflowLink model with no 3-D scene blocks
- **THEN** the 3-D Scene action is hidden or disabled

### Requirement: Lossless round-trip of untyped 3-D scene blocks

The editor SHALL open, persist, and re-save mflowLink models that contain 3-D scene blocks
(`signal_world3d`, `signal_actor3d`, `signal_light3d`, `signal_camera3d`,
`signal_sensor3d`, `signal_collision3d`) without dropping nodes or failing to load, even
though these block kinds are not yet typed in the IDE's node model. Unrecognized block kinds
and their parameters SHALL be preserved verbatim across load and save.

#### Scenario: Open a model containing 3-D scene blocks

- **WHEN** the user opens a `.mflow` that contains one or more 3-D scene blocks
- **THEN** the model loads successfully and every block — including the 3-D scene blocks —
  is present

#### Scenario: Re-save preserves 3-D scene blocks

- **WHEN** the user saves a model that was loaded with 3-D scene blocks
- **THEN** the saved `.mflow` still contains those blocks with their original kind and
  parameters unchanged
