# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## How to Read This Document

- **Planned** — Design is clear; waiting for implementation.
- **Gap** — Known limitation in the current runtime.
- **Deferred** — Infrastructure exists; blocked on time or needs focused implementation.
- **TODO** — Inline code marker (`// TODO:`) tracking a local fix.

---

## 1. Planned Features

### 1.1 Keyframe Merge in Keyframe Mode

**TODO:** `crates/animatix-gui/src/app/property_edits.rs:30`

When keyframe mode inserts a new keyframe within 50ms (`MERGE_WINDOW_S`) of an existing one, it skips insertion. It should **merge** with the existing keyframe instead.

**Effort:** Low.

---

### 1.2 LSP Diagnostics Publishing

**Status:** Analyzer produces diagnostics. LSP server implementation incomplete; diagnostics are not yet published to the client.

**Gap:** Semantic diagnostics (unknown labels, actions, properties) are produced by the analyzer but the LSP connection layer does not yet forward them.

**Effort:** Low. The `Analyzer::diagnostics()` method exists; needs wiring in the LSP server loop.

---

## 2. Known Limitations



### 2.2 Static Geometry

**Status:** Declaration-time only.

`Polygon.points` and `Path.commands` are set at declaration time and cannot be animated dynamically frame-by-frame.

**Impact:** Cannot morph a polygon by changing its point set over time.

**Workaround:** Re-declare the entire actor at a new keyframe (triggers path morphing).

---

### 2.3 Missing Rotation on `Ellipse`

**Status:** No dedicated rotation parameter.

Basic shapes like `Ellipse` do not support a dedicated `angle` or `rotation` parameter for geometry rotation. Must use `rotate` action (visual transform) or `rotation` property (if available on the actor).

---

### 2.4 Coordinate System Friction

**Status:** Design tension.

`at` (absolute coordinates) and `anchor`/`offset` (layout-based coordinates) often clash when mixed, requiring manual intervention.

---

### 2.5 Asymmetrical Reveal/Exit Actions

**Status:** Partial.

Some fade-out behaviors are incomplete or non-intuitive compared to entrance counterparts (`fade-in` vs `fade-out`, `draw-in` vs `draw-out`).

---

### 2.6 Re-Declaration for Morphing/Media

**Status:** Requires full re-declaration.

Morphing text or updating SVG/Image sources requires re-declaring the entire object at a new keyframe. Standard property assignment (`img.url = "new.svg"`) does not trigger media reload.

---

## 3. Deferred Features

### 3.1 Font Selection — Phase 3: System Font Discovery

**Status:** Phases 1 & 2 done. Phase 3 deferred.
**Location:** `crates/animatix/src/renderer/text.rs`, `crates/animatix/src/timeline/scene_eval.rs`.

Access all installed system fonts via `font-kit` / `fontconfig`. Removes the curated-bundle limitation but introduces cross-platform complexity, non-determinism, and async loading concerns.

**Effort:** High. Platform APIs, async loading, font caching.
**Blocked until:** User demand for out-of-bundle fonts.

---

### 3.2 `strategy: fade` Morph

**Status:** Deferred — requires compositing architecture change.
**Location:** `crates/animatix/src/timeline/morph.rs`.

Current morphing (`auto`, `match`, `path_arc`, `stretch`) interpolates paths between two states. `fade` would cross-fade between overlapping states, which needs:
- Render both source and target shapes at partial opacity.
- Compositing pass or dual-path rendering.

**Effort:** High. Touches renderer compositing.

---

## 4. Analyzer / LSP Improvements

### 4.1 Cross-File Analysis

**Status:** Phase 7 of analyzer design — not started.
**Location:** `docs/contributing.md` §Analyzer Architecture.

- Extend `Analyzer` to accept multiple files.
- Use `ModuleGraph` for import resolution.
- Cross-file symbol table.
- LSP: `workspace/symbol`, `textDocument/references`.

**Effort:** Medium-High.

---

### 4.2 Analyzer Default Serialization

**TODO:** `crates/animatix-analyzer/src/symbol_table.rs:271`
```rust
default: None, // TODO: serialize default
```

Symbol table property entries don't capture default values yet.

**Effort:** Low.

---

## 5. Architecture / Cleanup Debt

### 5.1 Property System — Post-Refactor Cleanup

The property system refactor (`c23117e`) replaced 7+ cross-file match blocks with a registry-driven engine. Some cleanup remains:

- **Backward-compat accessors** on `AnimationTrack` (e.g. `track.position()` forwarding to `track.geometry.position`). These can be removed once all call sites use tiered paths.
- **`PrimitiveDescriptor::for_actor_type()`** — replace with `ActorKindId` stored in `track.header.kind`.
- **`VectorShapeState`** struct — if fully subsumed by `GroupHandlerId::VectorShapeState`, delete it.

**Effort:** Low. Mechanical removal.

---

### 5.2 Dynamic Layout — Post-Migration Cleanup

**Location:** `docs/architecture.md` §Layout System.

- Richer `ContainerLayoutChild` entries than just labels.
- Reducing metadata duplication between `child_order` and `layout_children`.
- Retiring legacy `size` from non-layout subsystems if desired.

**Effort:** Low-Medium.

---

### 5.3 Randomness Determinism

**Status:** Documented caveat.
**Location:** `docs/architecture.md` §Reactive System.

Current `rand()` is not a deterministic function of time. Scenes depending on fresh randomness per evaluation break the random-access frame promise.

**Options:**
- Seed `rand()` from `t` + label hash for deterministic pseudo-randomness.
- Add `seeded_rand(t, seed)` builtin.

**Effort:** Low-Medium.

---

## 6. Long-Term / Speculative

### 6.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 6.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 6.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 7. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Keyframe merge in keyframe mode | Low | Medium |
| 2 | LSP diagnostics publishing | Low | Medium |
| 3 | Property system cleanup (remove accessors) | Low | Low (cleanup) |
| 4 | Analyzer default serialization | Low | Low |
| 5 | Randomness determinism | Low-Medium | Medium |
| 6 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 7 | Cross-file analyzer | Medium-High | Medium |
| 8 | `strategy: fade` morph | High | Medium |
| 9 | Green tree / trivia AST | Very High | Low (polish) |

---

*Last updated: 2026-05-12*
