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

**Status:** Completed. Scene list with transitions, transport bar overlays, drag-to-reorder, inline transition editing, and click-to-jump.
**Location:** `crates/animatix-gui/src/app/panels/`, `crates/animatix-gui/src/app/shell/transport_bar.rs`.

The sidebar Scenes tab displays each scene's play target and transition type/duration beneath the scene name, with inline editing (click the transition badge). The transport bar scrubber shows transition overlap regions and supports clicking scene blocks to jump. Scenes can be reordered via drag handles.

---

## 2. GUI / Inspector Debt

*(Empty — inspector widgets are adequate for current scope. Geometry editing belongs in the preview canvas; see 3.11.)*

---

## 3. GUI Preview Drag System Debt

**Location:** `crates/animatix-gui/src/app/panels/preview/`, `crates/animatix-gui/src/app/preview/`.

The preview drag system uses a state-machine (`DragState` enum) supporting move, scale (8 handles), rotate, and reorder operations. Core mechanics work, but several standard visual editor features are missing.

## 4. Scene Transitions

**Status:** Visual blending implemented (fade + 4 wipe directions). Easing curves applied.

### 4.1 Phase 7: Visual Transition Blending

**Goal:** Composite two scenes during a transition period using GPU shaders.

**Architecture:**
```
Render Scene A → Texture A
Render Scene B → Texture B
Composite Pass  → Fullscreen quad shader mixes A/B based on progress + transition type
```

**Completed:** Dual offscreen targets, `TransitionCompositor` WGSL shader, video export integration, GUI preview integration, chunk boundary handling, background blending, easing curves (linear, ease-in/out, bounce, elastic, back, expo).

---

### 4.2 Extensible Transition System

**Status:** Implemented. `TransitionType` enum replaced with `Transition.id: String`. `transition_registry::REGISTRY` is the single source of truth.

**Registry:** `crates/animatix/src/transition_registry.rs` defines `TransitionDef { id, display_name, default_duration_ms, shader_case }`. Adding a new transition only requires adding an entry to `REGISTRY` and a corresponding shader case.

**Parser:** Uses `transition_registry::find()` for generic ID lookup instead of hardcoded match arms.

**Renderer:** `TransitionCompositor::render` takes `transition_id: &str` and maps to shader case via `transition_registry::shader_case()`.

**GUI:** Dropdowns auto-generate from `REGISTRY` — no GUI code changes needed for new transitions.

**Files:** `ast.rs`, `parser.rs`, `composition.rs`, `renderer/transition.rs`, `renderer/offscreen.rs`, `renderer/video.rs`, `preview_surface.rs`, GUI panels.

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
