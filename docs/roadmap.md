# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. Deferred Features

### 1.1 GUI Inspector: Animated Geometry Editing

**Status:** `Polygon.points` and `Path.commands` assignments work at source level. GUI widgets deferred.
**Location:** `crates/animatix-gui/src/app/panels/inspector/`.

No widget exists for editing variable-length lists of `Vec2` points or path commands. The inspector currently displays `"[N pts]"` / command string as read-only labels.

**Effort:** High (custom multi-point / command editor).

---

### 1.2 Multi-Scene GUI: Scene List & Composition Timeline

**Status:** Pending. Hard cuts supported; transition blending deferred.
**Location:** `crates/animatix-gui/src/app/panels/`.

The runtime supports multi-scene composition (`# SceneName`, `play SceneName [transition, duration]`), but the GUI lacks a scene list / composition timeline panel. Transition blending (dual render) is Phase 7; only hard cuts work in Phase 1.

**Effort:** Medium–High.

---

## 2. Architecture / Cleanup Debt

### 2.1 Unified Property System: Primitive Trait Dispatch + Registry Metadata

**Status:** The property registry claims to be the source of truth, but several properties bypass the generic engine entirely via hardcoded special cases in `assignments.rs`.

**Special cases that bypass the registry/engine:**
- `url` (Image/Svg) — requires file I/O at assignment time
- `position` / `at` — compound property resolving to `PositionBinding` + `[f32; 2]`
- `text` / `math` / `code` — requires glyph path recompilation after write

**Chosen direction:** Move assignment-phase behavior into the `Primitive` trait. Keep the registry as pure metadata for GUI/completions/docs.

---

#### Design Principles

1. **Registry = declarative metadata.** Answers "what properties exist, what are their types, defaults, groups, and applicability?"
2. **Primitive = imperative behavior.** Answers "how do I handle this property assignment?"
3. **Generic engine = default implementation.** Handles the 80% case: parse value → write to `AnimationTrack` field.

---

#### New Trait Method

Add to `Primitive` in `crates/animatix/src/primitives/mod.rs`:

```rust
/// Handle a property assignment at the assignment phase.
/// Return `true` if the primitive handled it (bypassing generic engine).
/// Default implementation delegates to the generic property engine.
fn handle_assignment(
    &self,
    track: &mut AnimationTrack,
    property: &str,
    value: &Expr,
    ctx: &AssignmentCtx,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> bool {
    // Default: look up in registry, use generic engine
    generic_property_engine::write(track, property, value, ctx, env, diagnostics, subject)
}
```

`AssignmentCtx` carries timing context:

```rust
pub struct AssignmentCtx {
    pub t_start_ms: f64,
    pub t_end_ms: f64,
    pub easing: Easing,
    pub instant_delayed: bool,
    pub duration_ms: f64,
}
```

---

#### What Each Primitive Would Override

| Primitive | Properties to override | Why |
|---|---|---|
| **Image** | `url` | File I/O: `load_image()` → `track.image.add_keyframe()` |
| **Svg** | `url` | File I/O + parsing: `read_to_string()` → `parse_svg()` → `track.svg_paths = ...` |
| **Text / Math / Code** | `text` / `math` / `code` | Post-write: write string → `recompile_text_at_assignment()` |
| **All actors** | `position` / `at` | Compound: `resolve_position_binding()` → write `position` + `position_binding` |
| **Rect / Ellipse / Line / Polygon / Path** | *none* | Generic engine handles `color`, `size`, `stroke`, etc. |

---

#### Migration Plan

**Phase 1 — Extract special cases from `assignments.rs` into primitive methods:**

1. Add `handle_assignment()` to `Primitive` with default generic-engine delegation.
2. Move `assignments.rs:135-191` (Image/Svg `url` handling) into `Image::handle_assignment()` and `Svg::handle_assignment()`.
3. Move `assignments.rs:81-109` (position/`at` handling) into a shared helper called from all primitives' `handle_assignment()`.
4. Move `assignments.rs:113-131` (text content recompilation) into `Text::handle_assignment()`, `Math::handle_assignment()`, `Code::handle_assignment()`.
5. Replace the special-case blocks in `assignments.rs` with:
   ```rust
   if let Some(primitive) = find_primitive(&track.kind.type_name()) {
       if primitive.handle_assignment(track, property, value, &ctx, env, diagnostics, &subject) {
           return;
       }
   }
   // Fallback: generic engine (already handles everything else)
   ```

**Phase 2 — Clean up `assignments.rs`:**

After Phase 1, `assignments.rs` should contain only:
- Target path resolution (`a.b.c` → track lookup)
- Property name extraction
- The primitive dispatch + generic engine fallback
- Unsupported property diagnostic

Remove the hardcoded match arms for `url`, `position`, `text`, etc.

**Phase 3 — Registry becomes metadata-only:**

Remove any behavioral code that reads from the registry (the registry is for GUI/analyzer/docs only). The `field: ActorField` mapping in `PropertySchema` stays — it tells the inspector which track field to show keyframes for.

---

#### Registry Remains Intact

