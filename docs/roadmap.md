# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 2. Long-Term / Speculative

### 2.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 2.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 2.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 4. Design Notes

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | ~~Property registry auto-dispatch (6.1)~~ | Medium | High |
| 2 | ~~Coordinate Transform struct (6.3)~~ | Low | High |
| 3 | ~~Icon mapping simplification (6.4)~~ | Low | Medium |
| 4 | ~~Panel state persistence (6.5)~~ | Low | Medium |
| 5 | ~~Easing in AST assignments (6.6)~~ | Medium | Medium |
| 6 | ~~InlineItem naming cleanup (6.2)~~ | Low | Low |
| 7 | ~~Uniform AST actor abstraction (6.7)~~ | High | Medium |
| 8 | ~~Finish property registry write dispatch (6.8)~~ | Medium | High |
| 9 | ~~Wire up easing extraction from modifiers (6.9)~~ | Low | Medium |
| 10 | ~~Anonymous label round-trip in source editor (6.10)~~ | Medium | Medium |
| 11 | Green tree / trivia AST (2.2) | Very High | Low (polish) |

---

## 6. Architectural Debt

Discovered during implementation of chapters 1 & 3.

### 6.1 Property Registry Auto-Dispatch

**Location:** `crates/animatix/src/timeline/property_engine.rs`, `property_registry.rs`, `track.rs`

**Status:** Partially complete. `TrackFieldRef`/`TrackFieldMut` enums and `AnimationTrack::field_ref`/`field_mut` methods centralize the field-to-variant mapping. The 5 read-only dispatch functions (`read_property_value`, `has_keyframe_at`, `count`, `times`, `easing`) now use these enums, eliminating ~200 repetitive match arms. `write_property_field` still has per-field defaults and requires manual updating.

**Issue:** Adding one new property requires coordinated changes in 4+ files and ~7 separate match arms (write, read, has_keyframe_at, count, times, easing, inject). This is the #1 source of "forgot to update X" bugs. Both 3.7 (shadow/glow) and 3.8 (backdrop_blur) hit this.

**Fix:** Replace manual per-field dispatch with a macro or derive that registers the property once and auto-generates all engine boilerplate. Each `ActorField` variant should carry enough metadata (type, default, applicable_to) that the engine can dispatch generically.

**Effort:** Medium.

---

### 6.2 InlineItem Naming & Exhaustiveness

**Location:** `crates/animatix/src/ast.rs`

**Status:** Fixed.

**Issue:**
- ~~`SlotFill` uses `slot_name` but callers often assume `slot`.~~ (Fixed)
- ~~Rename `slot_name` → `slot` for consistency.~~ (Fixed)
- ~~Add `#[non_exhaustive]` to `InlineItem` so new variants don't break downstream matches.~~ (Fixed)
- ~~Use a deterministic label generator (`parent_label + index`) instead of a global counter.~~ (Fixed)

**Effort:** Low.

---

### 6.3 Coordinate Transform Struct

**Location:** `crates/animatix-gui/src/app/panels/mod.rs`, `app/preview/mod.rs`

**Issue:** Preview ↔ scene space conversion is duplicated across 3+ functions with different signatures (`preview_screen_to_scene`, `preview_scene_to_screen`, `scene_to_screen`). Adding zoom/pan (3.1) required threading new parameters through ~18 call sites.

**Fix:** Introduce a `PreviewTransform` struct that holds `zoom`, `pan`, `preview_rect`, and `scene_dimensions`. Provide `to_scene(pos)` and `to_preview(point)` methods. All overlays, selection handles, and drag logic use this single struct.

**Effort:** Low.

---

### 6.4 Icon Mapping Simplification

**Location:** `crates/animatix/src/primitives/`, `crates/animatix-gui/src/app/icons.rs`

**Issue:** Every new primitive requires:
1. `icon_id()` returning an opaque string in the primitive definition
2. A match arm in `app/icons.rs` `phosphor_icon()` mapping that string to a Phosphor constant
The test catches misses, but it's pure duplication.

**Fix:** Change `Primitive::icon_id()` to return `&'static str` that *is* the Phosphor constant path (or import `egui_phosphor` in the core crate and return the glyph directly). Eliminate `icons.rs` `phosphor_icon()` entirely.

**Effort:** Low.

---

### 6.5 Panel State Persistence

**Location:** `crates/animatix-gui/src/app/mod.rs` (`PreviewPaneState`), `app/panels/mod.rs`

**Issue:** The scene list transition editor stores open/close state in egui temp data (`ui.data(|d| d.get_temp::<bool>(...))`). This made 1.4 (scrubber → transition editor linkage) awkward — the transport bar can't open an editor in another panel. The workaround was adding `open_transition_editor: Option<String>` to `PreviewPaneState`, which is semantically wrong (preview state shouldn't know about panel UI state).

