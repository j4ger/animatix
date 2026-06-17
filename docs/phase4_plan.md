# Phase 4: Interaction Layer Upgrade Plan

## Goal
Ship the final GUI design-language migration by replacing monolithic canvas drag and ad-hoc keyboard/motion code with typed gesture routing, focus-aware shortcut dispatch, and token-backed animation helpers while keeping `cargo check` green after every step.

## Current Findings

- `crates/animatix-gui/src/app/preview/drag_handler.rs` is the current 763-line drag pipeline. It handles drag start hit testing, per-frame updates, drag-end source flushing triggers, and marquee selection in one function.
- `DragState` and `ToolMode` live in `crates/animatix-gui/src/app/preview/mod.rs`; overlays and cursor feedback in `preview/context.rs` still inspect `DragState`, so the first migration should keep `DragState` stable.
- The preview panel integration point is `crates/animatix-gui/src/app/panels/preview_panel.rs`, which currently imports and calls `drag_handler::handle_preview_drag` around line 322.
- `PreviewContext` in `crates/animatix-gui/src/app/preview/context.rs` already centralizes coordinate transforms, timeline access, selection, commands, and drag state; reuse it as the first gesture context rather than inventing parallel state.
- `runtime.rs::handle_keyboard_shortcuts` mixes global shortcuts, tool switching, actor nudging, and direct store mutations; some extra shortcuts still live in `GuiShell::ui` in `app/mod.rs`.
- `design_tokens/motion.rs` has duration and easing constants only; `animate_value_with_time` call sites are in `crates/animatix-gui/src/cell_editor/render.rs`, `app/shell/toolbar.rs`, and `app/panels/sidebar.rs`.
- Phase 3 command splitting is already present. New interaction code should emit existing `DocumentCommand`, `ActorCommand`, `PlaybackCommand`, `ViewCommand`, `ViewAction`, and `ShellAction::Drag`, not rework command domains.
- Drag source updates rely on `GuiShell::handle_property_edit` deferring source edits while `InteractionStore::is_dragging()` is true, then `ShellAction::Drag(DragEvent::DragEnded)` calls `flush_pending_drag_edits()`. Every migrated drag handler must preserve that lifecycle.

## Plan

1. Add gesture primitives behind the existing drag path.
   - Files: `crates/animatix-gui/src/app/preview/gesture.rs`, `crates/animatix-gui/src/app/preview/mod.rs`, `crates/animatix-gui/src/app/panels/preview_panel.rs`.
   - Change: define `Gesture`, `PointerButton`, `GestureResult`, and `GestureHandler`; add `pub mod gesture;` to `preview/mod.rs`; include `Tap`, `DoubleTap`, `SecondaryTap`, `DragStart`, `DragMove`, `DragEnd`, `Hover`, and `ScrollZoom` variants carrying screen positions plus modifiers.
   - Expected outcome: the type layer compiles with no behavior changes because `preview_panel.rs` still calls `handle_preview_drag`.
   - Verification: `cargo check -p animatix-gui`.

2. Create `gesture_router.rs` as an adapter, not a rewrite.
   - Files: `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/mod.rs`, `crates/animatix-gui/src/app/panels/preview_panel.rs`.
   - Change: add `GestureRouter::from_egui(ui, preview_rect, response)` or `GestureRouter::dispatch_preview(...)` that converts raw egui state into typed gestures, then initially delegates to `drag_handler::handle_preview_drag` through a `LegacyDragHandler` wrapper.
   - Expected outcome: `preview_panel_ui` calls `gesture_router::handle_preview_gestures(ctx, ui, preview_rect, &response)` and behavior remains identical.
   - Verification: `cargo check -p animatix-gui`; manually confirm move, scale, rotate, marquee still work because all logic is still legacy.

3. Extract common drag utilities before moving modes.
   - Files: `crates/animatix-gui/src/app/preview/drag_handler.rs`, `crates/animatix-gui/src/app/preview/drag_utils.rs`, `crates/animatix-gui/src/app/preview/mod.rs`.
   - Change: move pure helpers for selected-actor start positions, locked checks, body hit testing, resize-mode lookup, position-binding edits, snap resolution, and drag-end keyframe finalization into `drag_utils.rs` without changing callers.
   - Expected outcome: `drag_handler.rs` shrinks but remains the only behavior owner; extracted helpers are testable and reusable by gesture handlers.
   - Verification: `cargo check -p animatix-gui`; add focused unit tests for body hit testing and position-binding edit selection if helper signatures are pure enough.

