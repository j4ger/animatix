# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## 1. Visual Polish & Tooling (P3)

### 1.1 Design Token System

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

### 1.2 Cell Editor Visual Redesign

- Accent left border on focus.
- Large, prominent keyframe timestamp (accent color, clickable edit).
- Cell-level fold / unfold.
- Cell-level move-up / move-down / delete buttons (hover-reveal).
- Stronger visual distinction between code cells and keyframe cells.

**Key files:** `cell_editor/render.rs`

---

### 1.3 Preview Overlay System

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

### 1.4 Semantic Highlighting + Refactoring Tools

Deep `animatix_analyzer` integration:
- `SymbolTable`: actor / scene / component definitions.
- Semantic coloring: `ActorName`, `PropertyName`, `SceneName`, `Invalid` (red squiggle).
- Basic refactorings: `RenameActor`, `ExtractScene`, `MoveToScene`.

**Key files:** `cell_editor/highlighting.rs`, `completion_popup.rs`

---

## 2. Long-Term / Speculative

### 2.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 2.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 2.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 3. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Design token system (1.1) | Low | Medium |
| 2 | Cell editor visual redesign (1.2) | Low | Medium |
| 3 | Preview overlay system (1.3) | Low | Low |
| 4 | Semantic highlighting + refactor (1.4) | High | Medium |
| 5 | Green tree / trivia AST (2.2) | Very High | Low |
| 6 | Web Canvas (2.1) | Very High | Low |
| 7 | Trivia-inspired AST (2.3) | High | Low |
