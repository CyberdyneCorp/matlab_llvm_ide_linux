# UI / UX specification

The Linux IDE reproduces the macOS reference's dark, multi-pane layout. Colors,
metrics, and fonts are ported verbatim from `Theme.swift` into
[`theme.rs`](../crates/core/src/theme.rs) (shared by the Cairo renderers) and
[`theme.css`](../crates/app/resources/theme.css) (GTK chrome).

## Window layout

```
┌───────────────────────────────────────────────────────────────────┐
│ TOOLBAR  ⬣ MatForge IDE                                            │
│          Open Folder · Save │ Target ▾ · Opt ▾ │ Compile · ▶ Run   │
├────┬──────────────┬─────────────────────────────┬─────────────────┤
│ A  │   EXPLORER    │  EDITOR (tabs)              │   WORKSPACE     │
│ C  │  (file tree)  │  ───────────────────────    │  (variables)    │
│ T  │               │  CONSOLE / PROBLEMS /       │                 │
│ B  │               │  artifact tabs              │                 │
│ A  │               │  >> REPL input              │                 │
│ R  │               │                             │                 │
├────┴──────────────┴─────────────────────────────┴─────────────────┤
│ STATUS  Ln, Col │ message │ language │ encoding                    │
└───────────────────────────────────────────────────────────────────┘
```

* **Activity bar** (56 px): Explorer · Search · Run · Compiler · HDL · Debug ·
  Docs · Flowchart.
* **Left sidebar** (220 px): a `GtkStack` switched by the activity bar — the
  Explorer file tree (folders expand/collapse; files open in the editor,
  classified + colored by kind), the Search (find-in-files) panel, and the Debug
  panel.
* **Center**: editor tab notebook over a bottom console. The console tab strip is
  CONSOLE + PROBLEMS, and grows an artifact tab (LLVM IR / C++ / Python / … ) as
  each compile target is produced. A `>>` REPL input sits at the bottom.
* **Workspace** (380 px): the `whos` variable table (Name · Size · Class).
* **Status bar** (22 px): cursor position, status message, language, encoding.

## Appearance, personalization & session

Theming is data-driven and swappable at runtime. A `ThemeTokens` struct
([`theme.rs`](../crates/core/src/theme.rs)) resolves every UI color; three
built-in themes ship as data — **Midnight** (dark, the default), **Daylight**
(light), and **High Contrast** — recolorable by an **Accent** hue. The
`AppearanceViewModel` holds the active theme / accent / UI font scale / code
font; on any change [`theme_css::render`](../crates/app/src/theme_css.rs)
regenerates the stylesheet from the tokens (a generated `@define-color` block +
scaled fonts) and a swappable `CssProvider` reloads it instantly. The Cairo
renderers (plots, flowchart, gutter) read the same tokens, so the whole surface
re-themes together — no restart.

- **Preferences** dialog ([`settings_view.rs`](../crates/app/src/settings_view.rs))
  via `Edit ▸ Preferences…` / `Ctrl+,` — theme, accent, font size, code font,
  and the resolved toolchain paths.
- **Font zoom** — `Ctrl+=` / `Ctrl+−` / `Ctrl+0` and `Ctrl+scroll`.
- **Focus mode** (`Ctrl+Shift+F`, `View ▸ Focus Mode`) hides the activity bar,
  sidebar, and right panels for distraction-free work.
- **Persistence + session restore** — appearance, panel visibility, the open
  tabs, last folder, and recent folders are saved to
  `~/.config/matforge/config.toml` (a tested [`Preferences`](../crates/core/src/services/preferences.rs))
  and restored on the next launch.
- **Welcome screen** — shown when nothing is open: New / Open actions, recent
  folders, and example models.

## Palette (from `Theme.Palette` / `Theme.Code`)

| Role | Hex | Role | Hex |
|------|-----|------|-----|
| window background | `#121A26` | accent orange | `#E08A45` |
| chrome | `#16202E` | accent green (Run) | `#5EBE6E` |
| editor background | `#131C2A` | accent blue (Debug) | `#4FA3E3` |
| panel | `#1A2434` | accent red (Stop/error) | `#E05B5B` |
| border | `#2A3A52` | accent magenta (flow) | `#C678DD` |
| text primary | `#D3DCEA` | keyword | `#C678DD` |
| text secondary | `#8898AE` | string | `#E0A06A` |

