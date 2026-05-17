# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. Deferred Features

### 1.1 Canvas Polygon Vertex Editing

**Status:** Deferred. `Polygon.points` works at source level; preview canvas has move/scale/rotate but no vertex editing.
**Location:** `crates/animatix-gui/src/app/preview/`.

The inspector displays `"[N pts]"` as a read-only label. A popup point table was considered (Chapter 2.1, now removed) but provides poor UX compared to dragging vertices directly on the canvas. Polygon points are stored in actor-local space; the existing `DragState` machine and scene↔screen transforms can be extended with an `EditVertices` variant.

**Scope:** Polygon only. Path commands (SVG beziers) require a full pen-tool mode and are deferred indefinitely.
**Effort:** Medium.

---

### 1.2 Multi-Scene GUI: Scene List & Composition Timeline

**Status:** Partially implemented. Scene list and transition visualization exist; drag-to-reorder and transition editing deferred.
**Location:** `crates/animatix-gui/src/app/panels/`, `crates/animatix-gui/src/app/shell/transport_bar.rs`.

The sidebar Scenes tab now displays each scene's play target and transition type/duration beneath the scene name. The transport bar scrubber shows transition overlap regions as semi-transparent stripes with transition-type labels.

**Remaining:**
- Drag-to-reorder scenes in the sidebar scene list
- Edit transition type/duration directly in the scene list or transport bar
- Click a scene block in the transport bar to jump to that scene

**Completed:** Scene list with transitions, transport bar transition overlays.

---

## 2. GUI / Inspector Debt

*(Empty — inspector widgets are adequate for current scope. Geometry editing belongs in the preview canvas; see 3.11.)*

---

## 3. GUI Preview Drag System Debt

**Location:** `crates/animatix-gui/src/app/panels/preview/`, `crates/animatix-gui/src/app/preview/`.

The preview drag system uses a state-machine (`DragState` enum) supporting move, scale (8 handles), rotate, and reorder operations. Core mechanics work, but several standard visual editor features are missing.

### 3.1 No Multi-Select

**Status:** Only one actor can be selected at a time. No Shift/Ctrl-click to add to selection, no marquee/box selection.
**Impact:** Cannot move, scale, or rotate multiple actors simultaneously.
**Effort:** Medium.

---

### 3.2 No Pivot / Origin Manipulation

**Status:** Rotation and scaling always use the actor center as pivot. No way to change the pivot point.
**Impact:** Rotating or scaling around a corner or edge requires manual position math.
**Effort:** Medium.

---

### 3.3 No Grid Snapping

**Status:** No snap-to-grid for position, rotation, or scale.
**Impact:** Precise alignment requires typing values in the inspector.
**Effort:** Low–Medium.

---

### 3.4 No Alt-Drag Duplicate

**Status:** Hold-Alt-to-duplicate is not implemented.
**Impact:** Common workflow for copying actors is unsupported.
**Effort:** Low.

---

### 3.5 No Keyboard Transform Shortcuts

**Status:** Arrow keys for nudge, Delete for remove, R/S/M mode keys, etc. are not implemented in the preview canvas.
**Impact:** All transform operations require mouse.
**Effort:** Low.

---

### 3.6 No Bounding-Box (Marquee) Selection

**Status:** Dragging on empty space does nothing. No way to select multiple overlapping actors at once.
**Impact:** See 3.1 — multi-select is impossible.
**Effort:** Low.

---

### 3.7 Handle Hit Radius Is Fixed

**Status:** `HANDLE_HIT_RADIUS = 10.0` pixels, not scaled by zoom or DPI.
**Impact:** Handles may be too small to hit on high-DPI displays or when zoomed out.
**Effort:** Trivial.

---

### 3.8 No Handle Tooltips

**Status:** Hovering a scale handle shows a resize cursor but no text label.
**Impact:** New users can't tell corner vs. edge handle behavior.
**Effort:** Trivial.

---

### 3.9 Nested Transform Not Supported

**Status:** Child actors in layout containers are layout-managed and cannot be moved/scaled/rotated individually; only reordering works.
**Impact:** Cannot fine-tune positions of children within a Row/Col/Grid.
**Effort:** Medium–High (requires layout override mechanism).

---

### 3.10 Reorder Preview Is Limited

**Status:** Shows ghost + insertion line during drag, but no persistent visual feedback until drag ends.
**Impact:** Users can't see the final order until they release the mouse.
**Effort:** Low.

---

### 3.11 Polygon Vertex Editing

**Status:** Not implemented. Selected polygons show a bounding box with scale/rotate handles, but individual vertices cannot be dragged.
**Location:** `crates/animatix-gui/src/app/preview/`.

Polygon points are stored in actor-local space (`track.points: Vec<[f32; 2]>`). The preview drag system already handles scene↔screen transforms, hit-testing (`hit_test_handle`), and property-edit emission. Adding vertex editing requires:

- New `DragState::EditVertices { actor, vertex_index, start_points }` variant
- Render vertex handles (small circles) in world space via the actor's `local_transform`
- Hit-test vertex handles before falling through to body/handle checks
- Inverse-transform drag delta from world back to local space
- Emit `PropertyEdit::PointList` on drag, wired through `apply_property_edit_to_track`

**Scope:** Polygon only. Path/SVG bezier editing requires a pen-tool mode (control handles, cubic/quadratic bezier math, SVG string reconstruction) and is deferred.
**Effort:** Medium.

---

## 4. Scene Transitions

**Status:** Visual blending implemented (fade + 4 wipe directions). Easing curves pending.

### 4.1 Phase 7: Visual Transition Blending

**Goal:** Composite two scenes during a transition period using GPU shaders.

