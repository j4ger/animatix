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

### 2.3 Coordinate System Friction

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

## 5. Error Handling & Robustness

### 5.1 Panic Audit — In Progress

**Status:** ~151 unwrap/expect + ~21 panic in `animatix` core; ~27 unwrap/expect in `animatix-gui`.
**Location:** Across renderer, action system, parser internals, document I/O.

**Recently fixed:** `crates/animatix/src/renderer/video.rs` — all 8 export functions now return `Result<(), ExportError>` with typed error variants (`RendererCreation`, `FrameRender`, `VideoEncode`, `GifEncode`, `ImageEncode`, `ImageSave`, `InvalidPath`, `ThreadPanicked`).

**Remaining hotspots (by severity):**

| Area | Count | Severity | Notes |
|------|-------|----------|-------|
| Action system (`timeline/actions/*.rs`) | 34 | Medium | `.expect("validated target track")` — internal invariants that should hold but could fail on corrupted timeline data |
| Renderer pipeline (`renderer/core.rs`, `text.rs`, `window.rs`) | 12 | High | GPU adapter failure, surface creation, typst compilation — all crash the app |
| Parser internals (`parser.rs`) | 10 | Medium | `panic!` on unexpected AST node types in helper functions |
| Source serialization (`to_source.rs`) | 2 | Low | `panic!` on expected Keyframe node |
| Morph system (`morph.rs`) | 2 | Medium | `panic!` on unexpected path command types |
| Module/primitives/utils | 3 | Low | Internal invariant violations |
| GUI document I/O (`document.rs`) | 10 | High | `fs::write`, `fs::read_to_string`, `create_dir_all` — crash on permission denied / disk full |
| GUI source editing (`source_edit.rs`) | 10 | Low | Mostly in test code |
| GUI UI interaction | 3 | Medium | `.unwrap()` on `interact_pointer_pos()` — could panic on edge-case input |
| GUI runtime/highlighting | 4 | Low | Tree-sitter setup, app startup |

**Cleanup plan:**

1. **Renderer pipeline** — Convert `RendererCore::new`, `OffscreenRenderer::new`, `run_timeline_with_options` to return `Result`. Surface creation and Vello init should propagate errors.
2. **Action system** — Replace `.expect("validated target track")` with `Result`-based dispatch. Actions already go through a central registry; the registry can return `Result` instead of panicking.
3. **GUI document I/O** — Convert `DocumentSession::save_to_disk`, `load`, `reload_from_disk` to return `Result` with typed errors (`IoError`, `ParseError`, `BuildError`).
4. **Parser internals** — Replace `panic!` in AST traversal helpers with `Result` or `Option` returns. Most callers can propagate gracefully.
5. **Morph system** — Replace `panic!` with `Result` that propagates up to `Timeline::build` diagnostics.

**Effort:** Medium. The export renderer refactor took ~1 hour and touched ~500 lines. A full audit would take ~1-2 days.
**Impact:** High. Every panic is a potential user-facing crash.

---

## 6. Architecture / Cleanup Debt

### 6.1 Primitive System — Completed

✅ **Unified primitive architecture implemented** (`crates/animatix/src/primitives/`).

The `PRIMITIVES` array is now the single source of truth. `ActorKindMeta` registry, `PrimitiveDescriptor`, and `find_actor_kind()` all delegate to it. See [`primitive_architecture.md`](primitive_architecture.md) for details.

**Remaining cleanup:**
- **`VectorShapePrimitive` trait** in `timeline/shapes/primitives.rs` — still used by the render pipeline. Can be absorbed into `Primitive::render()` once render dispatch is fully migrated.
- **`ShapeType` enum** in `timeline/shapes/mod.rs` — still used in match arms. Consider unifying with `ShapeKind`.

**Effort:** Low. Mechanical removal once render pipeline is ready.

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

## 7. Long-Term / Speculative

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

## 8. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Panic audit & error handling (renderer pipeline, GUI I/O) | Medium | High |
| 2 | Keyframe merge in keyframe mode | Low | Medium |
| 3 | LSP diagnostics publishing | Low | Medium |
| 4 | Property system cleanup (remove accessors) | Low | Low (cleanup) |
| 5 | Analyzer default serialization | Low | Low |
| 6 | Randomness determinism | Low-Medium | Medium |
| 7 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 8 | Cross-file analyzer | Medium-High | Medium |
| 9 | `strategy: fade` morph | High | Medium |
| 10 | Green tree / trivia AST | Very High | Low (polish) |

---

*Last updated: 2026-05-13*
