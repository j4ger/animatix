# Phase 4 Remaining Gesture Handler Extraction Plan

## Goal

Extract `crates/animatix-gui/src/app/preview/drag_handler.rs::handle_preview_drag` into focused gesture handlers without changing canvas behavior, source-write flushing, or overlay state.

## Assumptions

- This is a behavior-preserving migration; UX changes such as new hit-test precedence or a new overlay state model are out of scope.
- `DragState` remains the compatibility contract for overlays, cursor feedback, property popup hiding, drag snapshots, and source-edit deferral until a later overlay migration.
- `PreviewContext` remains the primary mutable context for gesture handlers; do not introduce parallel timeline, selection, or command state.
- Selection taps, double-click inline text editing, and context-menu behavior stay in `PreviewContext::handle_preview_selection` until drag extraction is complete.
- No blocker was found in the current code. The main implementation constraint is preserving partial-migration fallback to the legacy handler.

## Current Findings

### Drag Modes In `handle_preview_drag`

| Mode | State | Start / hit test | Update behavior | End behavior |
| --- | --- | --- | --- | --- |
| Move actor | `DragState::Move` | Selected actor body or hit region after selected affordances and motion-path keyframes fail. Layout-managed children use Shift to detach to manual placement before moving. Alt duplicates instead of starting a drag. | Compute scene delta from `start_scene`; Shift locks to dominant axis; optional grid snap; optional guide/actor/container/keyframe snap; emit position edit through `drag_utils::emit_position_edit` for every selected actor. | `drag_utils::finalize_drag_keyframes` may create a primary actor `position` keyframe, then `ShellAction::Drag(DragEvent::DragEnded)` flushes pending source edits. |
| Scale | `DragState::Scale` | `ToolMode::Scale` hits nearest of 8 handles; `ToolMode::Select` checks scale handles after vertices. Anchor is pivot offset if non-zero, otherwise the opposite handle. Resize mode comes from primitive metadata (`size` vs uniform `scale`). | Convert scene delta into actor-local axes; resize from handle sign; apply min size; preserve uniform ratio when Shift/start modifier/scale-mode requires it; preserve anchor world position; emit `size` or `scale` plus position edit. | `finalize_drag_keyframes` may create `size` and `position` keyframes, then `DragEnded`. |
| Rotate | `DragState::Rotate` | `ToolMode::Rotate` starts near the rotation handle or actor body hit region. `ToolMode::Select` starts only from the rotation handle. | Compute angle around the stored pivot; normalize delta into `[-PI, PI]`; Shift snaps to `ctx.rotation_snap_degrees`; emit `rotation`. | `finalize_drag_keyframes` may create `rotation` keyframe, then `DragEnded`. |
| Pivot | `DragState::MovePivot` | `ToolMode::Pivot` starts on pivot hit. `ToolMode::Select` checks pivot after vertex, scale, and rotate handle checks. | Convert scene delta to actor-local axes and update `ctx.pivot_offsets`. | No property finalizer today; still emit `DragEnded` so drag state resets. |
| Vertex | `DragState::EditVertices` | `ToolMode::Vertex` uses a larger vertex hit radius. `ToolMode::Select` checks vertices before handles. Requires evaluated non-empty `track.points`. | Convert scene delta to actor-local axes and update one vertex in the captured `start_points`; emit `points` as `PropertyValue::PointList`. | No extra finalizer; `DragEnded` flushes the deferred point-list source edit. |
| Motion path | `DragState::MotionPath` | After selected affordances fail, scan selected actor `position` keyframes and claim a keyframe dot within `hit_radius * 2.0`. | Offset the captured keyframe position by scene delta and emit `position` at `time_s = time_ms / 1000.0` with `create_keyframe: true`. | No extra finalizer; `DragEnded` flushes the deferred keyframe edit. |
| Reorder | `DragState::Reorder` | Layout-managed child body drag, no Shift, only when `response.drag_started()` is true so click/double-click can still reach selection/text editing. | Project mouse to row/column main axis; compare with sibling centers from hit regions or actor props; update `target_index` inside `ctx.drag_state`. | If `source_index != target_index`, emit container `child_order` edit, then `DragEnded` flushes source. |
| Marquee | `selection.marquee_start/current` | If drag starts without a selected actor/raw actor context, store screen-space marquee start/current. | While no `DragState` is active, keep `marquee_current` at the latest pointer. | On pointer release, convert marquee to scene rect, replace or toggle selected actors depending on Shift/Ctrl/Cmd, skip locked actors, and clear marquee state. No `DragEnded` is emitted because no source drag is active. |

