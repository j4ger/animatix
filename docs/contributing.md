# Contributing to Animatix

## Quick Start

```bash
cargo build
cargo test
```

If you use Nix, `nix develop` sets up Rust, FFmpeg, Tree-sitter, and graphics dependencies.

---

## Development Workflows

### CLI Commands

```bash
# Inspect parsed AST
cargo run --bin animatix -- ast examples/showcase.amx
cargo run --bin animatix -- ast examples/showcase.amx --compact

# Live preview
cargo run --bin animatix -- render examples/showcase.amx
cargo run --bin animatix -- render examples/showcase.amx --loop

# Frame export
cargo run --bin animatix -- image examples/showcase.amx --time 1.5 --output frame.png

# Video/GIF export
cargo run --bin animatix -- video examples/showcase.amx --fps 30 --duration 5 --output demo.mp4
cargo run --bin animatix -- gif examples/showcase.amx --fps 15 --duration 5 --output out.gif

# GUI
cargo run --bin animatix-gui -- examples/showcase.amx
```

### Grammar/Tooling Validation

```bash
cd tree-sitter-animatix
tree-sitter generate
tree-sitter test
tree-sitter highlight ../examples/reactive_runtime.amx
```

### Recommended Validation Loop

For parser/language changes:
```bash
cargo run -- ast path/to/scene.amx
cargo test
```

For runtime/layout/rendering changes:
```bash
cargo run -- image path/to/scene.amx --time 0.0 --output /tmp/frame0.png
cargo run -- image path/to/scene.amx --time 1.5 --output /tmp/frame1.png
cargo run -- video path/to/scene.amx --output /tmp/check.mp4 --fps 30
cargo test
```

---

## Project Structure

```
crates/
├── animatix/              # Core library
│   └── src/
│       ├── ast.rs         # AST types
│       ├── parser.rs      # Chumsky parser
│       ├── diagnostics.rs # Diagnostic types
│       ├── module.rs      # Module system
│       ├── source_index.rs# Source location mapping
│       ├── to_source.rs   # AST re-serialization
│       └── timeline/      # Timeline compilation, actions, morphing, plotting
│
├── animatix-analyzer/     # Shared language intelligence
│   └── src/
│       ├── lib.rs         # Analyzer struct
│       ├── symbol_table.rs# Symbol extraction from AST
│       ├── completer.rs   # Context-aware completions
│       └── diagnostics.rs # Parse + semantic diagnostics
│
├── animatix-lsp/          # LSP server for external editors
│   └── src/
│       └── main.rs        # tower-lsp server
│
├── animatix-gui/          # Desktop GUI application
│   └── src/
│       ├── app.rs         # Main app shell + state model
│       ├── document.rs    # Document session management
│       ├── editor.rs      # Code editor with analyzer integration
│       ├── completion_popup.rs # Completion popup widget
│       ├── highlighting.rs# Tree-sitter highlighting + diagnostic squiggles
│       ├── hot_reload.rs  # File watcher
│       ├── preview_surface.rs # GPU render surface
│       ├── source_edit_v2.rs # AST-based source editing
│       └── app/           # Submodules
│           ├── runtime.rs       # eframe::App impl
│           ├── persistence.rs   # Workspace state persistence
│           ├── file_tree.rs     # File explorer
│           ├── transport_bar.rs # Playback controls
│           ├── inspector.rs     # Actor property inspector
│           ├── preview.rs       # Preview pane
│           ├── workspace.rs     # Dock layout management
│           ├── selection.rs     # Click-to-select
│           └── components/      # Reusable UI components
│               ├── context_menu.rs  # Unified right-click / floating menus
│               └── widgets.rs       # Low-level primitives (tree rows, tabs)
│
└── tree-sitter-animatix/  # Tree-sitter grammar
    ├── grammar.js
    ├── queries/highlights.scm
    └── src/parser.c
```

### Source Areas Worth Knowing

- `crates/animatix/src/main.rs` — CLI entrypoint
- `crates/animatix/src/parser.rs` — parser
- `crates/animatix/src/ast.rs` — AST types
- `crates/animatix/src/timeline/` — keyframed runtime, actions, morphing, plotting
- `crates/animatix/src/timeline/modifier_runtime/` — modifier IR and bytecode VM
- `crates/animatix/src/renderer/` — rendering backend
- `crates/animatix-gui/src/app.rs` — GUI shell state and event loop
- `tree-sitter-animatix/` — editor grammar

---

## GUI Data Flow

```
Editor → DocumentSession.set_source_text() → debounce → rebuild → Timeline
Timeline → frame_cache → PreviewSurface.render() → egui texture → Preview pane
Timeline.tracks → Inspector (read properties at current time)
Preview click → hit_regions → selected_actor → Inspector highlights
Preview drag → PropertyEdit → handle_property_edit → source + timeline + invalidate
```

Special-case edits (e.g. `child_order` on containers) bypass per-track dispatch and are handled directly in `handle_property_edit` / `handle_keyframe_edit` before the generic pipeline:
```
PropertyEdit { actor: container, property: "child_order", value: StringList }
  → update ContainerMetadata.child_order + rebuild layout_children
  → AST mutation: ReorderContainerChildren → reorder children block in source
```

### Key State Objects

