# Animatix Roadmap

> Consolidated view of known gaps, planned features, deferred work, and code TODOs.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## How to Read This Document

- **Deferred** — Infrastructure exists; blocked on time or needs focused implementation.
- **Planned** — Design is clear; waiting for implementation.
- **Gap** — Known limitation in the current runtime.
- **TODO** — Inline code marker (`// TODO:`) tracking a local fix.

---

## 1. Deferred Features (High Impact)

### 1.1 Render-Time Font Compilation for Text

**Status:** Infrastructure in place; compilation deferred.
**Location:** `crates/animatix/src/timeline/declarations_text.rs`, renderer text path.

Property assignment for `text` / `latex` / `math` / `code` stores the string value but does **not** trigger re-compilation of text glyph paths at render time. Changing `title.text = "New"` at runtime updates the track but the renderer still shows the old glyph paths.

**What exists:**
- Text/Math/Code paths are compiled at build time via Typst/LaTeX/source shaping.
- `text_content` track stores the string.
- Renderer reads `text_paths` (compiled glyphs), not `text_content`.

**What’s needed:**
- At render time, if `text_content` changed since last frame, re-run the typesetter.
- Cache compiled paths per `(content, font_size, font_family)` to avoid re-shaping every frame.
- Wire into `scene_eval.rs` or `runtime.rs` so `always { label.text = format("{t}", t) }` works.

**Effort:** Medium. The typesetting infrastructure exists; it needs to be called from the frame loop.

---

### 1.2 `strategy: fade` Morph

**Status:** Deferred — requires compositing architecture change.
**Location:** `crates/animatix/src/timeline/morph.rs`.

Current morphing (`auto`, `match`, `path_arc`, `stretch`) interpolates paths between two states. `fade` would cross-fade between overlapping states, which needs:
- Render both source and target shapes at partial opacity.
- Compositing pass or dual-path rendering.

**Effort:** High. Touches renderer compositing.

---

## 2. Language Gaps (AST-Defined, Rejected at Runtime)

These expression variants are parsed into the AST but explicitly error at runtime. Implementing them unlocks richer object interaction.

### 2.1 `Expr::Index` — Array/Index Access

**Status:** AST-defined, explicit runtime error.
**Example:** `points[0]`, `items[i]`

**Blocker:** Need indexed read/write in the expression evaluator and the reactive VM.

**Effort:** Medium. Add `Index` to `Value`, evaluator dispatch, and VM `LoadIndex` instruction.

---

### 2.2 `Expr::Method` — Method Calls

**Status:** AST-defined, explicit runtime error.
**Example:** `list.length()`, `string.split(",")`

**Blocker:** Need method dispatch table on `Value` variants.

**Effort:** Medium-High. Design method namespaces per value type.

---

### 2.3 `Expr::Construct` — Struct/Object Construction

**Status:** AST-defined, explicit runtime error.
**Example:** `Point { x: 10, y: 20 }`

**Blocker:** Need named struct types in the value system.

**Effort:** High. Touches parser, AST, evaluator, and VM.

---

## 3. Planned Features

### 3.1 `reorder` Action

**Status:** Designed, not implemented.
**Location:** `docs/spec.md` §6, `crates/animatix/src/timeline/actions/`.

`swap a, b [500ms]` exists and swaps two children. `reorder` would allow explicit full-order specification independent of swap history:

```animatix
reorder row [c, b, a] [500ms]
```

Unlike `swap`, `reorder` could support overlapping transitions by capturing a snapshot of the current order at action start time.

**Reference implementation:** `swap` action in `timeline/actions/reorder.rs` (or similar).

**Effort:** Medium. Follows the same pattern as `swap`.

---

### 3.2 Custom Component Actions

**Status:** Reserved syntax, rejected by parser.
**Location:** `docs/spec.md` §12.

Authors could define actions inside components:

```animatix
pub component Button(text: "OK") {
    action pulse {
        scale = 1.2 [100ms]
        scale = 1.0 [100ms]
    }
    // ...
}
```