**Architecture:**
```
Render Scene A → Texture A
Render Scene B → Texture B
Composite Pass  → Fullscreen quad shader mixes A/B based on progress + transition type
```

**Remaining:**
- Apply easing curves to transition progress (currently linear)

**Completed:** Dual offscreen targets, `TransitionCompositor` WGSL shader, video export integration, GUI preview integration, chunk boundary handling, background blending.

---

### 4.2 Extensible Transition System (Option A)

**Goal:** Replace hardcoded `TransitionType` enum with a plugin-style registry so new transitions can be added without touching the parser, renderer, or GUI.

**Problem:** Adding "slide" requires changing enum, parser match arms, shader switch cases, and GUI dropdowns.

**Design:**

```rust
// Transition becomes ID + parameters
pub struct Transition {
    pub id: String,                    // "fade", "wipe", "slide", "custom"
    pub duration_ms: u64,
    pub params: Vec<(String, Expr)>,   // ("direction", "left"), ("blur", 10)
    pub easing: Easing,
}

// Registry-driven definitions
pub struct TransitionDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub params: &'static [TransitionParam],
    pub default_duration_ms: u64,
}

// Parser becomes generic
// play Scene [wipe, direction: left, 500ms]
// play Scene [slide, direction: up, distance: 50, 400ms]
```

**Benefits:**
- Add transitions by registering a `TransitionDef` + shader snippet
- Transitions can have custom parameters (direction, distance, blur amount)
- GUI dropdown auto-generates from registry
- Analyzer completions auto-generate from registry

**Implementation:**
1. Replace `TransitionType` enum with `Transition.id: String`
2. Build `TRANSITION_REGISTRY` with schema for each transition
3. Update parser to generic param parsing
4. Update renderer to data-driven shader (transition ID → shader case)
5. Update composition engine to use `id` instead of enum

**Files:** `ast.rs`, `parser.rs`, `composition.rs`, `renderer/transition.rs`, `renderer/video.rs`, `preview_surface.rs`

**Effort:** 3–4 days.

**Dependencies:** Blocked by 4.1 (Phase 7 must land first).

---

## 5. Architecture / Cleanup Debt

### 3.1 Unified Property System: Primitive Trait Dispatch + Registry Metadata

**Status:** Implemented. `Primitive::handle_assignment()` added. Image/Svg `url` and text content special cases moved into respective primitives. Registry is pure metadata.

**Files:** `primitives/mod.rs`, `primitives/{image,svg,text,math,code}.rs`, `timeline/assignments.rs`.

---

### 3.2 Silent Fallback to Rect

**Status:** Fixed. `ActorKindId::from_type_name` and `shape_type_for_actor` return `Option`. Unknown types emit `unknown-actor-type` diagnostic.

**Files:** `timeline/track.rs`, `timeline/shapes/mod.rs`, `timeline/build/mod.rs`, `timeline/build/plot.rs`, `diagnostics.rs`.

---

### 3.3 `VectorShapeState` Union Struct

**Status:** Fixed. Split into per-shape enum variants (`RectState`, `EllipseState`, `LineState`, `PolygonState`, `PathState`). Each shape only carries fields it uses.

**Files:** `timeline/shapes/mod.rs`, `primitives/{rect,ellipse,line,polygon,path}.rs`, `timeline/build/mod.rs`, `timeline/assignments.rs`, `timeline/scene_eval.rs`.

---

### 3.4 Hardcoded Stroke Fallback Color

**Status:** Fixed. `build_vello_path` forced-stroke fallback now uses the shape's `color` instead of pure black.

**File:** `timeline/shapes/mod.rs`.

---

### 3.5 `sides` Property Hidden from GUI

**Status:** Fixed. Changed `Applicable::Never` to `Applicable::ShapeKinds(&[S::Polygon])`.

**File:** `timeline/property_registry.rs`.

---

### 3.6 Stringly-Typed Actor Type Dispatch

**Status:** Fixed. `ActorKindId::from_type_name` iterates `PRIMITIVES` registry. `PrimitiveDescriptor::for_actor_type` also registry-driven.

**Files:** `timeline/track.rs`, `timeline/primitive.rs`.

---

### 3.7 Unused `actor_type` Parameter in Primitive Trait

**Status:** Fixed. Removed `_actor_type: &str` from `apply_defaults`, `apply_property`, `finalize_state`.

**Files:** `primitives/mod.rs`, `primitives/{rect,ellipse,line,polygon,path}.rs`, `timeline/shapes/mod.rs`, `timeline/shapes/primitives.rs`.

---

## 4. Long-Term / Speculative

### 4.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 4.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 4.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 5. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Multi-Scene GUI scene list / composition timeline (1.2) | Medium–High | High |
| 2 | Preview drag: multi-select + marquee (3.1, 3.6) | Medium | High |
| 3 | Preview drag: grid snapping (3.3) | Low–Medium | Medium |
| 4 | Preview: polygon vertex editing (3.11) | Medium | Medium |
| 5 | Preview drag: pivot manipulation (3.2) | Medium | Medium |
| 6 | Preview drag: Alt-drag duplicate (3.4) | Low | Medium |
| 7 | Preview drag: keyboard shortcuts (3.5) | Low | Medium |
| 8 | Scene transitions: easing curves (4.1 remaining) | Low | Medium |
| 9 | Scene transitions: extensible system (4.2) | Medium | Medium |
| 10 | Preview drag: handle hit radius (3.7) | Trivial | Low |
| 11 | Preview drag: handle tooltips (3.8) | Trivial | Low |
| 12 | GPU memory profiling: per-frame allocations, staging belt growth, renderer cache retention | Medium | Medium |
| 13 | Green tree / trivia AST (5.2) | Very High | Low (polish) |
