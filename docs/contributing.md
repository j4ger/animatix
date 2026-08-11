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
cargo run --bin animatix -- ast examples/animation/16_showcase.amx
cargo run --bin animatix -- ast examples/animation/16_showcase.amx --compact

# Live preview
cargo run --bin animatix -- render examples/animation/16_showcase.amx
cargo run --bin animatix -- render examples/animation/16_showcase.amx --loop

# Frame export
cargo run --bin animatix -- image examples/animation/16_showcase.amx --time 1.5 --output frame.png

# Video/GIF export (requires the `video` feature, see AGENTS.md)
cargo run --features animatix/video --bin animatix -- video examples/animation/16_showcase.amx --fps 30 --duration 5 --output demo.mp4
cargo run --features animatix/video --bin animatix -- gif examples/animation/16_showcase.amx --fps 15 --duration 5 --output out.gif

# GUI
cargo run --bin animatix-gui -- examples/animation/16_showcase.amx
```

### Grammar/Tooling Validation

```bash
cd tree-sitter-animatix
tree-sitter generate
tree-sitter test
tree-sitter highlight ../examples/animation/06_reactive.amx
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
# Video export requires the `video` feature (see AGENTS.md)
cargo run --features animatix/video -- video path/to/scene.amx --output /tmp/check.mp4 --fps 30
cargo test
```

---

## Project Structure

```
crates/
├── animatix-syntax/       # Syntax layer (parser, AST, module system, typing)
├── animatix/              # Runtime engine (timeline, renderer, primitives)
├── animatix-analyzer/     # Shared language intelligence
├── animatix-lsp/          # LSP server (tower-lsp)
├── animatix-gui/          # Desktop GUI (eframe/egui)
├── eparts/                # Themed egui widget framework
└── tree-sitter-animatix/  # Tree-sitter grammar
```

### Source Areas Worth Knowing

- `crates/animatix/src/main.rs` — CLI entrypoint
- `crates/animatix-syntax/src/parser/` — Chumsky parser (split into submodules)
- `crates/animatix-syntax/src/ast.rs` — AST types
- `crates/animatix-syntax/src/typing.rs` — shared `Type`/`TypeEnv` inference
- `crates/animatix-syntax/src/walk.rs` — shared AST traversal primitives
- `crates/animatix/src/timeline/` — keyframed runtime, actions, morphing, plotting
- `crates/animatix/src/renderer/` — Vello/WGPU rendering backend
- `crates/animatix/src/primitives/` — actor primitive system
- `crates/animatix-gui/src/app/mod.rs` — GUI shell state and event loop
- `crates/animatix-gui/src/app/panels/` — UI panels (inspector, timeline, sidebar, preview, editor)
- `crates/animatix-gui/src/app/commands/mod.rs` — command system (ShellAction / Command / ViewAction)
- `crates/animatix-gui/src/app/design_tokens/mod.rs` — GUI design token system
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
4. **Canonical parser**: `parse_canonical`/`reparse_canonical` produce the AST via the tree-sitter CST converter and fall back to chumsky; analyzer and module loading should not select a parser backend directly.

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
- **animatix-analyzer**: 48+ unit tests (symbol extraction, type inference, completions, diagnostics)
- **animatix-lsp**: Compiles, manual testing with editors
- **animatix-gui**: Integrated with workspace tests

For demo work: keep runnable demos under `examples/`, verify with both `ast` and `image`/`video`. Use `dogfood/` for in-progress real-content projects and grammar probes; do not move known-broken probes into `examples/`.

---

## Documentation Validation

```bash
bash scripts/check-docs.sh
```

This checks relative Markdown links under `docs/`, rejects completed-status rows still listed as active roadmap work, and fails on stale known-gap wording for completed roadmap items. CI runs it in the `doc` job.

---

## GUI Actor Creation

Users can create actors from the GUI via toolbar `+`, inspector CTA, or right-click canvas. New actors are inserted into the current keyframe block (or wrapped in `#0s` if none exist).

Supported types: Rect, Ellipse, Line, Polygon, Path, Text, Row, Col. Code-only types (Math, Image, Svg, plots) are not creatable from GUI.

To add GUI creation support for a new primitive:
1. Ensure it implements `Primitive::default_props()` in `primitives/<name>.rs`.
2. Add the `SourceEdit::InsertActor` handling in `source_edit/actor_edits.rs` if new behavior is needed.
3. Wire the palette entry in `app/shell/toolbar.rs`.

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
