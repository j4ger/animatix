# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## 1. Code Health & Maintainability

### 1.1 Generic AST Walker for source_edit.rs

`source_edit.rs` (1,884 lines) contains half a dozen near-identical tree walks:
`find_actor_decl_mut`, `find_assignment_mut`, `extract_inline_item`, `rename_in_stmt`, etc.
Build a generic `AstWalker` trait or macro so each operation only declares *what* it wants to find, not *how* to recurse.

**Key files:** `source_edit.rs`

---

### 1.2 Migrate deprecated `tree_row` → `components::Row`

`app/components/widgets.rs::tree_row` is a deprecated duplicate of `components::Row`.
Migrate remaining callers and delete the file.

**Key files:** `app/components/widgets.rs`, callers in `panels/` and `shell/`

---

### 1.3 Split `editor.rs` mixed responsibilities

`editor.rs` (~700 lines) currently holds cell editor, completion popup, diagnostics, and timeline sync. Split into:
- `editor/core.rs` — buffer + cells
- `editor/completion.rs` — completion popup logic
- `editor/diagnostics.rs` — diagnostic mapping + scrolling

**Key files:** `editor.rs` → `editor/`

---

### 1.4 Unify badge rendering

Badge rendering is inlined in `preview/mod.rs` (target-index badge) and `inspector/mod.rs` (index badge). Both should use `utils::badge()` or `components::badge_button()`.

**Key files:** `app/preview/mod.rs`, `app/panels/inspector/mod.rs`

---

## 2. Visual Polish & Tooling (P3)

### 2.1 Design Token System

Most colors are now tokenized in `theme.rs` (base colors, alpha-tinted variants, backgrounds, semantic colors). Remaining work:

- Formalize token groups with generated documentation.
- Add `lint_theme.py` to catch regressions.
- Rename `theme.rs` → `design_tokens.rs` for clarity.

Token groups:
- `color`: `SURFACE_BASE`, `ELEVATED`, `WIDGET`, `TEXT_PRIMARY`, `TEXT_SECONDARY`, `ACCENT`, `SUCCESS`, `WARNING`, `ERROR`
- `spacing`: `XS`, `S`, `M`, `L`, `XL`, `XXL`
- `radius`: `NONE`, `SM`, `MD`, `LG`, `FULL`
- `typography`: `H1`, `H2`, `BODY`, `CAPTION`, `MONO`

**Key files:** `app/theme.rs` → `app/design_tokens.rs`

---

### 2.2 Cell Editor Visual Redesign

- Accent left border on focus.
- Large, prominent keyframe timestamp (accent color, clickable edit).
- Cell-level fold / unfold.
- Cell-level move-up / move-down / delete buttons (hover-reveal).
- Stronger visual distinction between code cells and keyframe cells.

**Key files:** `cell_editor/render.rs`

---

### 2.3 Preview Overlay System

Individual overlays exist (grid, snap guides, hover highlight, selection boxes, ghost/onion skin, diff mode split, ruler guides) but there is no unified toggle system.

```rust
pub struct PreviewOverlay {
    show_scene_bounds: bool,
    show_grid: bool,
    show_guides: bool,
    show_actor_labels: bool,
    show_safe_area: bool,
}
```

**Key files:** new `app/preview/overlay.rs`

---

### 2.4 Semantic Highlighting + Refactoring Tools

Deep `animatix_analyzer` integration:
- `SymbolTable`: actor / scene / component definitions.
- Semantic coloring: `ActorName`, `PropertyName`, `SceneName`, `Invalid` (red squiggle).
- Basic refactorings: `RenameActor`, `ExtractScene`, `MoveToScene`.

**Key files:** `cell_editor/highlighting.rs`, `completion_popup.rs`

---

## 3. Long-Term / Speculative

### 3.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 3.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 3.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 4. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Design token system (2.1) | Low | Medium |
| 2 | Cell editor visual redesign (2.2) | Low | Medium |
| 3 | Preview overlay system (2.3) | Low | Low |
| 4 | Semantic highlighting + refactor (2.4) | High | Medium |
| 5 | AST walker for source_edit (1.1) | Medium | Medium |
| 6 | Migrate tree_row → Row (1.2) | Low | Low |
| 7 | Split editor.rs (1.3) | Medium | Low |
| 8 | Unify badge rendering (1.4) | Low | Low |
| 9 | Green tree / trivia AST (3.2) | Very High | Low |
| 10 | Web Canvas (3.1) | Very High | Low |
| 11 | Trivia-inspired AST (3.3) | High | Low |