### Exact `DragState` Variants

`crates/animatix-gui/src/app/preview/mod.rs` defines these variants and they should remain stable during extraction:

- `None`
- `Move { primary, actors, start_scene }`
- `Scale { actor, handle, start_scene, start_position, start_size, start_rotation, anchor_local, constrain_axis, uniform_ratio, resize_mode, start_scale }`
- `Rotate { actor, start_angle, start_rotation, pivot }`
- `Reorder { actor, container, source_index, target_index, layout_type }`
- `EditVertices { actor, vertex, start_points, start_scene }`
- `MovePivot { actor, start_offset, start_scene }`
- `MotionPath { actor, time_ms, start_position, start_scene }`

### `PreviewContext` Access Used By Gestures

`PreviewContext` already centralizes the state that extracted handlers need:

- State fields: `scene_dimensions`, `preview`, `commands`, `drag_state`, `selection`, `selected_actors`, `hit_regions`, `timeline`, `pivot_offsets`, `tool_mode`, `rotation_snap_degrees`, `composition`, `active_scene`, and `keyframe_mode`.
- Geometry helpers: `preview_screen_to_scene`, `preview_scene_to_screen`, `preview_transform`, `get_actor_props`, and `get_actor_props_at_time`.
- Layout helpers: `is_layout_managed` and `find_layout_container`.
- Selection/text behavior should stay in `handle_preview_selection`; gesture extraction should not duplicate click, double-click, or context-menu code.

### Overlay Dependencies To Preserve

- `render_preview_overlays` reads `DragState::Move` and `DragState::Scale` to draw snap guides and the snap HUD.
- `render_preview_selection_overlay` reads `DragState::EditVertices` for active vertex highlighting, `Move`/`Scale`/`Rotate` for measurement labels, and `Reorder` for the reorder ghost/drop indicator.
- Selection overlays and property popups use `is_dragging = !matches!(drag_state, DragState::None)` to draw drag styling and hide floating cards.
- Marquee overlay reads `selection.marquee_start/current`, not `DragState`.
- Therefore every extracted drag handler must populate the same `DragState` variant with the same fields before emitting any drag update commands.

## Router Wiring Strategy

`GestureRouter::handle_preview_gestures` should become an incremental dispatcher with legacy fallback:

1. Build a per-frame gesture snapshot from egui: latest pointer position, scene position, modifiers, `response.drag_started()`, primary-press fallback, `response.drag_stopped()`, `pointer.any_released()`, `pointer.any_down()`, and middle-button suppression.
2. If `ctx.drag_state` is an extracted variant, route `DragMove` and `DragEnd` directly to that variant's handler and do not call the legacy handler.
3. If `ctx.drag_state` is a non-extracted variant, call `drag_handler::handle_preview_drag` unchanged so partial migration remains safe.
4. On drag start, run extracted start handlers in final hit-test priority. If none claim, call the legacy handler.
5. Each extracted handler returns `GestureResult::Claimed` only when it has fully handled that event. `Claimed` stops dispatch.
6. Recompute or pass `response_drag_started` separately from the broader primary-press fallback because `Reorder` must only start on actual drag movement.

Final drag-start priority should preserve current behavior:

