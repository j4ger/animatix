# Dynamic Layout Flow Design

This document now describes the current dynamic-layout architecture and what remains deferred.

## Current Status

Animatix now has both:

- **build-time baked layout** via `apply_container_layout()`
- **opt-in dynamic layout** via `config { dynamic_layout: true }`

The current pipeline is:

```text
AST → build()
    → seed render/runtime tracks
    → seed dedicated layout_size tracks
    → admit measured children into container_metadata.layout_children
    → optionally bake declaration-time positions

evaluate()
    → if dynamic_layout: recompute layout positions from admitted children
    → scene traversal still follows track.children
```

---

## Core Architectural Split

### Scene graph

- `track.children` remains the traversal/render graph
- children excluded from layout still render if otherwise valid

### Layout graph

- `container_metadata.layout_children` is the admitted layout subset
- layout computation uses this subset only
- admitted children must have seeded `layout_size`

This distinction is now deliberate and is the main architectural invariant added by the migration.

---

## Current Data Model

```rust
pub enum LayoutType {
    Row,
    Col,
    Grid,
    Stack,
}

pub struct ContainerLayoutChild {
    pub label: String,
}

pub struct ContainerMetadata {
    pub layout_type: LayoutType,
    pub gap: f32,
    pub align: String,
    pub cols: Option<usize>,
    pub child_order: Vec<String>,
    pub layout_children: Vec<ContainerLayoutChild>,
}
```

And on each track:

```rust
pub size: Option<PropertyTrack<[f32; 2]>>,      // legacy/general geometric size
pub layout_size: LayoutSizeState,               // authoritative layout measure
```

Layout uses `layout_size`; rendering/runtime compatibility still uses legacy `size` where needed.

---

## Current Layout Engine Behavior

### Build-time layout

`Timeline::apply_container_layout()`:

- reuses the already-registered real `ContainerMetadata`
- reads only admitted children from `container_metadata.layout_children`
- samples `layout_size_last()`
- expects admitted children to have seeded layout size
- writes baked authored positions for layout-managed children only

### Dynamic layout

`LayoutEngine::compute_layout_for_time()`:

- reads only admitted children from `container_metadata.layout_children`
- samples `layout_size_get(time_ms)`
- expects admitted children to have seeded layout size
- returns computed positions for layout-managed children only

### Container backends

- `Row`, `Col`, and `Grid` use the Taffy-backed layout adapter
- `Stack` remains special-cased and places all admitted children at origin

---

## Admission Rules

At build time, each container validates its children.

- If a child has seeded `layout_size`, it is admitted into `layout_children`
- If a child does not have seeded `layout_size`, it is excluded from layout admission
- If that excluded child was layout-managed, a `LayoutSizeFallback` warning is emitted

Important: despite the diagnostic code name, layout no longer falls back to `[50, 50]` inside admitted layout computation. The warning now means **excluded from admission**, not **positioned with fallback size**.

---

## Dynamic Layout Scope

When `config { dynamic_layout: true }` is enabled:

- admitted children are re-sampled from `layout_size` per frame
- size animation can trigger recomputed layout positions
- manual placement is still respected at output time

What dynamic layout does **not** currently do:

- it does not rebuild admission at frame time
- it does not animate container metadata like `gap`, `align`, or `cols`
- it does not replace scene traversal membership

So current dynamic layout is:

> dynamic resampling of a static admitted child set

not a fully dynamic scene/layout graph.

---

## What Was Completed Across the Migration

### Completed

1. Added a dedicated `layout_size` path
2. Mirrored layout-relevant builders and assignments into `layout_size`
3. Switched layout to read `layout_size` instead of legacy `size`
4. Added `layout_children` as the layout-authoritative child set
5. Excluded unmeasured children from layout admission
6. Removed layout-engine fallback sizing from admitted-child layout computation
7. Validated both baked and dynamic layout paths end to end

### Still Deferred

1. richer `ContainerLayoutChild` entries than just labels
2. reducing metadata duplication between `child_order` and `layout_children`
3. typed builder outputs instead of mutation-first construction
4. retiring legacy `size` from non-layout subsystems if desired later

`child_order` is still intentionally retained today for authored/debug/test visibility, even though layout execution no longer consumes it.

---

## Relationship to Other Docs

- `layout_design.md` describes the shipped author-facing model
- this document focuses on runtime/build architecture and admission rules

---

## Main Remaining Risk Boundary

The strongest invariant now is local to layout:

> admitted layout children must have seeded `layout_size`

The remaining architectural looseness is outside that boundary:

- scene graph and layout graph are still separate data structures
- builders still mutate tracks directly rather than returning typed build products
- legacy `size` still coexists for compatibility

Those are cleanup/future-architecture topics, not active correctness gaps in the current admitted-child layout path.