## Syntax highlighting

The editor applies one `GtkTextTag` per token color, computed by the pure
[`highlighter`](../crates/core/src/services/highlighter.rs) service (MATLAB, C,
C++, Python, TypeScript, Verilog/Verilog-A, LLVM IR, MLIR). Keywords render
magenta, builtins/calls blue, strings amber, comments muted, numbers green —
matching the reference's `Theme.Code` colors exactly.

## Menu bar & keyboard shortcuts

A `GtkPopoverMenuBar` above the toolbar mirrors the macOS reference's menus,
driven by `win.*` `GSimpleAction`s registered on the window (see
[`build_menu_bar`](../crates/app/src/ui.rs)). Accelerators are bound on the
`GtkApplication` and shown inline in the menus.

| Menu | Item | Shortcut |
|------|------|----------|
| File | New File | `Ctrl+N` |
| File | Open Folder… | `Ctrl+O` |
| File | Save | `Ctrl+S` |
| File | Close Tab | `Ctrl+W` |
| File | Quit | `Ctrl+Q` |
| Edit | Undo / Redo | `Ctrl+Z` / `Ctrl+Shift+Z` (built-in text view) |
| Edit | Cut / Copy / Paste / Select All | standard text-view actions |
| Edit | Search in Files | `Ctrl+F` |
| View | Toggle Sidebar | `Ctrl+B` |
| View | Toggle Workspace | `Ctrl+Shift+W` |
| View | Toggle Plots | `Ctrl+Shift+P` |
| Run | Compile | `Ctrl+Shift+B` |
| Run | Run | `Ctrl+R` |
| Run | Stop | `Shift+F5` |
| Debug | Start Debugging | `F5` |
| Debug | Continue | `F8` |
| Debug | Step Over / Into / Out | `F10` / `F11` / `Shift+F11` |
| Help | About | — |

Toggling a breakpoint stays on `F9` in the focused editor (handled by the code
view, not the menu) to match the gutter-click affordance.

## Search panel (find in files)

The activity bar's **Search** entry (or `Ctrl+F`) shows the find-in-files panel,
backed by the tested [`SearchViewModel`](../crates/core/src/viewmodels/search.rs).
It offers a query field, a match-mode selector (**File names** / **In files** /
**Both**), a result count, and a result list. Each result shows `file:line` over a
trimmed preview; clicking it opens the file and jumps to the line (reusing the
PROBLEMS-pane goto path). The walk descends subfolders and skips dot-entries.

## Compiler panel

