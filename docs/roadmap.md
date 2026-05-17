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

## 2. GUI / Inspector Debt

### 2.1 Point / Path Command Editors

**Status:** `Polygon.points` and `Path.commands` assignments work at source level. GUI widgets deferred.
**Location:** `crates/animatix-gui/src/app/panels/inspector/`.

No widget exists for editing variable-length lists of `Vec2` points or path commands. The inspector currently displays `"[N pts]"` / command string as read-only labels.

**Effort:** High (custom multi-point / command editor).

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

## 4. Architecture / Cleanup Debt

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
| 4 | GUI Inspector: point / path command editors (2.1) | High | Medium |
| 5 | Preview drag: pivot manipulation (3.2) | Medium | Medium |
| 6 | Preview drag: Alt-drag duplicate (3.4) | Low | Medium |
| 7 | Preview drag: keyboard shortcuts (3.5) | Low | Medium |
| 8 | Preview drag: handle hit radius (3.7) | Trivial | Low |
| 9 | Preview drag: handle tooltips (3.8) | Trivial | Low |
| 10 | Green tree / trivia AST (4.2) | Very High | Low (polish) |
