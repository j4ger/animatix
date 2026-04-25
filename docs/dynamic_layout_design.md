# Dynamic Layout Flow Design

## Problem Statement

The current layout system is a **declaration-time measure/place contract** — positions are baked as keyframes during `Timeline::build()` and never recomputed at frame time. From `layout.rs`:

> Layout is a declaration-time measure/place contract, not a per-frame reflow.
> It is evaluated once when a layout container (Row, Col, Grid, Stack) is applied,
> and does not re-sample when animated tracks change later.

This means:
- When a child's `size` changes via animation, siblings do **not** reflow
- When a child's `scale` changes, container positions stay frozen
- Container properties (`gap`, `align`) cannot be animated
- Text content changes that affect measured bounds do not trigger relayout

The test `test_row_layout_does_not_reflow_from_scaled_child_animation` explicitly codifies this as the intended behavior.

## Goal

Enable **dynamic layout flow**: layout containers should recompute child positions per-frame based on current track values (size, placement_mode, etc.), while preserving backward compatibility for existing scenes.

## Architectural Redesign

### Core Principle

Refactor layout from a **build-time mutator** (writes keyframes) into a **frame-time pure function** (returns computed positions):

```
Current:  AST → build() → [bake positions] → evaluate() → scene
Proposed: AST → build() → [metadata only] → evaluate() → [layout pass] → scene
```

### Phase 1: Extract Pure Layout Functions (Non-Breaking)

Refactor `layout.rs` so that the layout algorithm is callable as a pure function independent of timeline mutation.

Current `apply_container_layout()`:
- Reads `track.size.last_value()` (final declared size)
- Writes `track.position.add_keyframe(t_ms, [x, y], Linear)` directly

New design:
- Extract `compute_row_layout()`, `compute_col_layout()`, `compute_grid_layout()`, `compute_stack_layout()` as pure functions
- They take `&[(String, [f32; 2])]` (child label + sampled size) and container params
- They return `BTreeMap<String, [f32; 2]>` (computed positions)
- `apply_container_layout()` calls the pure function + writes keyframes (unchanged behavior)

### Phase 2: ContainerMetadata (Non-Breaking)

Add a new struct to `Timeline` that stores container properties for frame-time access:

```rust
#[derive(Clone, Debug)]
pub struct ContainerMetadata {
    pub layout_type: LayoutType,      // Row, Col, Grid, Stack
    pub gap: f32,
    pub align: String,
    pub cols: Option<usize>,
    pub child_order: Vec<String>,     // stable iteration order
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutType {
    Row,
    Col,
    Grid,
    Stack,
}
```

Populate `container_metadata` in `build.rs` when processing container actors. Existing behavior is unchanged.

### Phase 3: LayoutEngine (Non-Breaking)

Add `LayoutEngine` to `Timeline`:

```rust
pub struct LayoutEngine;

impl LayoutEngine {
    /// Pure function — samples track sizes at time_ms, returns computed positions.
    /// Does NOT mutate tracks.
    pub fn compute_layout_for_time(
        &self,
        container_label: &str,
        metadata: &ContainerMetadata,
        time_ms: u64,
        tracks: &BTreeMap<String, AnimationTrack>,
        nodes: &BTreeMap<String, SceneNode>,
    ) -> BTreeMap<String, [f32; 2]>;
}
```

Key implementation detail: use `track.size.evaluate(time_ms)` instead of `track.size.last_value()`. This is the single most important change — it enables layout to respond to animated size changes.

### Phase 4: Per-Frame Layout Integration (Opt-In)

Modify `scene_eval.rs::evaluate_node()` to:

1. Before processing children, check if this node is a layout container
2. If yes, call `layout_engine.compute_layout_for_time()`
3. Store computed positions in a temporary `EvalContext`
4. When resolving child positions, use computed layout position if available and child is `LayoutManaged`

Add a config option to enable dynamic layout:
```animatix
config { dynamic_layout: true }
```

