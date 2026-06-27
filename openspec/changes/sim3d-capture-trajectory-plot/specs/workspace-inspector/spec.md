## ADDED Requirements

### Requirement: Plot a sim3d capture trajectory

The Workspace inspector SHALL offer a Plot Trajectory action for a workspace matrix variable that visualizes a `sim3d.capture` result. A capture matrix is `N`-by-(at least 4) holding `[t, x, y, z, …]` per frame. When invoked on such a variable, the action SHALL add a 2-D line figure of the X–Y ground track (the x and y position columns) to the Plots panel. When invoked on a variable that is not capture-shaped, the action SHALL report a status message and SHALL NOT add a figure.

#### Scenario: Capture matrix plots its X–Y path

- **WHEN** the user invokes Plot Trajectory on an `N`-by-7 `[t, x, y, z, rx, ry, rz]` workspace
  matrix
- **THEN** a 2-D line figure of the x column versus the y column is added to the Plots panel

#### Scenario: Non-capture variable is rejected

- **WHEN** the user invokes Plot Trajectory on a matrix with fewer than four columns (or a single
  frame)
- **THEN** no figure is added and a status message explains the expected capture shape
