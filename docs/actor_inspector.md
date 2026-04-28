# Actor Inspector Design

## Overview

The Actor Inspector is a new dockable panel in the Animatix GUI that provides read-only inspection of animation actors. It shows:

1. A tree view of all actors in the current timeline
2. Selected actor's properties and current values at the playback time
3. Selected actor's keyframe timeline (all keyframe times with values and easing)

## Motivation

Currently, the GUI provides source editing and preview playback but no way to inspect the compiled timeline structure. Users must read the `.amx` source or add diagnostic prints to understand what actors exist, their properties, or when keyframes fire. An inspector bridges this gap — it makes the timeline's internal state visible and debuggable.

## Architecture

### Panel Integration

The inspector fits into the existing `egui_dock` workspace alongside Explorer, Editor, and Preview.

| Change | File |
|--------|------|
| Add `WorkspaceTab::Inspector` variant | `app.rs` |
| Add `Inspector` match arms in `TabViewer` | `workspace.rs` |
| Add `inspector_ui()` method | `workspace.rs` (or new `inspector.rs`) |
| Add `selected_actor: Option<String>` to state | `GuiShell` → `WorkspaceViewer` |
| Add `select_actor: Option<String>` action | `UiActions` → `handle_actions` |
| Position inspector in default dock layout | `persistence.rs` |

### Default Layout

```
Explorer(15%) | Editor(middle) | Preview(30%) | Inspector(18%)
```

The inspector sits to the right of the preview, consistent with animation tools like Blender and After Effects.

### Data Flow

```
DocumentSession.timeline: Option<Timeline>
    │
    ├─ timeline.tracks: BTreeMap<String, AnimationTrack>
    │     ├─ label, children (hierarchy)
    │     ├─ shape_type, first_seen_ms
    │     └─ properties (position, scale, color, opacity, ...)
    │
    ├─ Inspector: reads tracks → actor list + details
    │
    └─ PreviewPaneState.current_time_s → current property values
```

The inspector reads directly from `DocumentSession.timeline` which is already shared via `WorkspaceViewer`. The `PreviewPaneState.current_time_s` is used to compute current property values.

### Selection Model

**Phase 1 (this implementation):** Selection is purely through the inspector's actor list — click an actor label to select it. No preview interaction yet.

**Phase 2 (future):** Click-to-select on the preview canvas via CPU bounding-box hit-testing. The `select_actor` action channel already exists; Phase 2 only needs the hit-testing logic in `evaluate_node()` and a `Sense::click()` on the preview rect.

## UI Design

### Actor List (left/top section)

- Header: "Actors" with count badge (e.g., "Actors — 12")
- Collapsible tree view using `root_nodes` + `AnimationTrack.children`
- Each entry shows:
  - **Label** — prefixed by parent hierarchy for children
  - **Shape type** — icon or text hint (Rect, Circle, Text, Path, Group, ...)
  - **Anonymous badge** — muted "anon" tag for `__anon_*` labels
- Selected actor highlighted with blue background
- Empty state: "No timeline loaded — rebuild to inspect"

### Details Panel (right/bottom section for selected actor)

**Header card:**
- Actor label (bold, large)
- Shape type
- First seen time: `t = 1.50s`

**Properties table:**
| Property | Current Value |
|----------|---------------|
| Position | (100, 200) |
| Size | (240, 120) |
| Scale | 1.00 |
| Rotation | 0.00° |
| Opacity | 1.00 |
| Color | (0.2, 0.5, 1.0, 1.0) |
| Stroke Width | 2.00 |
| Stroke Color | (1.0, 0.0, 0.0, 1.0) |
| Fill Opacity | 0.50 |
| Shape Type | Rect |
| Text Content | "Hello World" |

Only non-`None` properties are shown. Current values are evaluated at `current_time_s` using `PropertyTrack::evaluate()`.

**Keyframe Table:**
| Time (s) | Property | Value | Easing |
|----------|----------|-------|--------|
| 0.00 | position | (100, 200) | Linear |
| 0.00 | color | (0.2, 0.5, 1.0, 1.0) | Linear |
| 1.50 | opacity | 0.50 | EaseInOut |
| 2.00 | position | (300, 400) | Linear |
| 3.00 | scale | 1.50 | EaseOut |

Keyframes are collected across all properties, sorted by time. Each row shows the time, property name, value, and easing function.

## Value Formatting

Each `PropertyTrack<T>` has a different `T` type. Values are formatted via `Debug` for simplicity in Phase 1:

| T | Debug Output |
|---|-------------|
| `[f32; 2]` | `[100.0, 200.0]` |
| `[f32; 4]` | `[0.2, 0.5, 1.0, 1.0]` |
| `f32` | `1.5` |
| `ShapeType` | `Rect` / `Circle` / `Text` / ... |
| `PositionBinding` | `At(x, y)` / ... |

A later refinement could add formatted display (e.g., `(100, 200)` for positions).

## Edge Cases

| Scenario | Handling |
|----------|----------|
| No timeline loaded | Show "No timeline — rebuild to inspect" placeholder |
| Timeline has zero actors | Show "No actors in scene" |
| Actor has no keyframes | Keyframe table empty; properties show default values |
| Actor has no children | Show as leaf node in tree |
| Anonymous actor labels (`__anon_*`) | Show with muted style and "(anonymous)" tag |
| Component with deeply nested children | Tree recursively flattens all descendants |
| Actor appears after current_time_s | Properties show default values (not yet "alive") |

## Implementation Size

| Artifact | Est. Lines |
|----------|-----------|
| `docs/actor_inspector.md` | ~200 (this doc) |
| `crates/animatix-gui/src/app/inspector.rs` | ~350 |
| `crates/animatix-gui/src/app.rs` changes | ~30 |
| `crates/animatix-gui/src/app/workspace.rs` changes | ~40 |
| `crates/animatix-gui/src/app/persistence.rs` changes | ~5 |
| **Total** | ~625 |

## Future: Phase 2 — Click-to-Select

**IMPLEMENTED.** Click-to-select is now shipped.

### Implementation

1. **Hit region collection** (`scene_eval.rs`): During `evaluate_node()`, world-space bounding boxes are computed for each actor via `node_local_bounds()` transformed through the actor's `local_transform` (position + rotation + scale). A fallback uses `half_size` when no path bounds exist.

2. **Storage** (`mod.rs`): Hit regions are stored in `Timeline.hit_regions: RefCell<Vec<(String, kurbo::Rect)>>`, populated during `evaluate_with_debug()` and accessible via `Timeline::hit_regions()`.

3. **PreviewSurface** (`preview_surface.rs`): Caches hit regions after each render call, exposed via `PreviewSurface::hit_regions()`.

4. **Click handling** (`workspace.rs`): The preview area uses `Sense::click()`. On click, pointer coordinates are mapped from preview rect space to scene coordinates using the aspect-ratio-fitted preview scale. Hit regions are tested in reverse order (children before parents, topmost first). A hit sets `UiActions::select_actor`.

5. **Coordinated selection**: Clicking an actor in the preview selects it in the inspector panel. The `select_actor` action channel created in Phase 1 handles this bidirectionally.