1. Ignore all preview drags while middle mouse is down.
2. For the selected actor, return early if the selected actor is locked.
3. Tool-specific selected affordance pass:
   - `ToolMode::Vertex`: `VertexGesture` only.
   - `ToolMode::Scale`: `ScaleGesture` only.
   - `ToolMode::Rotate`: `RotateGesture` from rotation handle or body.
   - `ToolMode::Pivot`: `PivotGesture` only.
   - `ToolMode::Select`: `VertexGesture`, then `ScaleGesture`, then rotation-handle `RotateGesture`, then `PivotGesture`.
   - `ToolMode::Move`: no selected-affordance claim; fall through.
4. `MotionPathGesture` for selected actor keyframe dots.
5. Body interactions:
   - `MoveActorGesture` handles layout detach when Shift is held.
   - `ReorderGesture` handles layout-managed children when Shift is not held and `response.drag_started()` is true.
   - `MoveActorGesture` handles Alt duplicate and normal actor-body movement.
6. `MarqueeGesture` handles empty-canvas drags.

During partial migration, do not enable a lower-priority select-mode handler ahead of unextracted higher-priority handlers unless it performs the same preflight hit tests. Example: `PivotGesture` can be extracted first for `ToolMode::Pivot`, but select-mode pivot claiming should wait until vertex, scale, and rotate-handle starts are extracted or preflighted.

## Drag-End Lifecycle

All non-marquee extracted handlers should share one drag-end helper that exactly preserves the legacy lifecycle:

1. Clone `old_drag_state` before any reset.
2. Call `drag_utils::finalize_drag_keyframes(&old_drag_state, ctx)` for the existing Move/Scale/Rotate finalizers.
3. If `old_drag_state` is `DragState::Reorder` and `source_index != target_index`, compute the current child order, remove the dragged actor, insert it at `target_index.min(new_order.len())`, and emit `child_order` on the container.
4. Push `ShellAction::Drag(DragEvent::DragEnded)` exactly once.
5. Do not directly set `DragState::None` and do not call `flush_pending_drag_edits()` from preview code. `GuiShell::handle_drag_event` resets interaction state and flushes pending source edits.

## File Structure

Add a `gestures` module under `crates/animatix-gui/src/app/preview`:

- `gestures/mod.rs` — module declarations and handler list exports.
- `gestures/common.rs` — shared frame data, selected-actor context, current time helper, locked checks, resize-mode lookup, affordance preflight helpers, and shared `finish_drag`.
- `gestures/pivot.rs` — `PivotGesture` start/update for `DragState::MovePivot`.
- `gestures/marquee.rs` — `MarqueeGesture` start/update/end for `selection.marquee_start/current`.
- `gestures/motion_path.rs` — `MotionPathGesture` start/update for position keyframe dots.
- `gestures/vertex.rs` — `VertexGesture` start/update for polygon points.
- `gestures/rotate.rs` — `RotateGesture` start/update for rotation handle/body rotation.
- `gestures/scale.rs` — `ScaleGesture` start/update for transform handles.
- `gestures/move_actor.rs` — `MoveActorGesture` body movement, multi-move, snapping, Shift detach, and Alt duplicate.
- `gestures/reorder.rs` — `ReorderGesture` layout-managed child drag/drop.

Keep `drag_handler.rs` until all variants are migrated; then delete it or leave only a temporary compatibility shim if needed by external call sites.

## Extraction Order

Least-risk implementation order, with partial-migration gates to avoid hit-test precedence changes:

1. Router/common lifecycle scaffolding.
2. `PivotGesture` for `ToolMode::Pivot` only.
3. `MarqueeGesture`.
4. `VertexGesture` for `ToolMode::Vertex`, then select-mode vertex.
5. `MotionPathGesture`, gated behind current selected-affordance preflight until all selected affordances are extracted.
6. `RotateGesture` for `ToolMode::Rotate`, then select-mode rotation handle.
7. `ScaleGesture` for `ToolMode::Scale`, then select-mode scale handles.
8. Enable select-mode `PivotGesture` after vertex/scale/rotate select handlers are active.
9. `MoveActorGesture` for Shift detach, Alt duplicate, normal move, multi-move, grid/snap, and position binding.
10. `ReorderGesture` last because it overlaps with layout-managed body drags, text double-click, and `child_order` source edits.
11. Remove legacy drag branches/file after every mode is covered.

