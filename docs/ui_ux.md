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
  loop variable/iterable, signal-flow block parameters, state actions, …) and a
  **Toggle breakpoint** action for executable blocks.
* **Save** writes the `.mflow` back to disk; **Compile** lowers the chart to MATLAB
  via `matlabc -emit-matlab`, writes the generated `.m` beside it, and opens it in
  the editor.