The activity bar's **Compiler** entry shows a build panel backed by the shared
`ToolbarViewModel` (so it stays in lock-step with the top toolbar's pickers). It
has a **build-state badge** (IDLE / BUILDING / READY / FAILED, fed by
`is_compiling` + `last_build`), a **SOURCE** line that names the active file and
warns when it is unsaved, a **TARGET** picker that prints the resolved
`matlabc` emit flag (e.g. `-emit-cpp`, or "(runs program, captures .va)" for
Verilog-A), **OPTIONS** (optimization + numeric-mode pickers), and a **Compile**
action that is enabled only for a saved file.

## Command-window mode

When the center notebook has nothing open (no source tab and no flowchart), the
editor is hidden and the console — the MATLAB command window / REPL workspace —
fills the center, matching the reference's "everything is a REPL" feel. Opening a
file or flowchart restores the editor with the console docked at the bottom.

## Flowchart editor

Opening a `.mflow` (or the demo charts) shows a three-pane editor:
[`flowchart_view`](../crates/app/src/flowchart_view.rs) renders the document on a
Cairo canvas between a block palette and a property inspector. All edits go
through the tested [`FlowchartViewModel`](../crates/core/src/viewmodels/flowchart.rs).

* **Palette** (left): a **Save** / **Compile** action row, the dialect-appropriate
  block list (click to drop a node), and **undo / redo / delete** controls.
* **Canvas** (center): pan-free but **zoom-to-fit on open** (and a **Fit** button)
  always frames the chart; scroll to zoom, drag a node body to move it, and drag
  from a node's output port to another node to draw a control edge (a dashed
  rubber band follows the cursor and snaps to the target's nearest input port).
* **Inspector** (right): edits the selected block — its label plus the fields that
  matter for its kind (assignment target/expression, `if`/`while` condition, `for`
  loop variable/iterable, signal-flow block parameters, …) and a
  **Toggle breakpoint** action for executable blocks. For a **state** block the
  inspector shows four multi-line, MATLAB-highlighted action editors — `entry`,
  `during`, `exit`, and `on event` (one `EVENT: code` line per event) — each
  lint-checked for balanced brackets; an unbalanced snippet is flagged inline and
  with a red dashed halo on the state in the canvas.
* **State hierarchy** (state charts): drag a state onto another to **reparent** it
  (drops into a state's own descendant are rejected — no cycles), and compound
  states **autosize** to wrap their children with a titled header. The inspector
  exposes **decomposition** (OR exclusive / AND parallel), a **history junction**
  toggle, and an **execution order** for AND siblings; AND children show numbered
  order badges and a history "H" badge renders on the compound. A hierarchy lint
  (history-on-AND, duplicate execution order) is flagged inline and as a red
  dashed halo on the offending state.
* **Transitions…** (state charts): opens the **state-transition table** — a
  tabular alternative to drawing transitions, one row per transition with
  source × dest × event × guard × cond-action × trans-action × priority. Edits
  write straight back to the chart edges (ids preserved), so the table and canvas
  stay in sync.
* **Subsystems** (signal flow): **double-click** a `signal_subsystem` block to
  descend into its sub-flow; a **breadcrumb** bar (with an **Up** action and a
  clickable crumb per level) tracks where you are and navigates back. Right-click
  a block ▸ **Extract to Subsystem** moves it into a fresh sub-flow, rerouting its
  wires through inport / outport blocks and leaving a linked subsystem node in its
  place.
* **Save** writes the `.mflow` back to disk; **Compile** lowers the chart to MATLAB
  via `matlabc -emit-matlab`, writes the generated `.m` beside it, and opens it in
  the editor.
* **Export ▾** surfaces the compiler's codegen lanes — `-emit-matlab`,
  `-dump-chart`, `-emit-c`, `-emit-cpp`, `-emit-llvm`, `-emit-systemverilog` —
  each writing its artifact beside the model and opening it.
* **Preview** toggles a docked, syntax-highlighted **live `-emit-matlab` preview**
  pane that re-runs the generator (debounced ~500 ms) as you edit the chart.

## mStateflow runner

Running a state chart (`▶ Run Chart`) opens a live window
([`statechart_window`](../crates/app/src/statechart_window.rs)) backed by the
tested [`StateChartViewModel`](../crates/core/src/viewmodels/statechart.rs):

* **Chart canvas** (left): every currently-active state gets a green halo;
  clicking an event-log row reveals that state with a yellow halo.
* **Active-state pane** (top-right): the state hierarchy as an indented tree with
  a live ●/○ active marker and an OR/AND decomposition badge per compound.
* **Event log** (bottom-right): one row per chart event prefixed with its
  super-step index (`[i] → enter S`), clickable to reveal the state on the canvas,
  with a **⭳ CSV** export (`step,kind,detail`) written beside the model file.

## mflowLink scope

Simulating a signal-flow model (`▶ Simulate`) opens a window
([`mflowlink_window`](../crates/app/src/mflowlink_window.rs)) with the model
canvas beside a production **overlay scope** backed by the streamed `SimTrace`:

* **Overlay + legend** — every logged signal shares one set of axes with a
  legend and stable per-signal colors (`services/scope.rs`).
* **Crosshair** — hovering shows a dashed crosshair and a readout box with the
  cursor time and each signal's nearest-sample value.
* **Box-zoom / pan / scale** — left-drag a box to zoom, middle-drag to pan,
  **Auto** to autoscale, or pin a manual **Y min / Y max**.
* **Export** — **CSV** writes the *visible* trace (the pinned time window) and
  **PNG** writes the rendered scope, both beside the model file.