## Plan

1. Establish gesture router frame and fallback.
   - Files: `crates/animatix-gui/src/app/preview/gesture.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/mod.rs`.
   - Change: add a `GestureFrame` or equivalent context carrying raw pointer state, modifiers, preview rect, scene position, `response_drag_started`, and release/stop flags; add `pub mod gestures;`; keep fallback to `drag_handler::handle_preview_drag` for all unextracted paths.
   - Expected outcome: no behavior change; router has enough information for extracted handlers without requiring them to read raw egui state repeatedly.
   - Verification: `cargo check -p animatix-gui`; smoke move/scale/rotate still work through legacy fallback.

2. Add shared gesture utilities and drag-end helper.
   - Files: `crates/animatix-gui/src/app/preview/gestures/common.rs`, `crates/animatix-gui/src/app/preview/gestures/mod.rs`, `crates/animatix-gui/src/app/preview/drag_utils.rs`.
   - Change: add helpers for current time, selected actor context, locked actor check, vertex point evaluation, resize-mode lookup, body/hit-region testing, and `finish_drag(old_drag_state, ctx)` that wraps final keyframes, reorder commit, and `DragEnded` emission.
   - Expected outcome: handlers share legacy-equivalent math/lifecycle code and do not duplicate source-flush logic.
   - Verification: `cargo check -p animatix-gui`; add small unit tests only for pure helpers such as resize-mode fallback or body hit-testing if signatures stay pure.

3. Extract `PivotGesture` in explicit Pivot tool mode.
   - Files: `crates/animatix-gui/src/app/preview/gestures/pivot.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: move pivot hit-test start logic for `ToolMode::Pivot` and `DragState::MovePivot` update logic into `PivotGesture`; route active `MovePivot` updates/end through the handler; keep select-mode pivot in legacy for now.
   - Expected outcome: Pivot tool drags update `ctx.pivot_offsets` through the new handler while overlays still draw the pivot marker from actor props plus `pivot_offsets`.
   - Verification: `cargo check -p animatix-gui`; manual smoke: select actor, switch Pivot tool, drag pivot, release, confirm no stuck drag state and other tools still work.

4. Extract `MarqueeGesture`.
   - Files: `crates/animatix-gui/src/app/preview/gestures/marquee.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: move empty-canvas marquee start/update/release selection logic into `MarqueeGesture`; do not emit `DragEnded`; keep selection replacement/toggle and locked-actor filtering identical.
   - Expected outcome: marquee no longer depends on legacy drag handler and remains visually driven by `selection.marquee_start/current`.
   - Verification: `cargo check -p animatix-gui`; manual smoke: marquee replace selection, Shift/Ctrl/Cmd toggle selection, locked actors are skipped, no source flush occurs.

5. Extract `VertexGesture`.
   - Files: `crates/animatix-gui/src/app/preview/gestures/vertex.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: move vertex hit testing for `ToolMode::Vertex` and select-mode vertex priority plus `DragState::EditVertices` update logic; emit the same `points` `PropertyEdit` with `create_keyframe: ctx.keyframe_mode`.
   - Expected outcome: polygon vertex edits are isolated and `render_preview_selection_overlay` still highlights the active vertex from `DragState::EditVertices`.
   - Verification: `cargo check -p animatix-gui`; manual smoke: Vertex tool drag, Select tool vertex drag, rotated polygon local-axis behavior, release flushes point-list source edit.

6. Extract `MotionPathGesture` with selected-affordance preflight.
   - Files: `crates/animatix-gui/src/app/preview/gestures/motion_path.rs`, `crates/animatix-gui/src/app/preview/gestures/common.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`.
   - Change: move position-keyframe dot hit testing and `DragState::MotionPath` update logic; before claiming, check that current tool/select affordances with higher priority would not claim the same pointer while those handlers are still partially migrated.
   - Expected outcome: dragging motion-path keyframes emits timed `position` edits without stealing scale/rotate/pivot/vertex hits.
   - Verification: `cargo check -p animatix-gui`; manual smoke: show motion paths, drag a non-current keyframe dot, confirm the keyframe time is preserved and source flush occurs on release.

7. Extract `RotateGesture`.
   - Files: `crates/animatix-gui/src/app/preview/gestures/rotate.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: move rotate-tool body/handle start logic, select-mode rotation-handle start logic, angle normalization, Shift snapping, and rotation property emission.
   - Expected outcome: rotation measurement overlay still reads `DragState::Rotate`, and drag-end keyframe creation still comes from shared finalization.
   - Verification: `cargo check -p animatix-gui`; manual smoke: rotate handle in Select tool, rotate body in Rotate tool, Shift snap degrees, release creates/flushes rotation keyframe when needed.