**Fix:** Introduce a `PanelState` struct (or expand `UiActions`) that holds transient panel UI state: `open_transition_editor`, `active_modal`, `expanded_sections`, etc. `PreviewPaneState` should only hold playback-related data.

**Effort:** Low.

---

### 6.6 Easing in AST Assignments

**Location:** `crates/animatix/src/ast.rs`, `timeline/track.rs`, `source_edit.rs`

**Status:** Fixed.

**Issue:** `PropertyTrack<T>` stores `(value, Easing)` per keyframe, but the AST represents easing via `Assignment.modifiers`. This impedance mismatch made 1.3 (keyframe easing editor) complex — `SourceEdit::SetKeyframeEasing` must find the right assignment and mutate its modifier list, which may not round-trip cleanly.

**Fix:** ~~Add an `easing: Option<Easing>` field directly to `Stmt::Assignment`. Update the parser and `to_source` serializer. Modifiers can remain for advanced use, but the common case gets a first-class field.~~ (Fixed)

---

### 6.7 Uniform AST Actor Abstraction

**Location:** `crates/animatix/src/ast.rs`

**Status:** Fixed.

**Issue:** `Stmt` had 6+ actor-like variants (`ActorDecl`, `Text`, `Math`, `Code`, `Svg`, `Image`) with inconsistent fields. Generic operations like reparenting required large manual match blocks, and some variants lacked `props` entirely (`Svg`, `Image`).

**Fix:** ~~Introduce a uniform `ActorDecl`-like structure that all actor statements share.~~ All actor types (`Text`, `Math`, `Code`, `Svg`, `Image`) are now represented as `Stmt::ActorDecl` with their type in the `ty` field. The parser emits `ActorDecl` for all actors; the timeline builder dispatches by type name via the primitive system. This eliminated 5 enum variants and ~40 redundant match arms.

**Effort:** High. Massive AST refactor touching parser, serializer, timeline builder, module system, source editor, analyzer, and renderer.

---

### 6.8 Finish Property Registry Write Dispatch

**Location:** `crates/animatix/src/timeline/property_engine.rs`

**Status:** Fixed.

**Issue:** `TrackFieldRef`/`TrackFieldMut` enums eliminated ~200 repetitive match arms from the 5 read-only dispatch functions. But `write_property_field` still had per-field defaults and required manual updating.

**Fix:** ~~Extend the `TrackFieldRef`/`TrackFieldMut` approach to writes.~~ Added `ActorField::default_value()` that maps each field to its default `PropertyValue`. Refactored `write_property_field` to use `track.field_mut(field)` → `TrackFieldMut` → typed write helper with the default from `ActorField::default_value()`. Special cases (group field diagnostics, shape type conversion) handled explicitly before the uniform path.

**Effort:** Medium.

---

### 6.9 Wire Up Easing Extraction from Modifiers

**Location:** `crates/animatix/src/parser.rs`, `timeline/build/mod.rs`

**Status:** Fixed.

**Issue:** `Stmt::Assignment` had `easing: Option<Easing>`, but the parser always set it to `None`. The easing value was still buried in `Assignment.modifiers`.

**Fix:** ~~Have the parser extract `ease: ...` modifiers and populate `easing` directly.~~ Added `extract_easing()` helper in the parser that scans modifiers for `ease: ...`, removes the modifier, and populates the `easing` field. Updated `process_assignment_statement` to accept an `explicit_easing` parameter and use it, falling back to modifier parsing if `None`.

**Effort:** Low.

---

### 6.10 Anonymous Label Round-Trip in Source Editor

**Location:** `crates/animatix-gui/src/source_edit.rs`, `to_source.rs`

**Status:** Fixed.

**Issue:** Anonymous items (e.g. `Button, text: "OK"`) got synthetic `__anon_*` labels when converted to `Stmt` by the source editor. On serialization back to source, they retained these labels instead of becoming anonymous again.

**Fix:** Use the `__anon` prefix convention to detect anonymous items. `stmt_to_inline_item` creates `InlineItem::Anonymous` for labels starting with `__anon`. `serialize_actor_like_stmt` emits anonymous syntax (no label) for these labels. This preserves anonymity through edit round-trips without changing `ActorDecl`'s structure.

**Effort:** Low.

---

### 6.11 Primitive System / Timeline Builder Alignment

**Location:** `crates/animatix/src/timeline/build/mod.rs`, `primitives/`

**Status:** Fixed as part of 6.7.

**Issue:** The timeline builder had special-case dispatch for Text/Math/Code/Svg/Image that bypassed the primitive system's `ActorKind::build()` methods. The primitives existed but weren't used.

**Fix:** Resolved by 6.7: all actors now flow through `ActorDecl` → `process_actor_decl` → `find_actor_kind` → primitive `build()`.