- **DocumentSession**: file path, source text, dirty state, compiled AST, timeline, duration, scene dimensions
- **PreviewPaneState**: current time, playback state, dimensions, status/error
- **GuiShell**: document, editor, preview, dock state, hot reloader, selected actor, hit regions

---

## Analyzer Architecture

Language intelligence is shared between the GUI and LSP via `animatix-analyzer`:

```
animatix (core parser/AST)
    ↓
animatix-analyzer (pure computation, no I/O)
    ↓
animatix-gui (direct calls)    animatix-lsp (tower-lsp, JSON-RPC)
```

### Design Principles

1. **No I/O** — all functions take `&str` or `&[Stmt]`, return data
2. **Position-based API** — `(line, col)` matching LSP's `Position`
3. **Incremental** — `Analyzer::update()` re-parses only when source changes
4. **Dual parsers**: chumsky for semantic AST, tree-sitter for position queries

### LSP Capabilities

| Feature | LSP Method | Status |
|---------|------------|--------|
| Completions | `textDocument/completion` | ✅ |
| Hover | `textDocument/hover` | ✅ |
| Go-to-definition | `textDocument/definition` | ✅ |
| Document symbols | `textDocument/documentSymbol` | ✅ |
| Diagnostics | Published on change | ✅ |

### Editor Configuration

**VS Code** (`.vscode/settings.json`):
```json
{
  "amx.languageServer": {
    "command": "animatix-lsp",
    "args": []
  }
}
```

**Neovim** (nvim-lspconfig):
```lua
require('lspconfig').animatix.setup {
  cmd = { 'animatix-lsp' },
  filetypes = { 'amx' },
  root_dir = require('lspconfig').util.root_pattern('.git', 'Cargo.toml'),
}
```

---

## Error Model

| Class | Example | UI Treatment |
|-------|---------|--------------|
| File load | Missing file | Red banner in preview |
| Parse | Syntax error | Red squiggles in editor |
| Semantic | Unknown action/label | Yellow squiggles |
| Info | Unknown property for type | Blue squiggles |
| Build | Timeline build failure | Amber banner |
| Render | GPU error | Red overlay on preview |

### Diagnostic Sources

1. **Tree-sitter** — syntax errors (ERROR/MISSING nodes)
2. **Chumsky** — parse errors (detailed messages)
3. **Semantic checks** — unknown actions, undefined labels, unknown types/properties

---

## Testing

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

- **animatix**: Core timeline, parser, and rendering tests
- **animatix-analyzer**: 18+ unit tests (symbol extraction, completions, diagnostics)
- **animatix-lsp**: Compiles, manual testing with editors
- **animatix-gui**: Integrated with workspace tests

For demo work: keep runnable demos under `examples/`, verify with both `ast` and `image`/`video`.

---

## GUI Actor Creation

Users can create actors from the GUI via toolbar `+`, inspector CTA, or right-click canvas. New actors are inserted into the current keyframe block (or wrapped in `#0s` if none exist).

Supported types: Rect, Ellipse, Line, Polygon, Path, Text, Row, Col. Code-only types (Math, Image, Svg, plots) are not creatable from GUI.

To add GUI creation support for a new primitive:
1. Ensure it implements `Primitive::default_props()` in `primitives/<name>.rs`.
2. Add the `SourceEdit::InsertActor` handling in `source_edit.rs` if new behavior is needed.
3. Wire the palette entry in `app/shell/toolbar.rs`.

---

## GUI Widget Screenshot Harness

Renders isolated GUI components as PNG images for visual inspection. Useful for iterating on UI layout, spacing, and alignment without launching the full application.

The harness is gated behind the `dev-screenshots` Cargo feature and is **never compiled into shipped binaries**.

```bash
# List available widgets
cargo run --bin widget-screenshot --features dev-screenshots -- --list

# Render a specific widget
cargo run --bin widget-screenshot --features dev-screenshots \
  -- --widget property-row-float --output /tmp/out.png

# Custom size (default is 480×120)
cargo run --bin widget-screenshot --features dev-screenshots \
  -- --widget card --width 400 --height 200 --output /tmp/card.png
```

**Available widgets:** `property-row-vec2`, `property-row-float`, `property-row-slider`, `property-row-color`, `property-row-text`, `property-group`, `card`, `field`, `section-header`, `row`, `icon-button`, `empty-state`.

**Adding a new widget:** Add a demo function to `crates/animatix-gui/src/dev/screenshot_harness.rs`, register it in `WIDGET_REGISTRY`, and wire it in `render_widget()`.

---

---

## Commit Messages

This project uses [Conventional Commits](https://www.conventionalcommits.org/) enforced by [Cocogitto](https://docs.cocogitto.io/).

### Quick Reference

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

**Scopes (common):** `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`

### Examples

```bash
# Feature in the GUI
cog commit feat "add timeline scrubbing" gui

# Bug fix in the parser
cog commit fix "handle empty keyframes" parser

# Breaking change
cog commit fix -B "drop support for old scene format" animatix

# Simple docs update
cog commit docs "document timeline evaluation order"
```

### Validation

```bash
# Check all commits since the last tag
cog check

# Check a specific range
cog check --from <commit-sha>
```

CI enforces conventional commits on every pull request.

---

*For the language specification, see [`spec.md`](spec.md). For the system architecture, see [`architecture.md`](architecture.md). For work items, see [`roadmap.md`](roadmap.md).
