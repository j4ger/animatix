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

**Status:** Resolved.

- **Image `url` assignment:** Works. `photo.url = "new.png"` loads the new image at assignment time.
- **SVG `url` assignment:** Works. `icon.url = "new.svg"` parses and reloads the SVG paths.
- **Text/Math/Code content:** Works. `title.text = "new text"` now recompiles typst-rendered glyph paths at runtime via `TextCompiler`, and updates `size`/`layout_size` to match the new text dimensions.

**Location:** `crates/animatix/src/timeline/assignments.rs` (`recompile_text_at_assignment`).

---

## 2. Deferred Features

### 2.0 Multi-Scene Composition — GUI Phases

**Status:** Fully implemented (Phases 1–8 shipped).
**Location:** `crates/animatix/src/composition.rs`, `crates/animatix/src/renderer/video.rs`, `crates/animatix-gui/src/app/panels/mod.rs`, `crates/animatix-gui/src/source_edit.rs`, `docs/multi-scene-composition-design.md`.

Shipped:
- Phase 1: `# SceneName` / `play` syntax in parser, AST, serializer
- Phase 2: `Composition` engine with per-scene timeline building, edge resolution, cycle detection, `BuildTarget` routing
- Phase 3: CLI export (`render_video_composition`, `render_gif_composition`, `render_image_composition`); auto-routing in `main.rs`
- Phase 4: GUI Scene List Panel — Scenes tab in sidebar with select/add/reorder/rename
- Phase 5: GUI Composition Timeline — Scene blocks on scrubber, prev/next scene navigation, active scene time display
- Phase 6: GUI Source Write-Back — `ReorderScenes`, `SetPlayTarget`, `SetTransition`, `RenameScene`, `AddScene` edits via AST mutation
- Phase 7: Transition Blending — `PreviewSurface::render_composition()` evaluates active scene, handles transition periods
- Phase 8: Cross-File Scenes — `module.SceneName` resolution in parser/composition, import aliases shown in scene list
- Examples: `multi_scene_mini.amx`, `multi_scene_demo.amx`, `multi_scene_educational.amx`

### 2.1 Source-Level Animated Geometry — Implemented (non-GUI)

**Status:** `Polygon.points` and `Path.commands` are now animatable at source level. GUI inspector support remains deferred.
**Location:** `crates/animatix/src/timeline/property_engine.rs`, `crates/animatix-gui/src/app/panels/inspector/`.

`poly.points = [[0,0], [100,0], [50,100]]` inside keyframe blocks works and triggers path morphing automatically. `path.commands = {move_to(0,0), line_to(100,0)}` assignments with duration also morph between path states.

**Implementation details:**
- `ValueType::CommandList` parsing converts command expressions to SVG path strings via `kurbo::BezPath::to_svg()`
- `track.commands: Option<PropertyTrack<String>>` stores the SVG representation
- `rebuild_vector_paths` parses the SVG back to `BezPath` with `kurbo::BezPath::from_svg()` and builds the target `VelloPath`
- Start/end keyframes in `vector_paths` are now correctly inserted for all shape-geometry assignments with duration (fixing a pre-existing issue where the morph interval started from the previous keyframe rather than the assignment start time)

**Still deferred:**
- **GUI inspector editing** — no widget exists for editing variable-length lists of `Vec2` points or path commands. The inspector currently displays `"[N pts]"` as a read-only label.

**Effort:** Low–Medium for `commands` (completed). High for GUI (custom multi-point / command editor).

---

### 2.2 Font Selection — Phase 3: System Font Discovery

**Status:** Phases 1 & 2 done. Phase 3 deferred.
**Location:** `crates/animatix/src/renderer/text.rs`, `crates/animatix/src/timeline/scene_eval.rs`.

Access all installed system fonts via `font-kit` / `fontconfig`. Removes the curated-bundle limitation but introduces cross-platform complexity, non-determinism, and async loading concerns.

**Effort:** High. Platform APIs, async loading, font caching.
**Blocked until:** User demand for out-of-bundle fonts.

---

### 2.3 `strategy: fade` Morph

**Status:** Implemented.
**Location:** `crates/animatix/src/timeline/morph.rs`, `crates/animatix/src/timeline/track.rs`.

`fade` cross-fades between path states by rendering both source and target paths at partial opacity instead of geometrically interpolating vertices. Because Vello is an immediate-mode vector renderer, this is achieved by emitting both path sets into the `Scene` with per-path alpha scaling — no separate compositing pass was required.

**Syntax:**
```
circle c [100ms, strategy: fade]
```

**Supported actors:** SVG, Text/Math/Code, Plot, and any actor where `vector_paths` / `text_paths` is the direct render source. Primitive `shape_type` transitions (e.g. `Rect → Circle`) are affected by a pre-existing issue where `build_shape_vector_paths` in `scene_eval.rs` replaces morphed paths with a freshly built primitive path; fixing that is tracked separately.