4. Migrate move gestures first.
   - Files: `crates/animatix-gui/src/app/preview/gestures/move_actor.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/mod.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: add `MoveActorGesture` that claims actor-body drags for `ToolMode::Move` and `ToolMode::Select` fallback; preserve multi-select moves, Shift axis lock, grid snapping, guide/actor/container/keyframe snap lines, `PositionBinding` handling, Alt duplicate, and Shift detach-from-layout behavior.
   - Expected outcome: move interactions no longer run through the matching `DragState::Move` branches in `drag_handler.rs`, but still write `DragState::Move` so overlays keep working.
   - Verification: `cargo check -p animatix-gui`; manual smoke: move one actor, multi-move, Shift axis lock, grid snap, guide snap, Alt duplicate, layout-managed Shift detach.

5. Migrate scale gestures.
   - Files: `crates/animatix-gui/src/app/preview/gestures/scale.rs`, `crates/animatix-gui/src/app/preview/gesture_router.rs`, `crates/animatix-gui/src/app/preview/drag_handler.rs`.
   - Change: move handle hit testing and `DragState::Scale` updates into `ScaleGesture`; preserve 8 handles, pivot-as-anchor, Shift/uniform scaling, primitive resize mode (`size` vs `scale`), `PREVIEW_MIN_ACTOR_SIZE`, `PREVIEW_MIN_SCALE`, and position-binding edits.
   - Expected outcome: scale handles are owned by a focused handler and overlays still read `DragState::Scale`.
   - Verification: `cargo check -p animatix-gui`; manual smoke: corner scale, edge scale, Shift uniform, text/auto-measured actor scale, pivot anchor, keyframe creation at drag end.

6. Migrate rotate, pivot, vertex, and motion-path gestures.
   - Files: `crates/animatix-gui/src/app/preview/gestures/rotate.rs`, `pivot.rs`, `vertex.rs`, `motion_path.rs`, `gesture_router.rs`, `drag_handler.rs`.
   - Change: extract the remaining selected-actor transform gestures with router priority `pivot/handles > vertex > motion_path > rotate > move`; keep `DragState::Rotate`, `MovePivot`, `EditVertices`, and `MotionPath` variants for overlays.
   - Expected outcome: transform-specific logic is isolated; `drag_handler.rs` no longer owns selected-actor handle/keyframe manipulation.
   - Verification: `cargo check -p animatix-gui`; manual smoke: rotate handle/body rotate in rotate tool, Shift snap degrees, pivot drag, polygon vertex drag, motion-path keyframe drag.

7. Migrate reorder and marquee gestures.
   - Files: `crates/animatix-gui/src/app/preview/gestures/reorder.rs`, `marquee.rs`, `gesture_router.rs`, `drag_handler.rs`, `preview/selection.rs` if marquee helpers are moved there.
   - Change: add `ReorderGesture` for layout-managed child drag only after real `response.drag_started()` movement, and `MarqueeGesture` for empty-canvas drags using `selection.marquee_start/current`.
   - Expected outcome: double-click-to-edit text remains unblocked by reorder; marquee multi-select continues to support Shift/Ctrl/Cmd toggling and locked-actor filtering.
   - Verification: `cargo check -p animatix-gui`; manual smoke: reorder row/column children, double-click text actor in layout, marquee replace selection, marquee toggle selection.

8. Delete or retire the legacy drag handler.
   - Files: `crates/animatix-gui/src/app/preview/drag_handler.rs`, `crates/animatix-gui/src/app/preview/mod.rs`, `crates/animatix-gui/src/app/panels/preview_panel.rs`.
   - Change: remove `handle_preview_drag` once all branches have equivalent gesture handlers; either delete the file or leave a tiny module that re-exports shared drag types only if needed.
   - Expected outcome: all canvas drag interactions flow through `GestureRouter -> GestureHandler -> ShellAction/DragState`.
   - Verification: `cargo check -p animatix-gui`; `cargo test -p animatix-gui` if the extracted helpers added tests.

9. Add the keyboard registry framework.
   - Files: `crates/animatix-gui/src/app/interaction/mod.rs`, `crates/animatix-gui/src/app/interaction/keyboard.rs`, `crates/animatix-gui/src/app/mod.rs`.
   - Change: introduce `Shortcut`, `ShortcutScope`, `FocusContext`, `ShortcutRegistry`, and `KeyboardAction`; register global, canvas, timeline, and text-safe shortcuts. Add `pub(crate) mod interaction;` to `app/mod.rs`.
   - Expected outcome: shortcuts can be matched declaratively with focus guards instead of inline `ctx.input` chains.
   - Verification: `cargo check -p animatix-gui`; unit-test shortcut matching for Ctrl/Cmd aliases, Shift guards, and `egui_wants_keyboard_input()` filtering.

10. Replace `runtime.rs::handle_keyboard_shortcuts` incrementally.
    - Files: `crates/animatix-gui/src/app/runtime.rs`, `crates/animatix-gui/src/app/interaction/keyboard.rs`, `crates/animatix-gui/src/app/mod.rs`.
    - Change: make `handle_keyboard_shortcuts` build a `FocusContext` and dispatch registry output to existing commands. Keep Ctrl+Z/Ctrl+Shift+Z/Ctrl+Y, Ctrl+S, Ctrl+R, and Ctrl+Shift+R active when text input is focused; gate Space, arrows, tool shortcuts, scene numbers, comma/period, Delete/Backspace, duplicate, zoom, palette, find, group, and ungroup behind non-text focus.
    - Expected outcome: behavior matches current shortcuts, with additions from the design language: Backspace deletes, `[`/`]` map to `PrevKeyframe`/`NextKeyframe`, `,`/`.` map to `FrameStepBackward`/`FrameStepForward` if that is the accepted Interaction Language behavior.
    - Verification: `cargo check -p animatix-gui`; manual smoke: type in editor without triggering Space/arrows/tools, Ctrl+S still saves, canvas arrows nudge selection, arrows scrub when no selection.

11. Move remaining shell-local shortcuts into the registry.
    - Files: `crates/animatix-gui/src/app/mod.rs`, `crates/animatix-gui/src/app/interaction/keyboard.rs`.
    - Change: migrate the `GuiShell::ui` input block for editor sync (`Y`), insertion palette (`Shift+A`, `/`), copy (`Ctrl/Cmd+C`), and paste (`Ctrl/Cmd+V`) into the registry, using `KeyboardAction` variants for palette and clipboard operations when no command exists.
    - Expected outcome: all global shortcut definitions live in `interaction/keyboard.rs`; `GuiShell::ui` only handles rendered UI and command draining.
    - Verification: `cargo check -p animatix-gui`; manual smoke: copy/paste selected actors, insertion palette, editor sync toggle.

12. Add motion helper API.
    - Files: `crates/animatix-gui/src/app/design_tokens/motion.rs`, optionally `crates/animatix-gui/src/app/components/anim.rs` or `crates/animatix-gui/src/app/design_tokens/anim.rs`, `components/mod.rs` or `design_tokens/mod.rs` depending on placement.
    - Change: define `Transition { duration: f32, easing: CubicBezier }`, `TransitionKind` or constructors (`fast`, `normal`, `slow`), and `anim::transition(ctx, id, target, transition) -> f32`. Because egui only provides linear `animate_value_with_time`, first implementation should centralize duration and repaint behavior; document easing as stored metadata until cubic sampling is added.
    - Expected outcome: call sites stop hard-coding durations while using `motion::FAST`, `NORMAL`, `SLOW`, and easing constants.
    - Verification: `cargo check -p animatix-gui`; unit-test cubic-bezier sampling only if the helper actually evaluates easing.

13. Migrate scattered animations to `anim::transition`.
    - Files: `crates/animatix-gui/src/cell_editor/render.rs`, `crates/animatix-gui/src/app/shell/toolbar.rs`, `crates/animatix-gui/src/app/panels/sidebar.rs`.
    - Change: replace `animate_value_with_time` calls with named transitions: header button hover uses `FAST`, divider hover uses `FAST`, toolbar build pulse keeps its explicit longer duration with a comment or named `SLOW`-derived transition, sidebar slide uses `NORMAL + STANDARD`, reset calls use `INSTANT`.
    - Expected outcome: `grep -R "animate_value_with_time" crates/animatix-gui/src` has no direct UI call sites except inside the helper.
    - Verification: `cargo check -p animatix-gui`; grep check for remaining direct calls.

14. Final verification and cleanup.
    - Files: all Phase 4 files.
    - Change: remove dead imports, remove now-unused legacy helpers, add inline justification comments to any new `#[allow(dead_code)]`, and update `docs/gui_design_language.md` only if the implemented shortcut mapping intentionally differs from the current spec.
    - Expected outcome: repository has a single interaction entry point for canvas gestures, a single shortcut registry, and a single motion helper API.
    - Verification: `cargo check`; `cargo test -p animatix-gui`; if core crates are touched unexpectedly, also run `cargo test -p animatix`.

