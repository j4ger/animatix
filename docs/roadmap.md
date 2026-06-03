# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Review Fix Sprint (2026-06-03)

Issues surfaced by the 2026-06-03 code review. These are correctness, hygiene, and error-handling gaps that should be closed before larger features land.

### Critical

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 1 | **Modifier error diagnostics** | `apply_modifier_bytecode_program` and `apply_modifier_ir_program` return `Result` but are discarded with `let _`. Collect errors as frame-level diagnostics or at minimum `tracing::warn!`. | `timeline/scene_eval.rs` | 1 day |
| 2 | **Dead `snap.rs` module** | 215-line module is `#![allow(dead_code)]` with zero imports; `drag_handler.rs` inlines identical logic. Either delete `snap.rs` or refactor `drag_handler.rs` to call it. | `app/preview/snap.rs`, `app/preview/drag_handler.rs` | 1 day |
| 3 | **`handle_rename_actor` error handling** | `apply_edit` result is discarded; success status shown even when rename fails. Check return and set error status on failure. | `app/handlers/actor.rs` | 1 hour |

### Warnings

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 4 | **GPU filter pointer comparison** | `src_view as *const _ == tex_a_view as *const _` is fragile. Replace with enum flag tracking (`FilteredSource` already exists). | `renderer/filter_backend.rs` | 2 hours |
| 5 | **ffmpeg temp-file leak** | `std::fs::rename` failure on `*.tmp_muxed.mp4` leaks the temp file. Clean up with `remove_file` on error path. | `renderer/encode/mod.rs` | 30 min |
| 6 | **VM bounds checks** | `LoadConst`, `Jump`, `JumpIfFalse` index without bounds check → panic on corrupt bytecode. Return `EvalError` instead. | `timeline/modifier_runtime/vm.rs` | 1 hour |
| 7 | **Frame cache double-negative** | `cached.has_child_orders != self.child_orders.is_empty()` is correct but confusing. Rewrite or store count instead of bool. | `timeline/scene_eval.rs` | 30 min |
| 8 | **Duplicate `needs_frame_env()` call** | Called twice on cache-miss path. Cache the first result. | `timeline/scene_eval.rs` | 30 min |
| 9 | **Keyboard shortcuts without `wants_keyboard` guard** | `Y` (ToggleEditorSync) and `A` (Action Palette) fire while text input is active. Add `!ui.memory(|m| m.focused().is_some())` guard or equivalent. | `app/mod.rs` | 30 min |
| 10 | **`insertion_palette` error logging** | `apply_edit` failure only sets UI status; add `tracing::warn!` with the actual error. | `app/shell/insertion_palette.rs` | 15 min |
| 11 | **adelay hardcoded stereo** | `adelay={delay_ms}|{delay_ms}` assumes 2 channels. Mono input fails; multi-channel only delays first two. Use `adelay=delays={delay_ms}` for auto-adapt. | `renderer/encode/mod.rs` | 15 min |
| 12 | **`scene_to_screen` pseudo-deprecated** | Comment says deprecated but 44 call sites and no `#[deprecated]` attr. Either migrate callers and annotate, or remove the comment. | `app/preview/mod.rs` | 2 hours |
| 13 | **Handler test coverage** | Only UI commands (TogglePlayback, ScrubTo, etc.) have tests. Domain handlers (CreateActor, DeleteSelectedActors, RenameActor, Save, OpenFile) have zero coverage. | `app/command_handlers.rs` | 3 days |

### Suggestions

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 14 | **Remove always-`Some` `Option` fields** | `filter_backend.rs` has fields that are permanently `Some`, forcing ~15 impossible error checks. Remove `Option` wrapper. | `renderer/filter_backend.rs` | 2 hours |
| 15 | **radius override guard** | Frame-env radius recalculation may overwrite an explicit radius override. Add a check before recomputing. | `timeline/frame_env.rs` | 1 hour |
| 16 | **Remove unused modifier params** | `time_ms` and `scene_dimensions` in `apply_modifier_stmt` are never used; the `#[allow(clippy::only_used_in_recursion)]` is masking real dead code. Remove them. | `timeline/modifier_exec.rs` | 30 min |
| 17 | **Gate test-only helper** | `apply_modifier_stmt_for_test` is `pub` with no `#[cfg(test)]` gate and zero non-test callers. Gate it. | `timeline/modifier_exec.rs` | 15 min |
| 18 | **Avoid clone in track iteration** | `track.children.clone()` allocates every iteration. Refactor to borrow or iterate without clone. | `timeline/scene_eval.rs` | 2 hours |
| 19 | **Batch GPU blit submissions** | `blit()` creates its own encoder + submit, forcing a GPU sync boundary. Accept an external encoder so callers can batch. | `renderer/fullscreen_blit.rs` | 1 day |
| 20 | **Fix typo `PROPORTY` → `PROPERTY`** | Typo in `property_registry.rs`. | `timeline/property_registry.rs` | 5 min |
| 21 | **Replace `eprintln!` with `tracing::warn!`** | Per AGENTS.md policy. | `renderer/core.rs` | 5 min |
| 22 | **Use or remove `statements_mut`** | Marked `#[allow(dead_code)]`. Either use it or delete it. | `app/document_controller.rs` | 30 min |
| 23 | **Extract shared keyframe helper** | `prev_keyframe` and `next_keyframe` logic in `playback.rs` is almost identical. Extract a shared helper. | `app/handlers/playback.rs` | 1 hour |

---

## Phase 1 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 1 |
| 3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 2 |

---

## Phase 2 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 1 |
| 3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |
| 4 | **Snippet AST parsing** | Parse snippet text into `Vec<Stmt>` and insert via `SourceEdit` instead of raw text surgery. Requires lossless parsing (green tree) to preserve formatting. | `app/insertion.rs`, `animatix-green/` | 2 days | 2 |

---

## Order

1. **Review Fix Sprint** (correctness & hygiene — independent of other phases)
2. **Phase 1** (PiP — after syntax and renderer are stable)
3. **Phase 2** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 2 / green tree) | 2 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-2 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-2 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-2 or when plot count becomes a bottleneck again |

| **Amber flash on rewritten timestamps** | Visual polish: when `adjust_following_relative_keyframe` rewrites a relative offset, flash the timestamp label amber for ~300ms. Nice-to-have UX feedback. | When needed |
| **Unify duplicate PropertyValue types** | Two separate `PropertyValue` enums exist: `animatix::timeline::property_engine::PropertyValue` (engine-level) and `animatix_gui::app::commands::PropertyValue` (GUI-level). Different variant names (`F32` vs `Float`, `String` vs `Text`) force conversion logic in `apply_property_edit_to_track`. Unify into one canonical type. | When touching property dispatch again |
| **Replace `node_local_bounds` with trait-based bounds** | `node_local_bounds` takes `&[VelloPath]` forcing callers to materialize paths just for bounds computation. A `trait HasLocalBounds` on `VelloPath`/`TextPath`/`SceneImage` would be cleaner and allow lazy evaluation. Also simplifies the `command_local_bounds` helper (Phase 10b.1). | When touching scene_eval bounds logic |
| **Zero-readback filter compositing (end-to-end)** | Infrastructure is complete: `FullscreenBlitPipeline` supports alpha, `GpuFilterBackend` exposes `render_and_filter_scene_to_view()` and `take_last_filtered_view()`. Remaining work: modify `scene_eval.rs` to not draw filtered images into the Vello scene, and update `PreviewSurface`/`OffscreenRenderer` to blit the GPU texture after the base Vello render. `FilteredSource` tracking should be simplified to avoid fragile pointer comparison. | When filter performance matters |