8. Extract `ScaleGesture`.
   - Files: `crates/animatix-gui/src/app/preview/gestures/scale.rs`, `crates/animatix-gui/src/app/preview/gestures/common.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: move scale handle hit testing, resize-mode lookup, `DragState::Scale` creation, local delta sizing, uniform-ratio logic, min-size/min-scale constraints, anchor-preserving position calculation, and `size`/`scale`/position emissions.
   - Expected outcome: transform handles are owned by `ScaleGesture`; snap guides and measurement labels still work because `DragState::Scale` is unchanged.
   - Verification: `cargo check -p animatix-gui`; manual smoke: all 8 handles, Shift uniform, edge-handle axis constraint, pivot anchor, text/auto-measured actor scale mode, drag-end size/position keyframes.

9. Enable select-mode pivot claiming.
   - Files: `crates/animatix-gui/src/app/preview/gestures/pivot.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`.
   - Change: add select-mode pivot dispatch after select-mode vertex, scale, and rotate-handle handlers.
   - Expected outcome: Select tool pivot drag behavior matches legacy ordering without requiring legacy fallback for selected affordances.
   - Verification: `cargo check -p animatix-gui`; manual smoke: in Select tool, vertex/scale/rotate handles still win over pivot when overlapping, pivot drags when directly hit.

10. Extract `MoveActorGesture`.
    - Files: `crates/animatix-gui/src/app/preview/gestures/move_actor.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
    - Change: move actor body/hit-region start logic except reorder, Shift layout-detach commands, Alt duplicate command, selected start-position capture, multi-actor update loop, Shift axis lock, grid snapping, snap resolution, and position-binding edits.
    - Expected outcome: actor movement no longer depends on legacy code; `DragState::Move` keeps snap guides, measurement labels, popup hiding, and drag snapshots working.
    - Verification: `cargo check -p animatix-gui`; manual smoke: single move, multi-move, Shift axis lock, grid snap, guide/actor/container/keyframe snap, scene-anchor/percent position binding, Alt duplicate, Shift detach layout-managed child.