The `PROPERTY_REGISTRY` does not change. It continues to serve:
- **GUI inspector:** What properties to show, in which groups, with what widgets
- **Analyzer completions:** Which properties are valid for which actor types
- **Docs:** Human-readable property reference

Example entry stays exactly as-is:

```rust
schema!("tip_length", ValueType::F32, F::empty(),
        ActorField::VectorShapeGroup,
        Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }),
        Applicable::ShapeKinds(&[S::Line]),
        |_| PropertyValue::F32(10.0))
```

---

#### Files to Touch

| File | Change |
|---|---|
| `primitives/mod.rs` | Add `handle_assignment()` to `Primitive` trait; add `AssignmentCtx` struct |
| `primitives/image.rs` | Override `handle_assignment()` for `url` |
| `primitives/svg.rs` | Override `handle_assignment()` for `url` |
| `primitives/text.rs` | Override `handle_assignment()` for `text` |
| `primitives/math.rs` | Override `handle_assignment()` for `math` |
| `primitives/code.rs` | Override `handle_assignment()` for `code` |
| `timeline/assignments.rs` | Replace special-case blocks with primitive dispatch |
| `timeline/property_engine.rs` | Export `generic_property_engine::write()` as public API |

---

#### Dependencies

- **Blocked by:** 2.7 (Remove unused `actor_type` param) — clean up `Primitive` trait first.
- **Blocks:** Nothing directly, but enables cleaner property handling for all future primitives.

**Effort:** Medium–High.

---

### 2.2 Silent Fallback to Rect

**Status:** `ActorKindId::from_type_name("_")` and `shape_type_for_actor("_")` silently default unknown type names to `Rect`.
**Files:** `track.rs:62`, `shapes/mod.rs:130`, `shapes/mod.rs:138`.

This masks typos and makes debugging hard. A misspelled type name creates a Rect instead of reporting an error.

**Fix:** Return `Option` or `Result` and let the caller report a diagnostic.

**Effort:** Low.

---

### 2.3 `VectorShapeState` Union Struct

**Status:** `VectorShapeState` holds fields that are only valid for specific shape types (e.g., `line_from`/`line_to` for Line only, `arc_angles` for Ellipse only, `regular_polygon_sides` for Polygon only). There is no compile-time or runtime validation of which fields are active.
**File:** `shapes/mod.rs:78-88`.

**Impact:** Fragile — code can read/write fields that don't apply to the current shape. Wastes space (every shape carries all fields).

**Fix:** Split into per-shape state structs, or at minimum document the active-field mapping and add runtime assertions.

**Effort:** Medium.

---

### 2.4 Hardcoded Stroke Fallback Color

**Status:** `build_vello_path` uses pure black `(0, 0, 0, 255)` as the forced stroke fallback when `force_stroke` is true but no stroke is set.
**File:** `shapes/mod.rs:447`.

This ignores the shape's own color and the colorscheme. A Line with no explicit stroke renders as black instead of using `stroke.default` or the shape color.

**Fix:** Use the shape's `color` or `stroke_color` instead of hardcoded black.

**Effort:** Low.

---

### 2.5 `sides` Property Hidden from GUI

**Status:** `sides` is marked `Applicable::Never` in the property registry, so the GUI inspector won't show it. But `sides` is a legitimate user-facing property on `Polygon` (triggers regular polygon generation when ≥ 3).
**File:** `property_registry.rs:324`.

**Fix:** Change `Applicable::Never` to `Applicable::ShapeKinds(&[S::Polygon])`.

**Effort:** Trivial.

---

### 2.6 Stringly-Typed Actor Type Dispatch

**Status:** `ActorKindId::from_type_name` matches on raw strings. Adding a new primitive requires updating this match expression; typos are only caught at runtime.
**File:** `track.rs:40-63`.

**Fix:** Derive the mapping from the `PRIMITIVES` registry instead of hardcoding strings.

**Effort:** Medium.

---

### 2.7 Unused `actor_type` Parameter in Primitive Trait

**Status:** `apply_defaults`, `apply_property`, and `finalize_state` all take `actor_type: &str`. After removing backward-compat aliases, this parameter is no longer meaningful — the primitive itself knows its type.
**File:** `primitives/mod.rs:143`.

**Fix:** Remove the `actor_type` parameter from these trait methods.

**Effort:** Low.

---

## 3. Long-Term / Speculative

### 3.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 3.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 3.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 4. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Unified centralized property system (2.1) | Medium–High | High |
| 2 | Silent fallback to Rect (2.2) | Low | High |
| 3 | Multi-Scene GUI scene list / composition timeline (1.2) | Medium–High | High |
| 4 | `VectorShapeState` union cleanup (2.3) | Medium | Medium |
| 5 | GUI Inspector: point / path command editors (1.1) | High | Medium |
| 6 | Hardcoded stroke fallback color (2.4) | Low | Low |
| 7 | `sides` property visibility (2.5) | Trivial | Low |
| 8 | Stringly-typed actor dispatch (2.6) | Medium | Low |
| 9 | Remove unused `actor_type` param (2.7) | Low | Low |
| 10 | Green tree / trivia AST (3.2) | Very High | Low (polish) |