Then invoke as:

```animatix
pulse btn [200ms]
```

**Effort:** High. Touches parser, module system, timeline build, and action registry.

---

### 3.3 Nested `sequence` / `stagger`

**Status:** Explicitly rejected by parser/runtime.
**Location:** `docs/spec.md` §6.

Currently `sequence { ... }` and `stagger [150ms] { ... }` cannot nest, and declarations inside them are rejected.

**Use case:** Nested choreography:

```animatix
sequence {
    stagger [100ms] {
        fade-in a [300ms]
        fade-in b [300ms]
    }
    move group [to: (100, 0), 500ms]
}
```

**Effort:** High. Requires timeline build changes for nested temporal scopes.

---

### 3.4 Module Re-Exports

**Status:** Not supported.
**Location:** `docs/spec.md` §11.

A module cannot export values from its own imports:

```animatix
// theme.amx
import "colors.amx" as c
pub let accent = c.accent  // ERROR: re-exports not supported
```

**Effort:** Low-Medium. Module graph already tracks exports; need to resolve re-export chains.

---

## 4. Known Limitations (Documented Gaps)

### 4.1 Static Geometry

**Status:** Declaration-time only.

`Polygon.points` and `Path.commands` are set at declaration time and cannot be animated dynamically frame-by-frame.

**Impact:** Cannot morph a polygon by changing its point set over time.

**Workaround:** Re-declare the entire actor at a new keyframe (triggers path morphing).

---

### 4.2 Missing Rotation on `Ellipse`

**Status:** No dedicated rotation parameter.

Basic shapes like `Ellipse` do not support a dedicated `angle` or `rotation` parameter for geometry rotation. Must use `rotate` action (visual transform) or `rotation` property (if available on the actor).

---

### 4.3 Coordinate System Friction

**Status:** Design tension.

`at` (absolute coordinates) and `anchor`/`offset` (layout-based coordinates) often clash when mixed, requiring manual intervention.

---

### 4.4 Asymmetrical Reveal/Exit Actions

**Status:** Partial.

Some fade-out behaviors are incomplete or non-intuitive compared to entrance counterparts (`fade-in` vs `fade-out`, `draw-in` vs `draw-out`).

---

### 4.5 Re-Declaration for Morphing/Media

**Status:** Requires full re-declaration.

Morphing text or updating SVG/Image sources requires re-declaring the entire object at a new keyframe. Standard property assignment (`img.url = "new.svg"`) does not trigger media reload.

---

## 5. GUI / Editor Improvements

### 5.1 Editor Cursor Position

**TODO:** `crates/animatix-gui/src/editor.rs:218`
```rust
let cursor_rect = response.rect; // TODO: get actual cursor position
```

egui `TextEdit` does not expose a programmatic cursor/scroll API. The completion popup and insert-at-cursor features are approximated.

**Options:**
- Fork/patch egui to expose cursor position.
- Integrate `egui_code_editor` or a more capable editor widget.
- Build a custom text widget with cursor tracking.

---

### 5.2 Insert at Actual Cursor Position

**TODO:** `crates/animatix-gui/src/editor.rs:243`
```rust
// TODO: insert at actual cursor position
```

Completions and property edits insert at approximated positions. Needs actual cursor line/column from the editor widget.

---

### 5.3 Editor-Timeline Sync (Bidirectional)

**Status:** Partially implemented.

**What works:**
- Timeline scrub → editor scrolls to nearest keyframe line (via `find_keyframe_line_at`).
- Keyframe line highlighting (amber tag background, blue synced line).

**What’s missing:**
- **Parser span capture:** `Span` struct exists in `ast.rs`, but the parser (chumsky) does not populate it.
- **Timeline index:** No `time → source location` mapping built from parsed spans.
- **True bidirectional sync:** Editor cursor → timeline scrub is not implemented.

**Effort:** Medium. Requires parser changes to capture `Rich<'src, char>` span info.

---

### 5.4 Keyframe Merge in Keyframe Mode