## Files to Touch

- `crates/animatix-gui/src/app/preview/gesture.rs` — new typed gesture events, results, and handler trait.
- `crates/animatix-gui/src/app/preview/gesture_router.rs` — new raw egui-to-gesture adapter and handler priority orchestration.
- `crates/animatix-gui/src/app/preview/gestures/*.rs` — new focused handlers for move, scale, rotate, vertex, pivot, motion path, reorder, and marquee.
- `crates/animatix-gui/src/app/preview/drag_utils.rs` — shared hit testing, snapping, position-binding, and drag-end utilities extracted from the legacy handler.
- `crates/animatix-gui/src/app/preview/drag_handler.rs` — shrink incrementally, then delete or retire after parity.
- `crates/animatix-gui/src/app/preview/mod.rs` — register new modules and keep `DragState`/`ToolMode` public to overlays during migration.
- `crates/animatix-gui/src/app/panels/preview_panel.rs` — swap `handle_preview_drag` import/call for `gesture_router::handle_preview_gestures` and preserve preview rendering order.
- `crates/animatix-gui/src/app/preview/context.rs` — add small gesture-friendly helpers only when they remove duplication; keep rendering helpers separate from gesture mutation.
- `crates/animatix-gui/src/app/interaction/mod.rs` — new module root for interaction-layer framework.
- `crates/animatix-gui/src/app/interaction/keyboard.rs` — new focus-aware shortcut registry and dispatch mapping.
- `crates/animatix-gui/src/app/runtime.rs` — replace inline shortcut checks with registry dispatch.
- `crates/animatix-gui/src/app/mod.rs` — register `interaction`, remove shell-local shortcut block after migration, and route any non-command keyboard actions.
- `crates/animatix-gui/src/app/design_tokens/motion.rs` — add `Transition` helpers around existing duration and `CubicBezier` tokens.
- `crates/animatix-gui/src/app/components/anim.rs` or `crates/animatix-gui/src/app/design_tokens/anim.rs` — add the actual `transition(...)` wrapper if not placed directly in `motion.rs`.
- `crates/animatix-gui/src/cell_editor/render.rs` — migrate hover/divider animations to the helper.
- `crates/animatix-gui/src/app/shell/toolbar.rs` — migrate build indicator animation to the helper.
- `crates/animatix-gui/src/app/panels/sidebar.rs` — migrate sidebar slide animation to the helper.
- `docs/gui_design_language.md` — update only if implemented key bindings intentionally resolve the current `,`/`.` vs `[`/`]` ambiguity differently than the spec text.

