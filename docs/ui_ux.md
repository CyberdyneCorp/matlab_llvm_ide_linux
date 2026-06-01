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
* **Left sidebar** (220 px): Explorer file tree (folders expand/collapse; files
  open in the editor, classified + colored by kind).
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