**TODO:** `crates/animatix-gui/src/app.rs` (keyframe edit handler)

When keyframe mode inserts a new keyframe within 50ms of an existing one, it skips insertion. It should **merge** with the existing keyframe instead.

---

### 5.5 Analyzer Symbol Table Line Extraction

**TODO:** `crates/animatix-analyzer/src/symbol_table.rs:210`
```rust
line: 0, // TODO: extract from span when available
```

Symbol table entries use `line: 0` as a placeholder. Once parser spans are populated, this should extract actual line numbers.

---

### 5.6 Analyzer Default Serialization

**TODO:** `crates/animatix-analyzer/src/symbol_table.rs:271`
```rust
default: None, // TODO: serialize default
```

Symbol table property entries don’t capture default values yet.

---

## 6. Analyzer / LSP Improvements

### 6.1 Cross-File Analysis

**Status:** Phase 7 of analyzer design — not started.
**Location:** `docs/contributing.md` §Analyzer Architecture.

- Extend `Analyzer` to accept multiple files.
- Use `ModuleGraph` for import resolution.
- Cross-file symbol table.
- LSP: `workspace/symbol`, `textDocument/references`.

**Effort:** Medium-High.

---

### 6.2 LSP Diagnostics Publishing

**Status:** Analyzer produces diagnostics; LSP publishes them. Verified working.

**Gap:** Semantic diagnostics (unknown labels, actions, properties) are produced by the analyzer but may not be as comprehensive as timeline build diagnostics.

---

## 7. Architecture / Cleanup Debt

### 7.1 Property System — Post-Refactor Cleanup

The property system refactor (`c23117e`) replaced 7+ cross-file match blocks with a registry-driven engine. Some cleanup remains:

- **Backward-compat accessors** on `AnimationTrack` (e.g. `track.position()` forwarding to `track.geometry.position`). These can be removed once all call sites use tiered paths.
- **`PrimitiveDescriptor::for_actor_type()`** — replace with `ActorKindId` stored in `track.header.kind`.
- **`VectorShapeState`** struct — if fully subsumed by `GroupHandlerId::VectorShapeState`, delete it.

**Effort:** Low. Mechanical removal.

---

### 7.2 Dynamic Layout — Post-Migration Cleanup

**Location:** `docs/architecture.md` §Layout System.

- Richer `ContainerLayoutChild` entries than just labels.
- Reducing metadata duplication between `child_order` and `layout_children`.
- Retiring legacy `size` from non-layout subsystems if desired.

**Effort:** Low-Medium.

---

### 7.3 Randomness Determinism

**Status:** Documented caveat.
**Location:** `docs/architecture.md` §Reactive System.

Current `rand()` is not a deterministic function of time. Scenes depending on fresh randomness per evaluation break the random-access frame promise.

**Options:**
- Seed `rand()` from `t` + label hash for deterministic pseudo-randomness.
- Add `seeded_rand(t, seed)` builtin.

---

## 8. Long-Term / Speculative

### 8.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 8.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 8.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 9. Quick Reference: Priority Order

If you want to pick something up, here is a suggested order by effort-to-impact ratio:

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Render-time font compilation | Medium | Very High |
| 2 | `Expr::Index` (array access) | Medium | High |
| 3 | Parser span capture (enables editor sync) | Medium | High |
| 4 | `reorder` action | Medium | Medium-High |
| 5 | Module re-exports | Low-Medium | Medium |
| 6 | Property system cleanup (remove accessors) | Low | Low (cleanup) |
| 7 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 8 | Custom component actions | High | High |
| 9 | Nested sequence/stagger | High | Medium-High |
| 10 | `Expr::Method` | Medium-High | Medium |
| 11 | `strategy: fade` morph | High | Medium |
| 12 | Cross-file analyzer | Medium-High | Medium |
| 13 | `Expr::Construct` | High | Medium |
| 14 | Green tree / trivia AST | Very High | Low (polish) |

---

*Last updated: 2026-05-08*
