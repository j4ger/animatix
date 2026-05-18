# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. Deferred Features

*(Empty — no deferred features at this time.)*

---

## 2. GUI / Inspector Debt

*(Empty — inspector widgets are adequate for current scope.)*

---

## 3. GUI Preview Drag System

**Location:** `crates/animatix-gui/src/app/panels/preview/`, `crates/animatix-gui/src/app/preview/`.

The preview drag system uses a state-machine (`DragState` enum) supporting move, scale (8 handles), rotate, reorder, polygon vertex editing, and pivot manipulation. Core mechanics work. The remaining gaps are polish and advanced features.

### 3.1 Canvas Pivot Manipulation

**Status:** Implemented. The pivot crosshair is always rendered for the selected actor and can be dragged directly on the canvas. Inverse-transforms world delta to local space to update the pivot offset.

**Shortcuts:** `P` key activates pivot tool mode. Also accessible in Select mode by clicking the crosshair.

**Files:** `preview/mod.rs` (`DragState::MovePivot`), `panels/mod.rs` (drag logic, cursor feedback).

---

### 3.2 Tool-Mode Keyboard Shortcuts

**Status:** Implemented. `M` (Move), `Shift+S` (Scale), `R` (Rotate), `V` (Vertex), `P` (Pivot), `Esc` (cancel drag / return to Select).

Tool modes override the default auto-detect behavior when clicking on an actor body. Handle hit-tests still work in all modes. Cursor feedback reflects the active tool.

**Files:** `runtime.rs` (keybinds), `app/mod.rs` (`ToolMode` state), `panels/mod.rs` (mode-aware drag initiation).

---

### 3.3 DPI-Aware Visual Scaling for Handles and Gizmos

**Status:** Implemented. Handle sizes, rotation handle radius, vertex handles, and pivot crosshair all scale by `pixels_per_point` so they remain physically consistent across Retina/HiDPI and standard displays.

**Philosophy:** Interaction element sizes (handles, hit radii, gizmos) are developer-tuned, not user-configurable. Like Blender, we scale them automatically by display DPI so they feel correct on all hardware. Users should never need to think about handle size.

**Files:** `preview/mod.rs` (`draw_selection_overlay`, `draw_vertex_handles`), `panels/mod.rs` (call sites).

---

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

### 5.1 Unified Property System: Primitive Trait Dispatch + Registry Metadata

**Status:** Implemented. `Primitive::handle_assignment()` added. Image/Svg `url` and text content special cases moved into respective primitives. Registry is pure metadata.

**Files:** `primitives/mod.rs`, `primitives/{image,svg,text,math,code}.rs`, `timeline/assignments.rs`.

---

### 5.2 Silent Fallback to Rect

**Status:** Fixed. `ActorKindId::from_type_name` and `shape_type_for_actor` return `Option`. Unknown types emit `unknown-actor-type` diagnostic.

**Files:** `timeline/track.rs`, `timeline/shapes/mod.rs`, `timeline/build/mod.rs`, `timeline/build/plot.rs`, `diagnostics.rs`.

---

### 5.3 `VectorShapeState` Union Struct

**Status:** Fixed. Split into per-shape enum variants (`RectState`, `EllipseState`, `LineState`, `PolygonState`, `PathState`). Each shape only carries fields it uses.

**Files:** `timeline/shapes/mod.rs`, `primitives/{rect,ellipse,line,polygon,path}.rs`, `timeline/build/mod.rs`, `timeline/assignments.rs`, `timeline/scene_eval.rs`.

---

### 5.4 Hardcoded Stroke Fallback Color

**Status:** Fixed. `build_vello_path` forced-stroke fallback now uses the shape's `color` instead of pure black.

**File:** `timeline/shapes/mod.rs`.

---

### 5.5 `sides` Property Hidden from GUI

**Status:** Fixed. Changed `Applicable::Never` to `Applicable::ShapeKinds(&[S::Polygon])`.

**File:** `timeline/property_registry.rs`.

---

### 5.6 Stringly-Typed Actor Type Dispatch

**Status:** Fixed. `ActorKindId::from_type_name` iterates `PRIMITIVES` registry. `PrimitiveDescriptor::for_actor_type` also registry-driven.

**Files:** `timeline/track.rs`, `timeline/primitive.rs`.

---

### 5.7 Unused `actor_type` Parameter in Primitive Trait

**Status:** Fixed. Removed `_actor_type: &str` from `apply_defaults`, `apply_property`, `finalize_state`.

**Files:** `primitives/mod.rs`, `primitives/{rect,ellipse,line,polygon,path}.rs`, `timeline/shapes/mod.rs`, `timeline/shapes/primitives.rs`.

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

## 7. Settings Panel Philosophy (Blender-Style)

Only expose settings that meaningfully change workflow. Visual/interaction tuning (handle sizes, hit radii, stroke widths) is developer-tuned and DPI-scaled automatically. Users should not need to think about them.

### Exposed to Users

| Setting | Default | Why Exposed |
|---------|---------|-------------|
| Grid snap size | 20 px | Project-dependent density |
| Grid toggle | On | Quick accessibility |
| Keyframe merge window | 50 ms | Editing style preference |
| Playback scrub step | 0.1 s | Timeline scale varies |
| Arrow key nudge (base) | 1 px | Precision preference |
| Arrow key nudge (shift) | 10 px | Coarse positioning |
| Rotation snap angle | 15° | Domain convention varies |
| Undo history limit | 100 | Memory vs safety tradeoff |
| Rebuild debounce | 150 ms | Typing speed vs stability |
| Debug bounding boxes | Off | Debugging toggle |

### Hard-Coded (Developer-Tuned, DPI-Scaled)

| Parameter | Value | Scales with DPI |
|-----------|-------|-----------------|
| Scale handle size | 6 px | Yes |
| Scale handle hit radius | 10 px | Yes |
| Rotation handle offset | 20 px | No (relative to actor bounds) |
| Rotation handle radius | 4 px | Yes |
| Pivot crosshair size | 6 px | Yes |
| Pivot hit radius | 12 px | Yes |
| Vertex handle radius | 4 px | Yes |
| Minimum actor size | 10 px | No |
| Marquee fill/stroke opacity | 30/120 | No |
| Selection stroke width | 1.5 px | No |

---

## 8. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Settings panel expansion (grid size, nudge steps, rotation snap, scrub step, undo limit) | Low–Medium | Medium |
| 2 | GPU memory profiling: per-frame allocations, staging belt growth, renderer cache retention | Medium | Medium |
| 3 | Green tree / trivia AST (6.2) | Very High | Low (polish) |