## Handler Priority

1. `RulerGuideGesture` or existing ruler block, if ruler dragging is folded into the router later.
2. `PivotGesture` and scale/rotate handle gestures for selected actors.
3. `VertexGesture` when `ToolMode::Vertex` or select-mode vertex hit test succeeds.
4. `MotionPathGesture` for keyframe dots.
5. `RotateGesture` when `ToolMode::Rotate` body/handle criteria match.
6. `ScaleGesture` when `ToolMode::Scale` handle criteria match.
7. `ReorderGesture` for layout-managed children after drag movement starts.
8. `MoveActorGesture` for actor body drags.
9. `MarqueeGesture` for empty-canvas drags.
10. Selection tap/double-tap handling remains in `PreviewContext::handle_preview_selection` until it is deliberately migrated, because it owns context menu and inline text editing behavior.

## Keyboard Registry Details

- `FocusContext` should expose at least `wants_keyboard`, `has_selection`, `drag_active`, `inline_edit_active`, `command_palette_open`, `find_replace_open`, `workspace_switcher_open`, `unsaved_dialog_open`, and `tool_mode`.
- `ShortcutScope::TextSafe` should allow undo/redo/save/reload/rebuild while text inputs are focused.
- `ShortcutScope::Global` should be blocked by modal text fields unless the shortcut is explicitly text-safe.
- `ShortcutScope::Canvas` should cover actor nudging, delete/backspace, duplicate, tool switching, zoom-to-selection/all, Escape drag/tool cancellation, and playback shortcuts.
- Dispatch should prefer commands: `DocumentCommand` for save/reload/rebuild/undo/redo/property edits, `ActorCommand` for delete/duplicate/group, `PlaybackCommand` for playback/scrubbing/keyframes/frame step, `ViewCommand` for tool and zoom changes, and `ViewAction` for palette/find dialogs.
- Actor nudging can either keep direct `handle_property_edit` calls inside a `KeyboardAction::NudgeSelected` executor or emit `DocumentCommand::PropertyEdit` edits through `pending_actions`; the former is less invasive because it already computes timeline positions synchronously in `runtime.rs`.