**Implementation details:**
- `MorphStrategy::Fade` added to the enum alongside `Auto` and `Match`.
- Parser accepts `fade` in timing modifiers (was previously rejected with a diagnostic).
- `interpolate_vello_paths` and `interpolate_text_paths` branch on `Fade` and return both source and target path sets with alpha scaled by `(1-t)` and `t` respectively.
- `TextPath` gained an `opacity: f32` field so that per-path fade alpha can be applied during scene evaluation without converting `typst::visualize::Paint` prematurely.

**Effort:** Low–Medium. No renderer changes needed.

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

### 4.1 Panic Audit — Completed (runtime)

**Status:** All user-facing runtime panic sites converted to `Result` or graceful skip. Test-only unwraps/expects remain (acceptable).
**Location:** Across renderer, action system, parser internals, GUI I/O.

**Completed:**
- **Renderer pipeline** — `RendererCore::new`, `State::new`, `run_timeline_with_options` now return `Result<_, RenderError>`. GPU adapter/surface/device creation failures propagate instead of crashing.
- **Action system** — Replaced 15 `.expect("validated target track")` in reveal/motion/effects/exit/entrance with `match` + `continue` (graceful skip after diagnostic emission).
- **Parser internals** — Replaced non-test `unwrap()` in tuple parser with safe pattern match.
- **GUI I/O** — `DocumentSession` already returned `Result`; fixed non-test panics in highlighting setup (`tree-sitter` init), `interact_pointer_pos()` unwraps in UI interaction, and `eframe::run_native` expect in runtime.
- **GUI preview surface** — `PreviewSurface::new` now returns `Result` and propagates `RenderError`.

**Still present (test code only):**
- Parser tests, source edit tests, composition tests, to_source tests — all use `.unwrap()` on parser results.
- Morph system test panics (`panic!` on unexpected path command types in assertions).

**New error type:** `RenderError` in `crates/animatix/src/renderer/error.rs` covers surface creation, adapter not found, device request failure, Vello init, frame render, window creation, and event loop creation.

---

### 4.2 Diagnostics Quality — Completed

**Status:** Severity reclassified; source spans populated for key sites; `format_diagnostic` now includes line:column.
**Location:** `crates/animatix/src/diagnostics.rs`, across all `Diagnostic::warning/error` call sites.

Completed:
- **Severity reclassification** — The following are now `Error` instead of `Warning`: `UnknownAction`, `UnsupportedActionTarget`, `UnsupportedAssignmentProperty`, `UnsupportedMediaAssignment`, `MediaLoadFailure`, `ModuleExportEvalError`, `UnknownLookupPath`, `UnknownTargetPath`, `UnsupportedSequenceStatement`, `UnsupportedStaggerStatement`.
- **Source spans** — `with_ast_span()` is now populated for action dispatch diagnostics (`UnknownAction`, `UnsupportedActionTarget`). CLI `check` command displays `at line:col`.
- **Diagnostic formatting** — `format_diagnostic` prepends `line:col:` when available.

Remaining gaps:
- **Actionability:** A few diagnostics still describe internal state (e.g., "using default") without telling the user *which* default or *why*.
- **Full span plumbing:** Not every diagnostic creation site has access to an AST span; some are emitted during late-phase evaluation where span info has been lost.

**Impact:** High. User-actionable failures now surface as errors with location info.

---

### 4.3 Logging System — New

**Status:** Implemented.

- Added `tracing` + `tracing-subscriber` with `env-filter` to `Cargo.toml`.
- CLI `--verbose` / `-v` flag controls log level (default WARN, `-v` → DEBUG, `-vv` → TRACE).
- `RUST_LOG` environment variable overrides the CLI flag.
- Key functions instrumented with `#[instrument]`: `Timeline::build_with_diagnostics`, `process_body`, `process_action`.
- `println!` in `renderer/video.rs` replaced with `info!` — progress messages still visible at default INFO level.
- `println!`/`eprintln!` in `main.rs` replaced with `info!`/`error!`.

**Usage:**
```bash
animatix --verbose check file.amx       # debug output
RUST_LOG=animatix=trace animatix check file.amx  # trace output
```

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
| ~~1~~ | ~~Panic audit & error handling~~ | ~~Medium~~ | ~~High~~ |
| ~~2~~ | ~~Diagnostics quality + logging system~~ | ~~Low-Medium~~ | ~~Medium~~ |
| 1 | Multi-Scene GUI (scene list, composition timeline, write-back) | Medium | High |
| 2 | Randomness determinism | Low-Medium | Medium |
| 3 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 4 | Cross-file analyzer | Medium-High | Medium |
| 5 | Green tree / trivia AST | Very High | Low (polish) |

---

*Last updated: 2026-05-16 — Error handling overhaul & logging system implemented*
