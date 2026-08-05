# Animatix GUI Polish — Implementation Plan

Scope: `crates/animatix-gui`. Four workstreams (A unsaved-changes guard, B keyframe
selection wiring, C UX polish). Tasks are fixer-sized (~1–3 files). Each names exact
files/functions and a verification check.

## Premise corrections (verified against current code)

These differ from the task description and reshape the plan:

1. **The unsaved-changes dialog with a pending action already exists and is already
   wired** for `Command::OpenFile`, `Command::SwitchWorkspace`, and `Command::Reload`
   in `app/shell/mod.rs` (`handle_command`). It opens
   `ui_store.unsaved_changes.open(msg, action)` and the dialog in
   `app/mod.rs::unsaved_changes_dialog_ui` already runs Save/Discard/Cancel and replays
   `pending_action` via `execute_unsaved_pending_action`.
   The real remaining gaps are: (a) **older toast-only guards short-circuit before the
   shell routing fires**, and (b) **hot reload never routes through the dialog**.
2. **Snap `snap_line_color` / `snap_hud_label` are already rendered** (in
   `preview_panel.rs:596+` and `context.rs:680+`). The actual dead code is the boolean
   fields on `SnapResult` in `drag_utils.rs:67` (`snapped_guide_h`, `snapped_actor_h`,
   `snapped_container`, `snapped_keyframe`, `snap_hud_text`) — no caller reads them
   (`move_actor.rs:164` only uses `nx`/`ny`).
3. **Keyframe multi-select already exists** but lives in egui temp data
   (`kf_multi_select_id` in `timeline_panel.rs:651`, a `Vec<(String, u64)>`).
   `selected_keyframes: Vec<(String,String,u64)>` exists only on `UiSnapshot`
   (`document/history.rs:14`, behind `#[allow(dead_code)]`) and is never populated.
   There is **no `selected_keyframes` field on the live `SelectionStore`**.
4. The Explorer tab already has a search filter (`sidebar.rs:234
   explorer_content_ui`, `EXPLORER_FILTER_ID`). The **Layers tab
   (`layers_content_ui`) does not** — that is the gap.

---

# (A) Unsaved-changes guard

## Goal
Route every destructive document-replacement path (workspace switch, manual reload,
hot reload) through the existing blocking Save/Discard/Cancel dialog instead of a
non-blocking toast or silent block.

## Plan

### A1. Remove the toast-only guard in the workspace switcher dialog
- File: `app/mod.rs` (~1110–1125, the `Switch` confirm button in the workspace
  switcher modal).
- Current: on confirm it checks `self.document_store.source.is_dirty()` and, if dirty,
  pushes only a `Toast::warning` and never enqueues the command — so the dialog routing
  in `shell/mod.rs` never runs.
- Change: drop the inline dirty check. Always
  `commands.push_back(DocumentCommand::SwitchWorkspace(path).into())` and close the
  switcher. The dirty check now happens centrally in `handle_command`
  (`Command::SwitchWorkspace`), which opens the unsaved-changes dialog with the
  switch as `pending_action`.
- Verify: `cargo check -p animatix-gui`. Manual: with unsaved edits, open the
  workspace switcher, confirm → unsaved-changes dialog appears (not just a toast).

### A2. Remove the redundant internal guard in `handle_switch_workspace`
- File: `app/handlers/file.rs` (`handle_switch_workspace`, ~124–135).
- Current: re-checks `is_dirty()` and returns a toast. Now dead/conflicting since the
  shell already intercepts dirty state before calling the handler.
- Change: delete the `if document_store.source.is_dirty() { ... }` block. Keep the
  directory-validity check. (Handler is only reached post-confirmation.)