## Motion Helper Details

- Preferred API:
  ```rust
  pub struct Transition {
      pub duration: f32,
      pub easing: CubicBezier,
  }

  pub const HOVER: Transition = Transition { duration: FAST, easing: STANDARD };
  pub const PANEL: Transition = Transition { duration: NORMAL, easing: STANDARD };

  pub fn transition(ctx: &egui::Context, id: egui::Id, target: f32, transition: Transition) -> f32;
  ```
- If cubic easing is implemented now, use `CubicBezier::sample(progress)` and store previous raw animation progress in egui memory; otherwise centralize duration first and leave a clear TODO in the helper, not at call sites.
- Respect design-language constraints: playhead scrubbing remains instant, panel transitions use `NORMAL + STANDARD`, hover/press feedback uses `FAST`, and any duration greater than `SLOW` needs a local justification comment.

## Migration Order and Checks

1. Gesture primitives/router adapter: `cargo check -p animatix-gui`.
2. Utility extraction: `cargo check -p animatix-gui` plus focused helper tests if added.
3. Move handler: `cargo check -p animatix-gui` and manual move/snap smoke.
4. Scale handler: `cargo check -p animatix-gui` and manual handle smoke.
5. Rotate/pivot/vertex/motion-path handlers: `cargo check -p animatix-gui` and manual transform smoke.
6. Reorder/marquee handlers: `cargo check -p animatix-gui` and manual selection/layout smoke.
7. Legacy drag removal: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`.
8. Keyboard registry foundation: `cargo check -p animatix-gui`; registry unit tests.
9. Runtime and shell shortcut migration: `cargo check -p animatix-gui`; manual shortcut smoke.
10. Motion helper and call-site migration: `cargo check -p animatix-gui`; grep for direct `animate_value_with_time`.
11. Final pass: `cargo check`; `cargo test -p animatix-gui`; run `cargo test -p animatix` only if shared/core behavior changed.

## Risks

- Drag-end flushing is easy to break. Missing `ShellAction::Drag(DragEvent::DragEnded)` leaves deferred source edits unflushed and drag snapshots active.
- `DragState` currently drives overlays, cursor feedback, snap guides, measurement labels, and reorder overlays. Removing or renaming variants before overlay migration will create regressions.
- Selection click, double-click inline editing, context menu, and reorder drag-start semantics overlap. Keep tap/double-tap selection legacy until drag handlers are stable.
- `PreviewContext` has many mutable references; gesture handlers should be stateless structs or short-lived values to avoid borrow conflicts.
- Keyboard shortcut semantics have a spec/code mismatch: docs say `[`/`]` previous/next keyframe and `,`/`.` frame step, while current code uses `,`/`.` previous/next keyframe. Decide and update docs if implementation differs.
- Egui does not directly support cubic-bezier easing in `animate_value_with_time`; a full easing implementation may require storing animation start/end state in egui memory.
- Ruler guide dragging currently lives in `preview_panel.rs`, not `drag_handler.rs`. It can stay in place for Phase 4 unless the team wants all pointer interactions routed through gestures.
