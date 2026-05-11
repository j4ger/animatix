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

## 1. Planned Features (Ready to Implement)

### 1.1 Module Re-Exports

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

### 1.2 Keyframe Merge in Keyframe Mode

**TODO:** `crates/animatix-gui/src/app/property_edits.rs:30`

When keyframe mode inserts a new keyframe within 50ms (`MERGE_WINDOW_S`) of an existing one, it skips insertion. It should **merge** with the existing keyframe instead.

**Effort:** Low.

---

### 1.3 LSP Diagnostics Publishing

**Status:** Analyzer produces diagnostics. LSP server implementation incomplete; diagnostics are not yet published to the client.

**Gap:** Semantic diagnostics (unknown labels, actions, properties) are produced by the analyzer but the LSP connection layer does not yet forward them.

**Effort:** Low. The `Analyzer::diagnostics()` method exists; needs wiring in the LSP server loop.

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

## 3. Known Limitations (Documented Gaps)

### 3.1 Static Geometry

**Status:** Declaration-time only.

`Polygon.points` and `Path.commands` are set at declaration time and cannot be animated dynamically frame-by-frame.

**Impact:** Cannot morph a polygon by changing its point set over time.

**Workaround:** Re-declare the entire actor at a new keyframe (triggers path morphing).

---

### 3.2 Missing Rotation on `Ellipse`

**Status:** No dedicated rotation parameter.

Basic shapes like `Ellipse` do not support a dedicated `angle` or `rotation` parameter for geometry rotation. Must use `rotate` action (visual transform) or `rotation` property (if available on the actor).

---

### 3.3 Coordinate System Friction

**Status:** Design tension.

`at` (absolute coordinates) and `anchor`/`offset` (layout-based coordinates) often clash when mixed, requiring manual intervention.

---

### 3.4 Asymmetrical Reveal/Exit Actions

**Status:** Partial.

Some fade-out behaviors are incomplete or non-intuitive compared to entrance counterparts (`fade-in` vs `fade-out`, `draw-in` vs `draw-out`).

---

### 3.5 Re-Declaration for Morphing/Media

**Status:** Requires full re-declaration.

Morphing text or updating SVG/Image sources requires re-declaring the entire object at a new keyframe. Standard property assignment (`img.url = "new.svg"`) does not trigger media reload.

---

## 4. Deferred Features (Blocked or High Effort)

### 4.1 Font Selection — Phase 3: System Font Discovery

**Status:** Phases 1 & 2 done (see §8). Phase 3 deferred.
**Location:** `crates/animatix/src/renderer/text.rs`, `crates/animatix/src/timeline/scene_eval.rs`.

Access all installed system fonts via `font-kit` / `fontconfig`. Removes the curated-bundle limitation but introduces cross-platform complexity, non-determinism, and async loading concerns.

**Effort:** High. Platform APIs, async loading, font caching.
**Blocked until:** User demand for out-of-bundle fonts.

---

### 4.2 `strategy: fade` Morph

**Status:** Deferred — requires compositing architecture change.
**Location:** `crates/animatix/src/timeline/morph.rs`.

Current morphing (`auto`, `match`, `path_arc`, `stretch`) interpolates paths between two states. `fade` would cross-fade between overlapping states, which needs:
- Render both source and target shapes at partial opacity.
- Compositing pass or dual-path rendering.

**Effort:** High. Touches renderer compositing.

---

### 4.3 Nested `sequence` / `stagger`

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

### 4.4 Custom Component Actions

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

## 5. Analyzer / LSP Improvements

### 5.1 Cross-File Analysis

**Status:** Phase 7 of analyzer design — not started.
**Location:** `docs/contributing.md` §Analyzer Architecture.

- Extend `Analyzer` to accept multiple files.
- Use `ModuleGraph` for import resolution.
- Cross-file symbol table.
- LSP: `workspace/symbol`, `textDocument/references`.

**Effort:** Medium-High.

---

### 5.2 Analyzer Default Serialization

**TODO:** `crates/animatix-analyzer/src/symbol_table.rs:271`
```rust
default: None, // TODO: serialize default
```

Symbol table property entries don't capture default values yet.

**Effort:** Low.

---

## 6. Architecture / Cleanup Debt

### 6.1 Property System — Post-Refactor Cleanup

