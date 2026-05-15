# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## How to Read This Document

- **Gap** — Known limitation in the current runtime.
- **Deferred** — Infrastructure exists; blocked on time or needs focused implementation.
- **TODO** — Inline code marker (`// TODO:`) tracking a local fix.
- **Resolved** — Addressed; kept for historical context.

---

## 1. Known Limitations

### 1.1 Coordinate System Friction

**Status:** Resolved.

`at` (absolute coordinates) and `anchor`/`offset` (layout-based coordinates) previously clashed silently when mixed. Build-time diagnostics now warn about:
- Both `at` and `anchor` specified on the same actor (`conflicting-position-binding`)
- `offset` used with absolute `at` where it has no effect (`ignored-offset`)

**Location:** `crates/animatix/src/timeline/position.rs`.

---

### 1.2 Asymmetrical Reveal/Exit Actions

**Status:** Resolved.

- Added missing `reveal-in` entrance action (mirror of `reveal-out`).
- Fixed `draw-in` category from `"Reveal"` to `"Entrance"` for consistency.
- All entrance/exit pairs are now symmetrical:
  - `fade-in` ↔ `fade-out`
  - `wipe-in` ↔ `wipe-out`
  - `draw-in` ↔ `draw-out`
  - `reveal-in` ↔ `reveal-out`

**Location:** `crates/animatix/src/timeline/actions/reveal.rs`, `crates/animatix/src/timeline/actions/mod.rs`.

---

### 1.3 Re-Declaration for Morphing/Media

**Status:** Partially resolved.

- **Image `url` assignment:** Now works. `photo.url = "new.png"` loads the new image at the assignment time.
- **SVG `url` assignment:** Now works. `icon.url = "new.svg"` parses and reloads the SVG paths.
- **Text/Math/Code content:** Still requires re-declaration for full path regeneration. Property assignment updates the text content track but does not regenerate typst-rendered paths at runtime.

**Location:** `crates/animatix/src/timeline/assignments.rs`.

---

## 2. Deferred Features

### 2.0 Multi-Scene Composition — GUI Phases

**Status:** Core engine shipped (Phases 1–3). GUI and transitions deferred (Phases 4–8).
**Location:** `crates/animatix/src/composition.rs`, `crates/animatix/src/renderer/video.rs`, `docs/multi-scene-composition-design.md`.

Shipped:
- Phase 1: `# SceneName` / `play` syntax in parser, AST, serializer
- Phase 2: `Composition` engine with per-scene timeline building, edge resolution, cycle detection, `BuildTarget` routing
- Phase 3: CLI export (`render_video_composition`, `render_gif_composition`, `render_image_composition`); auto-routing in `main.rs`
- Examples: `multi_scene_mini.amx`, `multi_scene_demo.amx`, `multi_scene_educational.amx`

**Still deferred:**
- **Phase 4 — GUI Scene List Panel**: `app/panels/scene_list.rs`, select/add/reorder/rename scenes
- **Phase 5 — GUI Composition Timeline**: Scene blocks on scrubber, boundary interactions, transition editing
- **Phase 6 — GUI Source Write-Back**: `ReorderScenes`, `SetPlayTarget`, `SetTransition`, `RenameScene` edits
- **Phase 7 — Transition Blending**: Dual offscreen render + texture compositing for fade/wipe transitions
- **Phase 8 — Cross-File Scenes**: `module.SceneName` resolution, project explorer with referenced scene files

**Effort:** Medium for GUI (Phases 4–6, ~1 week each). High for transitions (Phase 7, ~1 week). Medium for cross-file (Phase 8, ~0.5 week).

### 2.1 Source-Level Animated Geometry — Partial

**Status:** `Polygon.points` now animatable at source level. `Path.commands` and GUI inspector support deferred.
**Location:** `crates/animatix/src/timeline/property_engine.rs`, `crates/animatix-gui/src/app/panels/inspector/`.

`poly.points = [[0,0], [100,0], [50,100]]` inside keyframe blocks now works and triggers path morphing automatically. The track storage (`PropertyTrack<Vec<[f32; 2]>>`), frame-time evaluation, and assignment engine were already wired; only the parser → property engine bridge was missing.

**Still deferred:**
- **`Path.commands`** — requires a new `commands: Option<PropertyTrack<String>>` field on `AnimationTrack` and on-the-fly parsing from command strings to `BezPath` at evaluation time. The current `vector_paths` field stores pre-built `VelloPath` and has no raw-command fallback.
- **GUI inspector editing** — no widget exists for editing variable-length lists of `Vec2` points. The inspector currently displays `"[N pts]"` as a read-only label.