11. Extract `ReorderGesture`.
    - Files: `crates/animatix-gui/src/app/preview/gestures/reorder.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
    - Change: move layout-managed child reorder start/update/drop logic; only claim when Shift is not held and `response.drag_started()` is true; emit container `child_order` on release through shared `finish_drag`.
    - Expected outcome: layout reordering is isolated and the reorder ghost/drop-line overlay still reads `DragState::Reorder`.
    - Verification: `cargo check -p animatix-gui`; manual smoke: reorder Row children, reorder Col children, no reorder on simple click, double-click text actor in a layout still starts inline edit, child_order source flushes after release.

12. Remove the legacy drag handler path.
    - Files: `crates/animatix-gui/src/app/preview/drag_handler.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/mod.rs`, `crates/animatix-gui/src/app/preview/gesture.rs`.
    - Change: delete moved branches or remove `drag_handler.rs` entirely; remove router fallback; remove now-unneeded `#[allow(dead_code)]` attributes from gesture types that are in use, preserving required inline justifications for any remaining dead-code allowances.
    - Expected outcome: every preview drag mode flows through `GestureRouter -> gestures/*`; no monolithic drag path remains.
    - Verification: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`; manual full drag regression pass across every mode.

13. Final cleanup and documentation sync.
    - Files: `docs/phase4_plan.md`, `docs/roadmap.md`, optionally `docs/gui_design_language.md` if behavior intentionally differs from the existing spec.
    - Change: mark gesture extraction complete in Phase 4 docs, remove completed roadmap items, and document any intentionally preserved legacy quirks that remain for later cleanup.
    - Expected outcome: docs match the implemented interaction layer and remaining work is visible in `docs/roadmap.md` only.
    - Verification: `cargo check`; `cargo test --no-fail-fast` before committing, per repository guidance.

## Files To Touch

- `crates/animatix-gui/src/app/preview/gesture.rs` — extend gesture/frame context enough for dispatch and remove dead-code allowances as types become live.
- `crates/animatix-gui/src/app/preview/gesture_router.rs` — convert legacy delegator into priority dispatcher with active-state routing and temporary fallback.
- `crates/animatix-gui/src/app/preview/gestures/mod.rs` — new gesture module root.
- `crates/animatix-gui/src/app/preview/gestures/common.rs` — shared hit-test/context/lifecycle helpers.
- `crates/animatix-gui/src/app/preview/gestures/pivot.rs` — pivot drag handler.
- `crates/animatix-gui/src/app/preview/gestures/marquee.rs` — marquee selection handler.
- `crates/animatix-gui/src/app/preview/gestures/motion_path.rs` — motion-path keyframe handler.
- `crates/animatix-gui/src/app/preview/gestures/vertex.rs` — polygon vertex handler.
- `crates/animatix-gui/src/app/preview/gestures/rotate.rs` — rotation handler.
- `crates/animatix-gui/src/app/preview/gestures/scale.rs` — scale/resize handler.
- `crates/animatix-gui/src/app/preview/gestures/move_actor.rs` — actor move, duplicate, snapping, and layout-detach handler.
- `crates/animatix-gui/src/app/preview/gestures/reorder.rs` — layout child reorder handler.
- `crates/animatix-gui/src/app/preview/drag_utils.rs` — reusable helpers shared by legacy and extracted handlers during migration.
- `crates/animatix-gui/src/app/preview/drag_handler.rs` — remove migrated logic incrementally, then delete or retire.
- `crates/animatix-gui/src/app/preview/mod.rs` — register `gestures`; keep `DragState` and `ToolMode` stable.
- `crates/animatix-gui/src/app/preview/context.rs` — only add small helper methods if they remove duplicated access patterns; do not move overlay rendering during this extraction.
- `docs/phase4_plan.md` and `docs/roadmap.md` — update after implementation lands.

## Risks

- Hit-test precedence can change during partial extraction if a lower-priority handler claims before a still-legacy higher-priority handler. Use explicit gates/preflight until the full priority chain is migrated.
- Missing or duplicate `ShellAction::Drag(DragEvent::DragEnded)` can leave source edits unflushed, snapshots stuck, or `DragState` active after release.
- `GestureContext` currently duplicates command access even though `PreviewContext` already owns `commands`; avoid borrow conflicts by either using `PreviewContext` directly or making `GestureContext` a thin frame wrapper without a second mutable command reference.
- `MoveActorGesture` is broad: multi-selection, grid snapping, guide snapping, position bindings, layout detach, and Alt duplicate all live in the same legacy body path.
- `ReorderGesture` overlaps with selection and double-click text editing; it must continue to require real drag movement via `response.drag_started()`.
- Pivot edits are currently stored in `ctx.pivot_offsets` only; extraction should preserve that behavior rather than inventing persistence.
- Existing scale drag-end finalization creates `size`/`position` keyframes, not an explicit `scale` keyframe for `ResizeMode::Scale`; preserve first, then consider a separate bugfix if needed.
- The preview panel computes `is_dragging` before gesture handling and uses it later for rendering in the same frame; avoid changing frame ordering during this migration unless a separate visual-latency fix is intended.