The property system refactor (`c23117e`) replaced 7+ cross-file match blocks with a registry-driven engine. Some cleanup remains:

- **Backward-compat accessors** on `AnimationTrack` (e.g. `track.position()` forwarding to `track.geometry.position`). These can be removed once all call sites use tiered paths.
- **`PrimitiveDescriptor::for_actor_type()`** — replace with `ActorKindId` stored in `track.header.kind`.
- **`VectorShapeState`** struct — if fully subsumed by `GroupHandlerId::VectorShapeState`, delete it.

**Effort:** Low. Mechanical removal.

---

### 6.2 Dynamic Layout — Post-Migration Cleanup

**Location:** `docs/architecture.md` §Layout System.

- Richer `ContainerLayoutChild` entries than just labels.
- Reducing metadata duplication between `child_order` and `layout_children`.
- Retiring legacy `size` from non-layout subsystems if desired.

**Effort:** Low-Medium.

---

### 6.3 Randomness Determinism

**Status:** Documented caveat.
**Location:** `docs/architecture.md` §Reactive System.

Current `rand()` is not a deterministic function of time. Scenes depending on fresh randomness per evaluation break the random-access frame promise.

**Options:**
- Seed `rand()` from `t` + label hash for deterministic pseudo-randomness.
- Add `seeded_rand(t, seed)` builtin.

**Effort:** Low-Medium.

---

## 7. Long-Term / Speculative

### 7.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 7.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 7.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 8. Quick Reference: Priority Order

If you want to pick something up, here is a suggested order by effort-to-impact ratio:

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Module re-exports | Low-Medium | Medium |
| 2 | Keyframe merge in keyframe mode | Low | Medium |
| 3 | LSP diagnostics publishing | Low | Medium |
| 4 | `Expr::Index` (array access) | Medium | High |
| 5 | Property system cleanup (remove accessors) | Low | Low (cleanup) |
| 6 | Analyzer default serialization | Low | Low |
| 7 | Randomness determinism | Low-Medium | Medium |
| 8 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 9 | `Expr::Method` | Medium-High | Medium |
| 10 | Cross-file analyzer | Medium-High | Medium |
| 11 | `strategy: fade` morph | High | Medium |
| 12 | Nested sequence/stagger | High | Medium-High |
| 13 | Custom component actions | High | High |
| 14 | `Expr::Construct` | High | Medium |
| 15 | Green tree / trivia AST | Very High | Low (polish) |

---

*Last updated: 2026-05-11*

---

## 9. Recently Completed

| Date | Item | Notes |
|------|------|-------|
| 2026-05-11 | `reorder` action | Runtime action for explicit full-order container reordering; syntax: `reorder container [order: (c, b, a), 500ms]`; overlap detection; same interpolation infrastructure as `swap` |
| 2026-05-11 | Source formatting spec + serializer | Deterministic `.amx` output: 2-space indent, children on separate lines, block statements properly nested |
| 2026-05-11 | GUI flexbox child reordering | Canvas drag-to-reorder for layout-managed children with ghost overlay and drop indicators; inspector children panel with up/down buttons; AST mutation persists order to source |
| 2026-05-11 | Container `padding` property | Added to `ContainerMetadata`, Taffy layout, build pipeline, property registry, and inspector |
| 2026-05-08 | Font Selection — Phase 1 & 2 | Static font bundle + runtime text recompilation with `TextCompiler` cache |
| 2026-05-08 | `font_family` / `font_size` property fix | Both were mapped to wrong fields (silently broken); now properly stored on `AnimationTrack` |
| 2026-05-08 | Parser span capture + analyzer positions | `Span` added to all `Stmt` variants; `Analyzer::enrich_positions` populates real line/col from tree-sitter for LSP go-to-definition |
| 2026-05-08 | Bi-directional timeline sync | `TimelineIndex` maps source lines ↔ times; editor cursor shows cyan indicator on timeline; timeline scrub scrolls editor to keyframe |
| 2026-05-08 | Editor cursor tracking | egui `TextEdit` cursor position exposed; completions and edits insert at actual cursor position |
| 2026-05-08 | Analyzer symbol table line extraction | Real `(line, col)` positions for all declarations; LSP `goto_definition` and document outline correct |