When `dynamic_layout: false` (default), behavior is identical to today.
When `dynamic_layout: true`, layout-managed children use per-frame computed positions.

### Phase 5: Migration to Default Dynamic Layout (Future)

Once dynamic layout is proven stable:
- Make it the default behavior
- Deprecate `apply_container_layout()` position-baking
- Remove baked position keyframes from build-time
- Update all tests to expect dynamic behavior

## Data Structure Changes

### `mod.rs` — Additions

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutType { Row, Col, Grid, Stack }

#[derive(Clone, Debug)]
pub struct ContainerMetadata {
    pub layout_type: LayoutType,
    pub gap: f32,
    pub align: String,
    pub cols: Option<usize>,
    pub child_order: Vec<String>,
}

pub struct LayoutEngine;
```

### `Timeline` struct additions

```rust
pub container_metadata: BTreeMap<String, ContainerMetadata>,
pub layout_engine: LayoutEngine,
pub dynamic_layout: bool,  // config option, default false
```

### `track.rs` — No changes needed

`PropertyTrack<T>` already supports animated values. `placement_mode` is already a track.

## Handling Edge Cases

### placement_mode transitions

A child can switch from `LayoutManaged` → `Manual` via `at` assignment. At frame time:
- If `placement_mode.evaluate(time_ms) == Manual`, skip in layout computation
- The child's `position` track continues to control its position
- Transition is instantaneous at the keyframe boundary (consistent with current discrete interpolation)

### Text/Math/Code size changes

Text content changes already produce `size` keyframes at build time via `declarations_text.rs`. When content changes, measured bounds are stored as `size` keyframes. With dynamic layout, these animated sizes naturally trigger reflow.

### Container property animation

Phase 1-4 keeps `gap`, `align`, `cols` as static metadata. Future work can promote them to tracks or allow modifier overrides.

### Performance

Per-frame layout is O(M) for M children. For initial implementation, recompute every frame without caching. The arithmetic is simple (addition, multiplication, max). Profile before adding caching complexity.

## Migration Path

| Phase | Action | Breaking? | Tests |
|-------|--------|-----------|-------|
| 1 | Extract pure layout functions | No | None changed |
| 2 | Add ContainerMetadata population | No | None changed |
| 3 | Add compute_layout_for_time() | No | None changed |
| 4 | Integrate into scene_eval, add config | No | Rename frozen test, add dynamic test |
| 5 | Make dynamic default | Yes | All layout tests updated |

## Files Modified

| File | Changes |
|------|---------|
| `mod.rs` | Add `LayoutType`, `ContainerMetadata`, `LayoutEngine`, `dynamic_layout` flag |
| `layout.rs` | Refactor into pure functions; add `LayoutEngine::compute_layout_for_time()` |
| `build.rs` | Populate `container_metadata`; parse `dynamic_layout` config |
| `scene_eval.rs` | Add layout pass; use computed positions for LayoutManaged children |
| `timeline_tests.rs` | Rename `test_row_layout_does_not_reflow...`, add dynamic tests |

## Relationship to Existing Design Documents

- **`layout_design.md`**: This document extends the shipped layout model. The core placement modes (LayoutManaged, Scene-relative, Manual) remain unchanged. Dynamic layout adds per-frame recomputation to LayoutManaged placement.
- **`implementation_plan.md`**: Dynamic layout was previously listed under "Deferred Architectural Work" ("sampled relayout / animated-size-triggered container recomputation"). This design enables it as an active feature.
- **`architecture_refactor_plan.md`**: The refactor to pure functions aligns with internal architecture cleanup goals.

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Performance regression from per-frame layout | Start without caching; profile first |
| Breaking existing scenes | Opt-in via `config { dynamic_layout: true }` |
| Text size keyframes out of sync | Already handled by build-time measurement |
| placement_mode interpolation edge cases | Discrete interpolation at t=0.5, consistent with current behavior |
| Nested container complexity | DFS evaluation already handles nesting; layout runs before child transform |