**Effort:** Low for `commands` (similar bridge work, plus a track field). High for GUI (custom multi-point editor).

---

### 2.2 Font Selection — Phase 3: System Font Discovery

**Status:** Phases 1 & 2 done. Phase 3 deferred.
**Location:** `crates/animatix/src/renderer/text.rs`, `crates/animatix/src/timeline/scene_eval.rs`.

Access all installed system fonts via `font-kit` / `fontconfig`. Removes the curated-bundle limitation but introduces cross-platform complexity, non-determinism, and async loading concerns.

**Effort:** High. Platform APIs, async loading, font caching.
**Blocked until:** User demand for out-of-bundle fonts.

---

### 2.3 `strategy: fade` Morph

**Status:** Deferred — requires compositing architecture change.
**Location:** `crates/animatix/src/timeline/morph.rs`.

Current morphing (`auto`, `match`, `path_arc`, `stretch`) interpolates paths between two states. `fade` would cross-fade between overlapping states, which needs:
- Render both source and target shapes at partial opacity.
- Compositing pass or dual-path rendering.

**Effort:** High. Touches renderer compositing.

---

## 3. Analyzer / LSP Improvements

### 3.1 Cross-File Analysis

**Status:** Phase 7 of analyzer design — not started.
**Location:** `docs/contributing.md` §Analyzer Architecture.

- Extend `Analyzer` to accept multiple files.
- Use `ModuleGraph` for import resolution.
- Cross-file symbol table.
- LSP: `workspace/symbol`, `textDocument/references`.

**Effort:** Medium-High.

---

### 3.2 Analyzer Default Serialization

**TODO:** `crates/animatix-analyzer/src/symbol_table.rs:271`
```rust
default: None, // TODO: serialize default
```

Symbol table property entries don't capture default values yet.

**Effort:** Low.

---

## 4. Error Handling & Robustness

### 4.1 Panic Audit — In Progress

**Status:** ~151 unwrap/expect + ~21 panic in `animatix` core; ~27 unwrap/expect in `animatix-gui`.
**Location:** Across renderer, action system, parser internals, document I/O.

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

### 4.2 Diagnostics Quality

**Status:** Partial. Language cleaned; deduplication implemented; source spans remain deferred.
**Location:** `crates/animatix/src/diagnostics.rs`, across all `Diagnostic::warning/error` call sites.

Completed:
- Removed POC-inappropriate language ("currently", "not supported yet", "remains deferred", internal jargon like "IR" / "AST fallback").
- Implemented automatic deduplication in `BuildReport::new` based on `(code, message, subject)` fingerprint.
- Removed two unused diagnostic codes (`ColorschemeLoadFailure`, `EmptyAutoColorPool`).

Remaining gaps:
- **Source spans:** Most diagnostics lack line/column info. `with_ast_span()` exists but is rarely populated. The parser has byte spans; these need to flow through `BuildReport` to the CLI / GUI.
- **Severity classification:** Some warnings should be errors. Need a pass to reclassify by user-actionability.
- **Actionability:** A few diagnostics still describe internal state (e.g., "using default") without telling the user *which* default or *why*.

**Effort:** Low-Medium. ~2-4 hours for severity audit + span plumbing.
**Impact:** Medium. Clean diagnostics are the primary user feedback channel.

---

## 5. Architecture / Cleanup Debt

### 5.1 Dynamic Layout — Post-Migration Cleanup

**Location:** `docs/architecture.md` §Layout System.

- Richer `ContainerLayoutChild` entries than just labels.
- Reducing metadata duplication between `child_order` and `layout_children`.
- Retiring legacy `size` from non-layout subsystems if desired.

**Effort:** Low-Medium.

---

### 5.2 Randomness Determinism

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
| 1 | Panic audit & error handling (renderer pipeline, GUI I/O) | Medium | High |
| 2 | Multi-Scene GUI (scene list, composition timeline, write-back) | Medium | High |
| 3 | Randomness determinism | Low-Medium | Medium |
| 4 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 5 | Multi-Scene transition blending (Phase 7) | High | Medium |
| 6 | Cross-file analyzer | Medium-High | Medium |
| 7 | `strategy: fade` morph | High | Medium |
| 8 | Multi-Scene cross-file scenes (Phase 8) | Medium | Medium |
| 9 | Green tree / trivia AST | Very High | Low (polish) |

---

*Last updated: 2026-05-15*