- Verify: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`.

### A3. Remove the toast-only guard in `handle_reload`
- File: `app/handlers/file.rs` (`handle_reload`, ~198–204).
- Current: refuses to reload when dirty with a status + toast. The shell
  (`Command::Reload`) already opens the dialog when dirty, so this is now unreachable
  for the dirty case via the command path — but leaving it makes Discard→Reload no-op.
- Change: delete the `if document_store.source.is_dirty()` early-return so that after
  the dialog's Discard path clears `is_dirty` and replays `DocumentCommand::Reload`,
  the reload actually runs.
- Dependency: must land together with A4-style flow; confirm `Discard` sets
  `document.is_dirty = false` before replay (it does — `app/mod.rs:1227`).
- Verify: `cargo test -p animatix-gui`. Manual: edit, Reload → dialog → Discard →
  file reloaded from disk.

### A4. Route hot reload through the dialog (pending action)
- File: `app/mod.rs` (`check_hot_reload`, ~373–405).
- Current: on `ReloadStatus::ShouldReload` with `document.is_dirty`, it only sets a
  status string ("External file changed • reload blocked") and returns.
- Change: when dirty and the dialog is not already open, call
  `self.ui_store.unsaved_changes.open("File changed on disk. Reload and discard your
  unsaved edits?", DocumentCommand::Reload.into())`. Keep the non-dirty branch
  (immediate `reload_from_disk`) unchanged.
- Note on semantics: with the existing dialog, **Save** writes the editor buffer to
  disk (overwriting the external change) then replays `Reload` (which then re-reads
  the just-written file — effectively keeps editor content). **Discard** clears
  `is_dirty` and replays `Reload`, pulling the on-disk change. This matches
  least-surprise; document it in a comment. If "Save" overwriting an external change
  is undesirable, that is a follow-up (would need a 3-way/keep-mine option), out of
  scope here.
- Guard against re-prompting every frame: only open if
  `!self.ui_store.unsaved_changes.is_open`. The watcher's `last_event` is already
  cleared when `ShouldReload` fires, so it won't re-trigger until the next external
  write.
- Verify: `cargo check --workspace`. Manual: edit in-app (don't save), modify the
  `.amx` externally → dialog appears once; Cancel keeps editor; Discard loads disk.

### A5. (Optional consistency) keep the close path intact
- File: `app/mod.rs` (`unsaved_changes_dialog_ui`, ~1157), `app/runtime.rs:561`.
- No change required; verify A1–A4 reuse the same `pending_action`/`pending_close`
  mechanism and the `pending_action == None && pending_close` close branch still works.
- Verify: `cargo test -p animatix-gui`.

## Files to touch (A)
- `app/mod.rs` — workspace switcher confirm (A1), `check_hot_reload` (A4).
- `app/handlers/file.rs` — `handle_switch_workspace` (A2), `handle_reload` (A3).

## Risks (A)
- Ordering: A1 must land with A2 (removing the switcher toast guard without removing
  the handler guard leaves dirty switches silently no-oped). Land A1+A2 together.
- A3 depends on Discard clearing `is_dirty` before replay — verified at `app/mod.rs:1227`.
- A4 re-prompt loop if the `is_open` guard is omitted — explicitly required.
- Hot-reload "Save" overwrites the external edit; flagged above as documented behavior.
- There are multiple `SwitchWorkspace` entry points
  (`app/mod.rs:879` welcome, `:1123` switcher, `app/shell/toolbar.rs:204`); all push
  `DocumentCommand::SwitchWorkspace` and now funnel through `handle_command`, so they
  inherit the guard for free once A1/A2 land. Spot-check each still compiles.

---

# (B) Keyframe selection wiring

## Goal
Promote keyframe multi-selection from panel-local egui temp data into shared store
state so deletion and undo/redo can read a single source of truth, and populate
`UiSnapshot.selected_keyframes`.

## Decision
Use `(actor: String, property: String, time_ms: u64)` as the canonical key (matches
`UiSnapshot.selected_keyframes`). The existing temp-data selection uses
`(actor, time_ms)` (property-agnostic, all properties at that time). To avoid a
behavior change in deletion (which currently deletes across all properties at that
time), store the canonical triple but keep the timeline's per-time selection UX.

## Plan

### B1. Add live selection field to the store
- File: `app/stores/ui_store.rs` (`SelectionStore`, ~12–24).
- Add `pub selected_keyframes: Vec<(String, String, u64)>` and init `Vec::new()` in
  `SelectionStore::new`.
- Verify: `cargo check -p animatix-gui`.

### B2. Populate the snapshot from live state
- File: `app/stores/ui_store.rs` (`UiStore::snapshot`, ~202–212) and
  `restore_snapshot` (~218–224).
- Change `selected_keyframes: Vec::new()` → `self.selection.selected_keyframes.clone()`.
  In `restore_snapshot`, add
  `self.selection.selected_keyframes = snapshot.selected_keyframes;`.
- Remove the now-satisfied TODO comment.
- File: `app/document/history.rs` — drop `#[allow(dead_code)]` on `UiSnapshot` if the
  field is now genuinely read (only if the snapshot is actually consumed; see B5).
- Verify: `cargo check -p animatix-gui`.

### B3. Mirror timeline multi-select into the store
- File: `app/panels/timeline_panel.rs` (`render_timeline_content`, multi-select block
  ~650–660 and write-back ~2181).
- The panel reads/writes `multi_selected: Vec<(String,u64)>` from temp data. After the
  selection mutations settle (near the `d.insert_temp(kf_multi_select_id, ...)` at
  ~2181), translate to canonical triples and write into the store via a command (B4)
  rather than mutating the store directly from the panel (panels emit commands).
- Translation: for each `(actor, time_ms)` resolve matching properties via
  `collect_per_property_keyframes(track)` (already used at ~1791), expanding to triples.
- Verify: `cargo check -p animatix-gui`.

### B4. Add a `SetSelectedKeyframes` command + handler
- Files: `app/commands/mod.rs` (or the keyframe/ui command submodule) — add
  `Command::SetSelectedKeyframes(Vec<(String,String,u64)>)`; wire `From`/dispatch in
  `app/shell/mod.rs::handle_command`.
- File: `app/handlers/ui.rs` (or `keyframe.rs`) — add
  `handle_set_selected_keyframes(ui_store, keyframes)` that sets
  `ui_store.selection.selected_keyframes`.
- Panel (B3) emits this command only when the selection actually changes (diff against
  prior temp-data value) to avoid per-frame churn.
- Verify: `cargo check --workspace`; add a handler unit test asserting the store field
  is set.

### B5. Make deletion read the store selection
- File: `app/panels/timeline_panel.rs` (track-bar context menu "Delete selected",
  ~1790–1820).
- Current source of truth is `multi_selected` (temp). After B3/B4 the store mirrors it;
  keep emitting `Command::DeleteKeyframe` per resolved triple. Optionally add a global
  "Delete selected keyframes" action (keyboard `Delete` when timeline focused) that
  reads `ui_store.selection.selected_keyframes` and emits `DeleteKeyframe` for each —
  this is the concrete consumer that justifies the store field.
- File: `app/runtime.rs` (keyboard routing ~147–371) — add a `Delete` binding guarded
  on timeline focus that enqueues the new action.
- Verify: `cargo test -p animatix-gui`. Manual: shift-select keyframes, press Delete →
  all removed; Undo restores.

### B6. Clear selection on relevant edits
- File: `app/handlers/keyframe.rs` (`handle_delete_keyframe`, `handle_move_keyframe`).
- After mutating keyframes, drop now-stale entries from
  `ui_store.selection.selected_keyframes` (retain only triples whose
  `(actor, property, time_ms)` still exist). Also clear on `SelectScene`.
- Verify: `cargo test -p animatix-gui`.

## Files to touch (B)
- `app/stores/ui_store.rs` — field + snapshot/restore (B1, B2).
- `app/document/history.rs` — dead_code attr (B2).
- `app/commands/mod.rs` (+submodule) — new command (B4).
- `app/shell/mod.rs` — dispatch (B4).
- `app/handlers/ui.rs` / `keyframe.rs` — handler + stale pruning (B4, B6).
- `app/panels/timeline_panel.rs` — emit command, deletion reads selection (B3, B5).
- `app/runtime.rs` — Delete keybinding (B5).

## Risks (B)
- Two sources of truth during migration (temp `multi_selected` vs store). Keep temp as
  the interaction buffer, store as the published value; emit on change only.
- `(actor,time_ms)` → triple expansion changes nothing about current delete semantics
  (already per-property at that time) but verify no double-deletes.
- Undo/restore of `selected_keyframes` only matters if `UiSnapshot` is actually applied
  on undo; confirm whether history currently restores UI snapshots before removing the
  `dead_code` attr (it is currently `#[allow(dead_code)]`).
- Keyboard `Delete` must not fire while editing text fields / inspector inputs — guard
  on focus like existing shortcuts in `runtime.rs`.

---

# (C) UX polish (independent fixer tasks)

## C1. Tap-selection highlight for callout place handles
### Goal
Highlight the tapped callout place handle (`context.rs:1110` TODO where
`active_place` is hardcoded `None`).
- Files: `app/preview/context.rs` (~1100–1125, the `draw_callout_place_handles` call),
  `app/stores/ui_store.rs` or `app/preview/mod.rs` selection state.
- Add a transient `tapped_place: Option<CalloutPlace>` (and the actor it belongs to) to
  the preview/selection state; set it on click hit-test of place handles in the gesture
  layer (`app/preview/gestures/`), clear on next click elsewhere / drag start.
- Replace the hardcoded `let active_place = None;` with the resolved value when the
  drawn callout's actor matches the tapped actor.
- Verify: `cargo check -p animatix-gui`. Manual: tap a place handle → it highlights;
  tap empty canvas → clears.
- Risk: ensure highlight state does not persist across scene switch / deselect; clear in
  `DeselectActors` and `SelectScene`.

## C2. Snap visual feedback — remove dead `SnapResult` fields (already-rendered case)
### Goal
The snap lines/HUD are already rendered from `ctx.preview.snap.*`. The dead code is the
unused boolean/text fields on `SnapResult` (`drag_utils.rs:67`).
- File: `app/preview/drag_utils.rs` (`SnapResult` struct ~67–87, construction ~257,
  caller `gestures/move_actor.rs:164`).
- Two viable options:
  - **Option A (recommended, minimal):** Delete the unused fields from `SnapResult`
    (keep only `nx`, `ny`), since the snap visuals are driven by the side-effect writes
    to `ctx.preview.snap`. Removes six `#[allow(dead_code)]` annotations.
  - **Option B:** If a forward use is intended (e.g. scale gesture wants to read which
    axis snapped), actually consume them in `gestures/scale.rs` / `move_actor.rs` and
    drop the `dead_code` attrs. Larger; only if there is a concrete consumer.
- Pick A unless a consumer is identified. Per AGENTS.md, prefer removing dead code over
  annotating it.
- Verify: `cargo check --workspace` (catches `move_actor.rs` field references);
  `cargo test -p animatix-gui`. Manual: drag an actor near a guide → snap line + HUD
  still render.

## C3. Layers sidebar search/filter
### Goal
Add a search box to the Layers tab mirroring the Explorer tab's filter.
- File: `app/panels/sidebar.rs` (`layers_content_ui` ~576; model after
  `explorer_content_ui` ~234 and `EXPLORER_FILTER_ID` ~32).
- Add a `LAYERS_FILTER_ID` temp-data key + a `TextEdit::singleline` with a hint
  ("Filter layers…"). Filter `render_actor_tree` roots/children case-insensitively;
  when filtering, force-expand matched branches (mirror Explorer's `show` mask logic at
  ~263–274) so descendant matches are visible.
- Clear the filter when switching away from Layers (mirror the Explorer clear at
  `sidebar.rs:123`).
- Verify: `cargo check -p animatix-gui`. Manual: type in the box → tree narrows to
  matches + ancestors.
- Risk: keep selection state intact when filtered (filtering must not clear
  `selected_actors`); don't break drag-reparent while filtered (consider disabling
  reparent drops on filtered partial trees, or document the limitation).

## C4. Canvas empty / rebuild state
### Goal
Show an explicit "No scene to preview" empty state and a subtle "Rebuilding…"
affordance during async rebuild.
- Files: `app/preview/context.rs` (`render_preview_content` ~586–630, currently the
  `None` arm prints "Preview initializing…"), `app/panels/behavior.rs` (~92 builds
  `PreviewContext`), `app/panels/preview_panel.rs` (overlay draw region ~575–625).
- Empty state: distinguish "no scene" (no active timeline / zero actors / welcome) from
  "texture not ready yet". Pass the relevant signal into `PreviewContext` (e.g. a
  `has_scene: bool` derived from
  `document_store.source.document.active_timeline().is_some()` and non-empty scene), and
  in the `None`/empty branch render "No scene to preview" with a hint instead of
  "Preview initializing…".
- Rebuilding affordance: add `rebuild_in_progress: bool` to `PreviewContext` sourced
  from `preview_store.rebuild_in_progress` (set in `handlers/file.rs:317/340`,
  also reflected by `document_store.snapshot_is_stale()`). When true, draw a subtle
  corner pill/spinner ("Rebuilding…") over the canvas (in `render_preview_overlays` or
  after content) using `text::MUTED` — non-blocking, does not hide the stale frame.
- Verify: `cargo check --workspace`. Manual: open welcome / empty doc → "No scene to
  preview"; trigger a rebuild (edit source) → transient "Rebuilding…" pill.
- Risk: don't flash "Rebuilding…" on every keystroke — gate on
  `rebuild_in_progress` (the async in-flight flag), not on `snapshot_is_stale` alone,
  which can be true between debounced edits. The toolbar already reads
  `rebuild_in_progress` (`toolbar.rs:110`) — reuse that exact signal for consistency.

## Files to touch (C)
- `app/preview/context.rs` — C1 (active_place), C4 (empty/rebuild render).
- `app/preview/gestures/*` — C1 (set tapped place on hit).
- `app/preview/drag_utils.rs` — C2 (remove dead fields).
- `app/preview/gestures/move_actor.rs` — C2 (verify caller still compiles).
- `app/panels/sidebar.rs` — C3 (layers filter).
- `app/panels/behavior.rs` — C4 (pass `has_scene` / `rebuild_in_progress`).
- `app/panels/preview_panel.rs` — C4 (overlay pill), C2 (snap render unchanged).
- `app/stores/ui_store.rs` — C1 (tapped place state, if stored there).

## Cross-cutting verification
Per AGENTS.md, before finishing any group run:
```
cargo check --workspace
cargo test -p animatix --lib
cargo test -p animatix-gui
cargo test --no-fail-fast
```
No tree-sitter / PEG changes here (no `.amx` syntax changes), so the parser-sync script
is not required for these tasks.

## Suggested ordering
1. A1+A2 (atomic), then A3, then A4 — low risk, high user value (data-loss guard).
2. C2 (pure cleanup, unblocks confidence in snap code).
3. C3, C1, C4 (independent UX, any order).
4. B1→B2→B4→B3→B5→B6 (largest; touches command pipeline + two eval-independent paths).
