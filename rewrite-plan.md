# Animatix GUI Rewrite Plan

## Recommendation

Use a **targeted core rewrite delivered incrementally by releases**, not a full GUI rewrite and not a slow 7-phase strangler.

The rewrite scope is the GUI core data/model pipeline only:

- Rewrite/new: active timeline resolution, source ownership, derived snapshots, rebuild worker, history entries, command bus seams, export target resolution.
- Adapt in place: existing panels, `GuiShell`, handlers, preview code, source edit code.
- Do not rewrite: egui visual layout, inspector/timeline widgets wholesale, `PreviewSurface`, `EditorBuffer`, `source_edit` AST mutation modules.

This is justified by the explorer findings: `crates/animatix-gui/src/` has 31,419 lines / 89 files, 191 uses of `document_store.source.document.*`, 26 composition-blind `document.timeline` accesses, a 925-line `GuiShell`, 15+ mutable borrows through `WorkspaceBehavior`, 0 external consumers, and ~180 tests. The correctness bugs come from the core data model and mutation/rebuild pipeline; panels are mostly structurally usable once they consume cleaner APIs.

## Big Rewrite Argument

### For a targeted core rewrite

- The root cause is core state ownership: `DocumentSession`, `SourceStore`, `DocumentStore`, rebuild scheduling, undo/redo, and panel mutation boundaries.
- Composition correctness cannot be reliably fixed with local patches while panels can still read `document.timeline` directly.
- The crate is a leaf with 0 external consumers, so API stability inside `animatix-gui` is not a constraint.
- A surgical rewrite of core abstractions plus panel adaptation is faster than preserving the current 4-level access path for months.
- Each release below ships user-visible value and keeps the app buildable.

### Against a full rewrite

- ~180 existing tests would churn heavily if panels/runtime were rewritten wholesale.
- A full rewrite would not ship until complete and would likely regress editor, inspector, preview, export, hot reload, and persistence details.
- The current UI has working features; replacing them would spend time rediscovering behavior instead of fixing correctness.

## Release Order

1. R1: Composition-Aware Core Access
2. R2: Composition Export and Scene Targets
3. R3: Source Ownership and Epochs
4. R4: Immutable Document Snapshots
5. R5: Background Rebuild Worker
6. R6: Undo/Redo UI Snapshots
7. R7: Command Bus and Panel View Models
8. R8: Store Boundary Cleanup and Runtime Services

Each release should compile, pass tests, and be independently mergeable.

---

## R1: Composition-Aware Core Access

### Goal

Make preview, inspector, timeline, drag handlers, actor commands, and label utilities edit the active scene in compositions instead of silently falling back to single-scene `document.timeline`.

User value: multi-scene composition documents become trustworthy for selection, locking, dragging, inspector edits, keyframe editing, layers, timeline display, and actor insertion.

### Files to create

- `crates/animatix-gui/src/app/document/mod.rs` — new module root for GUI document-core compatibility APIs.
- `crates/animatix-gui/src/app/document/active_timeline.rs` — `ActiveSceneId`, `ActiveTimelineRef`, `ActiveTimelineMut`, scene resolution helpers.

### Files to modify

- `crates/animatix-gui/src/app/mod.rs` — add `pub(crate) mod document;`; keep `GuiShell` in place.
- `crates/animatix-gui/src/document.rs` — replace existing `DocumentSession::active_timeline() -> Option<&Timeline>` with richer API, add `active_timeline_ref()`, `active_timeline_mut()`, `resolve_active_scene_id()`, and keep a temporary `active_timeline()` compatibility wrapper if needed.
- `crates/animatix-gui/src/app/panels/behavior.rs` — build one active timeline value and pass it to sidebar/preview/inspector/timeline contexts instead of direct `document.timeline.as_ref()`.
- `crates/animatix-gui/src/app/panels/sidebar.rs` — use active timeline for Layers and asset-cache reads; clear selection on scene changes if labels are not present in the new active scene.
- `crates/animatix-gui/src/app/panels/preview_panel.rs` — replace `Option<&Timeline>` context field with `Option<ActiveTimelineRef<'_>>` or a compatibility `timeline: active.map(|t| t.timeline)`.
- `crates/animatix-gui/src/app/preview/context.rs` — use active timeline for locked selection filtering, motion paths, vertex handles, ghost overlays, and context-menu selection.
- `crates/animatix-gui/src/app/preview/drag_handler.rs` — use active timeline for lock checks, vertex edits, motion-path keyframe hit-testing, and layout reorder.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs` — remove custom composition fallback logic; consume active timeline API.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs` — consume active timeline API and remove duplicated scene fallback.
- `crates/animatix-gui/src/app/actions/mod.rs` — use `active_timeline_mut()` for in-memory drag/property preview and child-order mutation.
- `crates/animatix-gui/src/app/handlers/actor.rs` — use active timeline for rename collision, visibility/lock toggles, alignment/distribution, and copy/duplicate helpers.
- `crates/animatix-gui/src/app/document_controller.rs` — use active timeline for selected-container detection, duplicate drag start, `has_actor_label()`, and `unique_label()`.
- `crates/animatix-gui/src/app/utils/labels.rs` — add scene-aware `unique_label_for_timeline()`.
- `crates/animatix-gui/src/app/shell/insertion_palette.rs` — use active timeline for "can insert into selected container" and label checks.
- `crates/animatix-gui/src/app/runtime.rs` — use active timeline in keyboard nudge path.
- `crates/animatix-gui/src/app/stores/source_store.rs` — update `rebuild_cache()` callers to pass active timeline consistently.

### Files to delete

- None.

### New types/traits introduced

- `ActiveSceneId`
  - `SingleScene`
  - `Scene(String)`
- `ActiveTimelineRef<'a>`
  - `id: ActiveSceneId`
  - `timeline: &'a Timeline`
  - `composition: Option<&'a Composition>`
  - `scene_name: Option<&'a str>`
  - `duration_s: f64`
  - `dimensions: SceneDimensions`
- `ActiveTimelineMut<'a>`
  - `id: ActiveSceneId`
  - `timeline: &'a mut Timeline`
  - `scene_name: Option<String>`

### Key APIs added/removed

Added:

- `DocumentSession::active_timeline_ref(&self) -> Option<ActiveTimelineRef<'_>>`
- `DocumentSession::active_timeline_mut(&mut self) -> Option<ActiveTimelineMut<'_>>`
- `DocumentSession::resolve_active_scene_id(&self) -> Option<ActiveSceneId>`
- `DocumentSession::resolve_active_scene_name(&self, composition: &Composition) -> Option<&str>`

Temporarily retained:

- `DocumentSession::active_timeline(&self) -> Option<&Timeline>` as a compatibility wrapper.

Removed by the end of R1:

- Direct panel/handler reads of `document.timeline.as_ref()` except inside tests, export target code, and the active timeline API itself.

### Test strategy

Update tests:

- `crates/animatix-gui/src/document.rs` tests for composition active scene resolution.
- `crates/animatix-gui/src/app/command_handlers.rs` actor/selection tests to include a composition fixture.
- Any tests that expected `document.timeline` in composition tests should call `active_timeline_ref()`.

New tests:

- Single-scene document resolves `ActiveSceneId::SingleScene`.
- Composition active scene resolves selected scene.
- Invalid active scene falls back to declaration order / entry scene.
- Locked actor in active composition scene cannot be selected/dragged.
- Visibility/lock toggles mutate active composition scene timeline.
- Label generation avoids collisions in the active scene.

Verify:

- `cargo test -p animatix-gui active_timeline`
- `cargo test -p animatix-gui command_handlers`
- `cargo test -p animatix-gui`

### Migration path

1. Add new active timeline API while keeping the old wrapper.
2. Convert `behavior.rs` first so panels receive the same active timeline.
3. Convert preview/drag/inspector/timeline callers.
4. Convert handlers/controllers.
5. Grep must show no production direct timeline reads except explicit build/export/runtime render code:
   - `rg "document_store\.source\.document\.timeline|document\.timeline" crates/animatix-gui/src`

### Risk assessment

- Borrow conflicts are likely when introducing `ActiveTimelineMut`; solve by resolving scene names before taking mutable composition borrows.
- Some source edits still operate on whole raw AST and may not be scene-scoped; R1 should make visual/runtime behavior scene-aware but may need R2/R7 for fully explicit scene-targeted source commands.
- In-memory mutation of composition scene timelines before rebuild can diverge from source if source edit fails; preserve existing status/error behavior and schedule rebuilds consistently.

---

## R2: Composition Export and Scene Targets

### Goal

Make GUI export support both single-scene timelines and whole compositions, with correct duration, dimensions, and progress.

User value: users can export multi-scene compositions from the GUI instead of seeing "No timeline to export".

### Files to create

- `crates/animatix-gui/src/app/document/export_target.rs` — export target resolution and owned export payload conversion.

### Files to modify

- `crates/animatix-gui/src/app/document/mod.rs` — export `export_target`.
- `crates/animatix-gui/src/app/shell/export_dialog.rs` — add export scope UI/state, use `ExportTargetRef`, call composition renderer APIs, compute progress frames from resolved target duration.
- `crates/animatix-gui/src/app/stores/export_store.rs` — add `ExportScope` to state if export state lives there.
- `crates/animatix-gui/src/app/handlers/ui.rs` — default export filename/scope uses document export target.
- `crates/animatix-gui/src/document.rs` — add `export_target()` compatibility method delegating to `app/document/export_target.rs` helpers if needed.
- `crates/animatix-gui/src/app/tests.rs` or `crates/animatix-gui/src/app/command_handlers.rs` — add export target tests.

### Files to delete

- None.

### New types/traits introduced

- `ExportScope`
  - `ActiveScene`
  - `WholeComposition`
  - `Scene(String)`
- `ExportTargetRef<'a>`
  - `Timeline { scene, timeline, duration_s, dimensions }`
  - `Composition { composition, duration_s, dimensions }`
- `ExportTargetOwned`
  - `Timeline(Timeline)`
  - `Composition(Composition)`

### Key APIs added/removed

Added:

- `DocumentSession::export_target(&self, scope: ExportScope) -> Option<ExportTargetRef<'_>>`
- `ExportTargetRef::duration_s(&self) -> f64`
- `ExportTargetRef::dimensions(&self) -> SceneDimensions`
- `ExportTargetRef::to_owned_target(&self) -> ExportTargetOwned`

Changed:

- `ExportDialogState` gains `scope: ExportScope`.
- `start_export()` matches `ExportTargetOwned`:
  - Timeline image/video/gif calls existing `render_*_timeline_with_progress`.
  - Composition image calls `animatix::renderer::render_image_composition`.
  - Composition video calls `animatix::renderer::render_video_composition_with_progress`.
  - Composition gif calls `animatix::renderer::render_gif_composition_with_progress`.
  - WebM/MOV composition use `render_video_composition_with_progress` with appropriate `ExportSettings`.

Removed:

- `start_export()` early failure on missing `document.timeline`.

### Test strategy

Update tests:

- Existing export dialog tests, if any, must expect composition documents to resolve a target.

New tests:

- Single-scene default scope resolves `Timeline`.
- Composition default scope resolves `Composition`.
- Composition `ActiveScene` resolves active scene `Timeline`.
- Auto duration uses `composition.global_duration_s` for whole composition.
- Export total frame count uses target duration.

Manual verify:

- Open a two-scene `.amx`; export image at transition time.
- Export video/GIF for whole composition.
- Export active scene only.

Commands:

- `cargo test -p animatix-gui export`
- `cargo test -p animatix`
- `cargo test -p animatix-gui`

### Migration path

1. Add target resolver with no UI changes.
2. Change duration preview to use resolver.
3. Change `start_export()` to clone resolved target.
4. Add scope UI only after backend works.
5. Default to `WholeComposition` for compositions to match user expectation.

### Risk assessment

- `render_image_composition` lacks progress/debug arguments unlike timeline image progress; image exports are one frame, so set progress manually before/after.
- Composition dimensions may differ per scene; default to document config/current `scene_dimensions` in R2 and defer per-scene dimension controls.
- Exporting while source has pending unrebuild edits still exports last accepted in-memory document; show status "Exporting last rebuilt version" until snapshots land in R4/R5.

---

## R3: Source Ownership and Epochs

### Goal

Make source text, file path, dirty state, and edit versioning explicit in `SourceStore`, while keeping `DocumentSession` as a derived-state compatibility facade.

User value: save/reload/undo/editor changes become safer; stale diagnostics and stale preview can be identified instead of silently clearing or mutating old state.

### Files to create

- `crates/animatix-gui/src/app/document/version.rs` — `SourceEpoch`, `SourceHash`, `DocumentGeneration`, `Versioned<T>`.
- `crates/animatix-gui/src/app/document/source_change.rs` — `SourceChange`, `TextDiffSummary` placeholder.

### Files to modify

- `crates/animatix-gui/src/app/document/mod.rs` — export version/source change types.
- `crates/animatix-gui/src/app/stores/source_store.rs` — add canonical `file_path`, `text: String` or `Arc<str>`, `saved_text_hash`, `edit_epoch`, `dirty`; add source mutation methods.
- `crates/animatix-gui/src/document.rs` — add temporary mirror helpers and stop direct mutation where possible; add `set_source_text_from_store()` or `rebuild_from_source_snapshot()`.
- `crates/animatix-gui/src/app/stores/document_store.rs` — expose `source_text()`, `file_path()`, `is_dirty()`, `replace_source_text()` facade to reduce `document_store.source.document.*`.
- `crates/animatix-gui/src/app/handlers/playback.rs` — `handle_editor_changed()` calls `SourceStore::replace_text_from_editor()` and marks diagnostics stale instead of clearing them.
- `crates/animatix-gui/src/app/handlers/file.rs` — save writes `SourceStore::text()`, reload/open reset `SourceStore` and editor together.
- `crates/animatix-gui/src/app/handlers/ui.rs` — undo/redo replace `SourceStore` text.
- `crates/animatix-gui/src/app/actions/mod.rs` — source commits route through `SourceStore::commit_source()`.
- `crates/animatix-gui/src/app/document_controller.rs` — `apply_source()` routes through `SourceStore::commit_source()`.
- `crates/animatix-gui/src/app/stores/preview_store.rs` — add stale preview state with reason.
- `crates/animatix-gui/src/app/components/diagnostics.rs` — render stale badge/dim stale diagnostics.
- `crates/animatix-gui/src/app/mod.rs` — status strings use source facade; cursor sync uses document derived index only.
- `crates/animatix-gui/src/app/runtime.rs` — file persistence uses source facade.

### Files to delete

- None.

### New types/traits introduced

- `SourceEpoch(pub u64)`
- `SourceHash(pub u64)`
- `DocumentGeneration(pub u64)`
- `Versioned<T>`
- `SourceChange`
- `DiagnosticFreshness`
  - `Current(DocumentGeneration)`
  - `Stale { from_generation: Option<DocumentGeneration>, current_source_epoch: SourceEpoch }`
  - `Failed(DocumentGeneration)`
- `StalePreviewState`
- `StaleReason`
  - `SourceEdited`
  - `RebuildRunning`
  - `RebuildFailed`
  - `RenderPending`

### Key APIs added/removed

Added:

- `SourceStore::text(&self) -> &str`
- `SourceStore::file_path(&self) -> &Path`
- `SourceStore::epoch(&self) -> SourceEpoch`
- `SourceStore::hash(&self) -> SourceHash`
- `SourceStore::is_dirty(&self) -> bool`
- `SourceStore::replace_text(&mut self, text: String) -> SourceChange`
- `SourceStore::commit_source(&mut self, new_source: String, source_index: SourceIndex) -> SourceChange`
- `SourceStore::mark_saved(&mut self)`
- `SourceStore::load_document(document: DocumentSession, editor: EditorBuffer)`

Changed:

- `DocumentStore::combined_diagnostics()` includes freshness metadata or returns a wrapper model.
- `handle_save()` no longer writes `editor.text()` directly.

Removed by end of R3:

- Optimistic `document.diagnostics.clear()` in `handle_editor_changed()`.

### Test strategy

Update tests:

- Command handler tests that assert `document.source_text` should assert `document_store.source.text()` or compatibility mirror.
- Save tests should verify source store text is written.

New tests:

- `SourceStore::replace_text()` increments epoch and dirty flag.
- `SourceStore::mark_saved()` clears dirty only when saved hash matches.
- Editor changed leaves diagnostics stale, not empty.
- Undo/redo source replacement increments epoch.
- Reload resets epoch/dirty and synchronizes editor.

Commands:

- `cargo test -p animatix-gui source_store`
- `cargo test -p animatix-gui handlers`
- `cargo test -p animatix-gui`

### Migration path

1. Add `SourceStore` canonical fields while still mirroring into `DocumentSession.source_text`.
2. Convert save/open/reload/editor change paths first.
3. Convert controller/source-edit commit paths.
4. Leave derived fields in `DocumentSession` until R4.
5. Use facade methods to collapse the 191 deep access sites gradually.

### Risk assessment

- Temporary dual source storage can drift. Every source mutation must go through `SourceStore`; make direct `DocumentSession.source_text` writes private only in R4 if feasible.
- Tests may fail due to changed dirty/epoch behavior; update assertions around source access, not document semantics.
- Stale diagnostics require UI wording decisions; keep a simple "stale" badge in R3.

---

## R4: Immutable Document Snapshots

### Goal

Move parsed AST, expanded AST, timelines, composition, diagnostics, indexes, duration, dimensions, and derived caches into immutable generation-tagged `DocumentSnapshot`s.

User value: preview/inspector/timeline stop racing mutable rebuild state; last-known-good preview can remain visible when the current source is invalid.

### Files to create

- `crates/animatix-gui/src/app/document/snapshot.rs` — `DocumentSnapshot`, `BuildTargetSnapshot`, snapshot active timeline API.
- `crates/animatix-gui/src/app/document/caches.rs` — `DerivedCaches`, `VersionedSceneMap`, `SceneKey`.
- `crates/animatix-gui/src/app/document/rebuild_output.rs` — `RebuildOutput`, `RebuildFailure`, conversion from current rebuild pipeline.

### Files to modify

- `crates/animatix-gui/src/app/document/mod.rs` — export snapshot/caches/rebuild output.
- `crates/animatix-gui/src/app/stores/document_store.rs` — own `current: Option<Arc<DocumentSnapshot>>`, `last_good: Option<Arc<DocumentSnapshot>>`, generation counter, and compatibility facade.
- `crates/animatix-gui/src/document.rs` — split `rebuild()` internals into `build_snapshot_from_source()` and `apply_snapshot_compat()`; keep old fields populated during migration.
- `crates/animatix-gui/src/app/stores/source_store.rs` — move hot-path caches toward `DerivedCaches` or wrap current caches in `Versioned`.
- `crates/animatix-gui/src/app/stores/preview_store.rs` — track `rendered_generation` and `requested_generation`.
- `crates/animatix-gui/src/app/handlers/file.rs` — rebuild accepts/publishes snapshot through `DocumentStore`.
- `crates/animatix-gui/src/app/runtime.rs` — render from `document_store.last_good_snapshot()` / current snapshot instead of raw mutable fields.
- `crates/animatix-gui/src/app/panels/behavior.rs` — build panel inputs from `DocumentSnapshot`.
- `crates/animatix-gui/src/app/components/diagnostics.rs` — diagnostics read snapshot freshness.
- `crates/animatix-gui/src/app/shell/export_dialog.rs` — export from latest snapshot target.
- `crates/animatix-gui/src/app/audio.rs` — accept audio segments from snapshot target or document store facade.

### Files to delete

- None in R4. Do not delete `DocumentSession` yet.

### New types/traits introduced

- `DocumentSnapshot`
- `SnapshotStatus`
  - `Clean`
  - `Stale { current_source_epoch: SourceEpoch }`
  - `Failed { error: String }`
- `BuildTargetSnapshot`
  - `Empty`
  - `Timeline(Arc<Timeline>)`
  - `Composition(Arc<Composition>)`
- `DerivedCaches`
- `VersionedSceneMap<T>`
- `SceneKey`
  - `SingleScene`
  - `Scene(String)`
- `RebuildOutput`
- `RebuildFailure`

### Key APIs added/removed

Added:

- `DocumentStore::current_snapshot(&self) -> Option<Arc<DocumentSnapshot>>`
- `DocumentStore::last_good_snapshot(&self) -> Option<Arc<DocumentSnapshot>>`
- `DocumentStore::publish_snapshot(&mut self, snapshot: DocumentSnapshot)`
- `DocumentStore::mark_source_stale(&mut self, epoch: SourceEpoch)`
- `DocumentSnapshot::active_timeline(&self, active_scene: Option<&str>) -> Option<ActiveTimelineRef<'_>>`
- `DocumentSnapshot::export_target(&self, scope: ExportScope, active_scene: Option<&str>) -> Option<ExportTargetRef<'_>>`
- `DocumentSnapshot::all_audio_segments(&self, doc_dir: &Path) -> Vec<AudioSegment>`

Changed:

- `DocumentSession::rebuild()` becomes a compatibility wrapper around snapshot build/apply.
- Existing panel contexts should prefer snapshot-derived refs.

Removed by end of R4:

- Derived cache writes from render/rebuild without generation assignment.

### Test strategy

Update tests:

- `document.rs` rebuild tests should assert both compatibility fields and snapshot fields.
- Runtime tests should use `last_good_snapshot()`.

New tests:

- Successful rebuild publishes `current` and `last_good`.
- Failed rebuild publishes failed `current` but preserves previous `last_good`.
- Snapshot generation increments on success and failure.
- Snapshot active timeline works for single scene and composition.
- Render hit regions are tagged with snapshot generation.

Commands:

- `cargo test -p animatix-gui snapshot`
- `cargo test -p animatix-gui document`
- `cargo test -p animatix-gui`

### Migration path

1. Produce snapshots while still applying them into `DocumentSession` fields.
2. Convert read-only consumers to `DocumentSnapshot`.
3. Keep mutation/source edit paths using raw AST compatibility until command bus work.
4. Once consumers are snapshot-based, R5 can rebuild off-thread safely.

### Risk assessment

- `Timeline`/`Composition` clone/Arc boundaries may reveal non-`Send` or non-`Sync` fields. R4 can stay UI-threaded and use `Arc` immutability before R5 checks thread movement.
- Compatibility duplication is temporary and must be removed later or it will become the new stale-state source.
- Snapshot caches may increase memory; keep only `current` and `last_good`.

---

## R5: Background Rebuild Worker

### Goal

Move parse/module/typecheck/build work off the egui frame with request tokens, source epochs, cancellation, and snapshot acceptance.

User value: typing, playback, and UI interaction stay responsive on large documents; invalid source shows stale last-good preview instead of freezing or blanking.

### Files to create

- `crates/animatix-gui/src/app/document/rebuild.rs` — worker, request/response, token, cancellation, timings.
- `crates/animatix-gui/src/app/document/scheduler.rs` — debounce and immediate rebuild scheduling helpers if not kept in `DocumentStore`.

### Files to modify

- `crates/animatix-gui/Cargo.toml` — add `crossbeam-channel = "0.5"` or use `std::sync::mpsc`; recommendation: add `crossbeam-channel` for non-blocking polling and clean worker communication.
- `crates/animatix-gui/src/app/document/mod.rs` — export rebuild modules.
- `crates/animatix-gui/src/app/stores/document_store.rs` — own `RebuildWorker`, `RebuildState`, `submit_rebuild()`, `poll_rebuilds()`, `accept_rebuild()`.
- `crates/animatix-gui/src/app/stores/preview_store.rs` — replace `rebuild_in_progress` with generation-aware stale/rebuild status or keep compatibility flag set from `RebuildState`.
- `crates/animatix-gui/src/app/handlers/file.rs` — `handle_rebuild()` submits job instead of building synchronously; add `handle_rebuild_now()` only for tests if needed.
- `crates/animatix-gui/src/app/handlers/playback.rs` — editor changed marks stale and schedules worker request after debounce without clearing diagnostics.
- `crates/animatix-gui/src/app/mod.rs` — `GuiShell::prepare_frame()` polls rebuild responses before playback tick and schedules expired rebuilds through `DocumentStore`.
- `crates/animatix-gui/src/app/runtime.rs` — request repaint while rebuild is running; render last-good snapshot.
- `crates/animatix-gui/src/app/components/diagnostics.rs` — show stale/running/failed freshness states.
- `crates/animatix-gui/src/app/shell/toolbar.rs` — show rebuilding/stale status indicator if toolbar owns status UI.
- `crates/animatix-gui/src/document.rs` — expose pure build function usable from worker without mutating UI state.

### Files to delete

- None.

### New types/traits introduced

- `RebuildWorker`
- `RebuildToken(pub u64)`
- `RebuildRequest`
- `RebuildResponse`
- `CancellationToken`
- `RebuildState`
  - `Idle`
  - `Debounced { due_at, source_epoch }`
  - `Running { token, source_epoch }`
- `RebuildTimings`
- `BuildQuality` reuse from `animatix::timeline::BuildQuality` or wrapper if needed.
- `RebuildCacheSeed` if plot/modifier/module cache reuse survives worker extraction.

### Key APIs added/removed

Added:

- `DocumentStore::schedule_rebuild(&mut self, source: &SourceStore, debounce: Duration)`
- `DocumentStore::submit_rebuild(&mut self, source: &SourceStore)`
- `DocumentStore::poll_rebuilds(&mut self, source: &SourceStore) -> Vec<Effect>`
- `DocumentStore::has_rebuild_work(&self) -> bool`
- `CancellationToken::is_cancelled(&self) -> bool`

Changed:

- `GuiShell::prepare_frame()` ordering:
  1. poll hot reload/export
  2. poll rebuild responses
  3. tick playback
  4. sync active scene
  5. submit expired rebuilds
- `handle_rebuild()` returns after scheduling/submitting, not after building.

Removed:

- Synchronous UI-frame call to `DocumentSession::rebuild()` from pending rebuild path.

### Test strategy

Update tests:

- Existing rebuild handler tests must poll worker or use a test synchronous worker adapter.
- Status assertions should expect "rebuild scheduled/running" before completion.

New tests:

- Stale response with older source epoch is rejected.
- Newer rebuild token cancels older token.
- Failed rebuild preserves `last_good`.
- Diagnostics remain stale during rebuild and become current after acceptance.
- Rapid editor changes accept only newest source.

Commands:

- `cargo test -p animatix-gui rebuild`
- `cargo test -p animatix-gui document_store`
- `cargo test -p animatix-gui`
- Manual: type quickly in a large/import-heavy `.amx`; preview should remain responsive.

### Migration path

1. Keep synchronous build helper for tests.
2. Introduce worker but gate only scheduled/editor rebuilds through it.
3. Convert manual rebuild and reload to worker.
4. Keep initial load synchronous in R5 to avoid startup complexity; optional later improvement.
5. Remove synchronous pending rebuild from `prepare_frame()` after worker tests pass.

### Risk assessment

- `ModuleGraph` cache may not be safely reusable across worker requests; start with no cross-thread cache reuse if necessary, then seed safe data later.
- Imported file invalidation must still work; source hash alone is insufficient for external import edits.
- Worker output must own data; no borrowed AST/source references can cross threads.
- If `Timeline` or `Composition` is not `Send`, worker must build an owned output on the worker and publish on UI only if types permit; otherwise split parse/typecheck off-thread and final build on UI as fallback.

---

## R6: Undo/Redo UI Snapshots

### Goal

Make undo/redo restore source text plus relevant UI state: active scene, selection, playhead, loop range, timeline zoom/scroll, preview zoom/pan, tool mode, sidebar tab, property/keyframe view modes.

User value: undoing visual edits returns users to the same scene, selection, time, and view context instead of only replacing text.

### Files to create

- `crates/animatix-gui/src/app/document/history.rs` — `UiSnapshot`, `HistoryPolicy`, `HistoryGroup`, source/UI undo result types if not kept in store.

### Files to modify

- `crates/animatix-gui/src/app/document/mod.rs` — export history types.
- `crates/animatix-gui/src/app/commands.rs` — expand `UndoEntry` or move it to `history.rs`; add command kind/label helpers.
- `crates/animatix-gui/src/app/stores/history_store.rs` — store `source_before`, `source_after`, `ui_before`, `ui_after`, epochs, coalescing state.
- `crates/animatix-gui/src/app/stores/ui_store.rs` — add `UiStore::snapshot()` and `UiStore::restore_snapshot()`.
- `crates/animatix-gui/src/app/handlers/ui.rs` — undo/redo use `HistoryStore::undo()` / `redo()`, clear active drags, schedule immediate rebuild.
- `crates/animatix-gui/src/app/actions/mod.rs` — coalesce drag source edits into one history entry; stop snapshotting every drag update.
- `crates/animatix-gui/src/app/document_controller.rs` — capture before/after UI snapshots around document-affecting commands through caller or controller helper.
- `crates/animatix-gui/src/app/handlers/actor.rs` — use new history recording helpers.
- `crates/animatix-gui/src/app/handlers/keyframe.rs` — use new history recording helpers.
- `crates/animatix-gui/src/app/handlers/property.rs` — use new history recording helpers.
- `crates/animatix-gui/src/app/handlers/scene.rs` — use new history recording helpers.
- `crates/animatix-gui/src/app/shell/settings.rs` — expose undo depth setting if settings UI is appropriate.
- `crates/animatix-gui/src/app/persistence.rs` — persist undo settings only, not undo stack.

### Files to delete

- None.

### New types/traits introduced

- `UiSnapshot`
- `HistoryPolicy`
  - `RecordImmediate`
  - `BeginCoalesced`
  - `UpdateCoalesced`
  - `CommitCoalesced`
  - `Skip`
- `HistoryGroup`
- `UndoResult`
- `HistorySettings`

### Key APIs added/removed

Added:

- `UiStore::snapshot(&self, preview: &PreviewStore, document: &DocumentStore) -> UiSnapshot`
- `UiStore::restore_snapshot(&mut self, snapshot: UiSnapshot, preview: &mut PreviewStore, document: &DocumentStore)`
- `HistoryStore::record_source_change(...)`
- `HistoryStore::begin_coalesced(...)`
- `HistoryStore::commit_coalesced(...)`
- `HistoryStore::undo(source: &mut SourceStore, ui: &mut UiStore, preview: &mut PreviewStore) -> Option<UndoResult>`
- `HistoryStore::redo(...)`

Changed:

- `DocumentStore::snapshot(Command)` becomes a compatibility wrapper or is removed after handlers migrate.

Removed by end of R6:

- `UndoEntry { command, source_before }` as the only undo representation.

### Test strategy

Update tests:

- Empty undo/redo tests remain.
- Existing actor/property/keyframe undo tests must assert UI restoration.

New tests:

- Undo restores active scene and selected actor.
- Undo restores playhead and loop region.
- Undo restores preview zoom/pan and timeline zoom/scroll.
- Redo restores `ui_after`.
- Drag property edit records exactly one undo entry.
- Undo clears active drag state.
- Undo schedules immediate rebuild and marks preview stale.

Commands:

- `cargo test -p animatix-gui history`
- `cargo test -p animatix-gui undo`
- `cargo test -p animatix-gui`

### Migration path

1. Add `UiSnapshot` and new `UndoEntry` with both before/after source/UI.
2. Change history store first while maintaining `DocumentStore::snapshot()` compatibility.
3. Convert property/drag paths to coalescing.
4. Convert actor/keyframe/scene handlers.
5. Remove old source-only undo once all callers record after state.

### Risk assessment

- Snapshotting too much ephemeral state can revive stale drags; explicitly exclude active drag internals and clear them on restore.
- High-frequency text edits can create huge snapshots; keep editor text snapshots but coalesce visual drag edits.
- UI restoration may refer to actors/scenes removed by rebuild; reconcile after rebuild acceptance.

---

## R7: Command Bus and Panel View Models

### Goal

Stop panels from receiving broad mutable store references. Panels read immutable view models and emit typed commands/events through a command bus.

User value: fewer panel-induced state bugs, more consistent composition behavior, and a foundation for dope sheet/storyboard/property spreadsheet work.

### Files to create

- `crates/animatix-gui/src/app/command_bus.rs` — `CommandBus`, `ShellAction` queue wrapper, emit/drain helpers.
- `crates/animatix-gui/src/app/effects.rs` — move `Effect` from `commands.rs` if desired.
- `crates/animatix-gui/src/app/panels/workspace.rs` — view-model assembly and tiling behavior replacement.
- `crates/animatix-gui/src/app/panels/preview_model.rs` — `PreviewPanelModel`.
- `crates/animatix-gui/src/app/panels/timeline_model.rs` — `TimelinePanelModel`.
- `crates/animatix-gui/src/app/panels/inspector/model.rs` — `InspectorModel`.

### Files to modify

- `crates/animatix-gui/src/app/mod.rs` — add `command_bus` module, `GuiShell::build_view_models()`, `GuiShell::dispatch_actions()`, use new workspace UI.
- `crates/animatix-gui/src/app/commands.rs` — split `ShellAction` into document/playback/selection/view/file/export groups or keep enum but move queue mechanics to `command_bus.rs`.
- `crates/animatix-gui/src/app/shell/mod.rs` — dispatcher drains `CommandBus`; command handling remains here until later.
- `crates/animatix-gui/src/app/panels/behavior.rs` — shrink to adapter or replace with `panels/workspace.rs`.
- `crates/animatix-gui/src/app/panels/preview_panel.rs` — accept `PreviewPanelModel<'_>` and `&mut CommandBus`; remove document/store mutation from context.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs` — accept `TimelinePanelModel<'_>` and emit actions.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs` — accept `InspectorModel<'_>` and emit actions.
- `crates/animatix-gui/src/app/panels/sidebar.rs` — split read model from command emission; editor tab may keep mutable `EditorBuffer` in R7 if necessary.
- `crates/animatix-gui/src/app/panels/editor.rs` — emit editor changed action; avoid direct source writes.
- `crates/animatix-gui/src/app/preview/drag_handler.rs` — emit document/property/selection commands, no direct document mutation.
- `crates/animatix-gui/src/app/preview/property_popup.rs` — emit property edits through bus.
- `crates/animatix-gui/src/app/preview/selection.rs` — selection changes become `SelectionCommand`.
- `crates/animatix-gui/src/app/stores/ui_store.rs` — selection store gains scene id if not already added.
- `crates/animatix-gui/src/app/runtime.rs` — keyboard shortcuts emit via bus/pending actions only.

### Files to delete

- Delete `crates/animatix-gui/src/app/panels/behavior.rs` only after `panels/workspace.rs` fully replaces it. If too risky, keep as compatibility adapter in R7 and delete in R8.

### New types/traits introduced

- `CommandBus`
- `DocumentCommandKind`
- `PlaybackCommand`
- `SelectionCommand`
- `ViewCommand`
- `FileCommand`
- `ExportCommand`
- `PreviewPanelModel<'a>`
- `TimelinePanelModel<'a>`
- `InspectorModel<'a>`
- `SelectionView<'a>`
- `SnapSettings`
- `SceneTarget`

### Key APIs added/removed

Added:

- `CommandBus::emit(&mut self, action: impl Into<ShellAction>)`
- `CommandBus::drain(&mut self) -> impl Iterator<Item = ShellAction>`
- `GuiShell::build_preview_model(&self, texture: Option<TextureId>) -> PreviewPanelModel<'_>`
- `GuiShell::build_timeline_model(&self) -> TimelinePanelModel<'_>`
- `GuiShell::build_inspector_model(&self) -> InspectorModel<'_>`
- `SceneTarget::resolve(snapshot, ui) -> Result<ActiveSceneId, CommandError>`

Changed:

- Panel functions take `(ui, model, bus)` instead of mutable store-heavy contexts.

Removed by end of R7 or R8:

- `WorkspaceBehavior` fields for `&mut DocumentStore`, `&mut PreviewStore`, 15+ loose mutable borrows.

### Test strategy

Update tests:

- Panel tests or command handler tests should construct models and assert emitted commands.
- Existing runtime/panel compile errors will reveal context fields to migrate.

New tests:

- Preview panel emits `SelectionCommand` / `PropertyEdit` without document mutation.
- Timeline panel emits `MoveKeyframe` with scene id.
- Inspector emits `PropertyEdit` with `SceneTarget::Active`.
- Workspace model uses snapshot active timeline for composition.

Commands:

- `cargo test -p animatix-gui panels`
- `cargo test -p animatix-gui command`
- `cargo test -p animatix-gui`

### Migration path

1. Add command bus as a wrapper around existing `ActionQueue`.
2. Build view models while existing contexts still exist.
3. Convert timeline panel first because it already mostly emits commands.
4. Convert preview panel and drag handlers.
5. Convert inspector.
6. Convert sidebar/editor last because editor needs mutable buffer handling.
7. Remove or shrink `WorkspaceBehavior`.

### Risk assessment

- This release touches many files; keep each panel adaptation small and compile after each panel.
- Editor buffer may still need controlled mutability; allow a special `EditorPanelModel` bridge instead of forcing pure immutability immediately.
- Borrow checker issues are likely in workspace model assembly; use snapshots/Arc clones and short borrow scopes.

---

## R8: Store Boundary Cleanup and Runtime Services

### Goal

Finish ownership boundaries: source store owns canonical source, document store owns immutable derived snapshots/rebuild worker, UI store owns UI state only, preview store owns playback/render stale state only, shell dispatches commands, runtime owns eframe/WGPU/audio adapters.

User value: lower regression risk for future GUI features, easier testing without WGPU, and fewer stale-state bugs.

### Files to create

- `crates/animatix-gui/src/app/services/mod.rs` — services module root.
- `crates/animatix-gui/src/app/services/renderer.rs` — `PreviewRenderer`, `RenderRequest`, `RenderResult`.
- `crates/animatix-gui/src/app/services/audio.rs` — `AudioPreviewEngine` trait and adapter around current `AudioEngine`.
- `crates/animatix-gui/src/app/services/screenshots.rs` — screenshot service wrapper if extracted from runtime.
- `crates/animatix-gui/src/app/document/session.rs` — optional compatibility facade if `DocumentSession` cannot be removed entirely yet.

### Files to modify

- `crates/animatix-gui/src/app/stores/source_store.rs` — remove derived caches and `DocumentSession` ownership; keep source/editor synchronization only.
- `crates/animatix-gui/src/app/stores/document_store.rs` — own snapshots, rebuild worker, diagnostics, derived caches.
- `crates/animatix-gui/src/app/stores/preview_store.rs` — own playback, viewport/render stale state, not rebuild scheduling derived data.
- `crates/animatix-gui/src/app/stores/ui_store.rs` — ensure no source text or derived caches remain.
- `crates/animatix-gui/src/app/mod.rs` — shrink `GuiShell`; delegate frame pipeline to smaller methods.
- `crates/animatix-gui/src/app/runtime.rs` — become thin eframe adapter; use `PreviewRenderer` and `AudioPreviewEngine`.
- `crates/animatix-gui/src/preview_surface.rs` — implement renderer adapter or be wrapped by `app/services/renderer.rs`.
- `crates/animatix-gui/src/app/audio.rs` — implement audio service adapter or move under services.
- `crates/animatix-gui/src/app/persistence.rs` — persist versioned UI/workspace settings, not document derived state.
- `crates/animatix-gui/src/app/shell/mod.rs` — dispatch typed command groups to controller/services.
- `crates/animatix-gui/src/app/document_controller.rs` — become document command executor over source store + snapshot, not broad store mutator.
- `crates/animatix-gui/src/document.rs` — move GUI-specific session responsibilities into `app/document/session.rs` or reduce to pure load/build helpers.

### Files to delete

- Delete `crates/animatix-gui/src/app/panels/behavior.rs` if not deleted in R7.
- Delete compatibility `DocumentSession` GUI mutation fields only when all callers use `SourceStore` + `DocumentSnapshot`.
- Do not delete `crates/animatix-gui/src/document.rs` unless all tests and imports are migrated; prefer reducing it first.

### New types/traits introduced

- `PreviewRenderer`
- `RenderRequest<'a>`
- `RenderResult`
- `AudioPreviewEngine`
- `AudioSourceRef<'a>`
- `GuiShellFrameInput`
- `GuiShellFrameOutput`
- `WorkspacePersistenceV2`
- `WindowPersistence`
- `PanelPersistence`
- `TimelinePersistence`
- `PreviewPersistence`

### Key APIs added/removed

Added:

- `GuiShell::frame(&mut self, input: GuiShellFrameInput) -> GuiShellFrameOutput`
- `PreviewRenderer::render(&mut self, request: RenderRequest<'_>) -> Result<RenderResult, RenderError>`
- `AudioPreviewEngine::sync(&mut self, source: AudioSourceRef<'_>, playback: &PlaybackController)`
- `SourceStore::save_to_disk()`
- `SourceStore::load_from_disk(path)`

Changed:

- `AnimatixApp` owns eframe/WGPU texture/screenshot/audio adapters and delegates business state to `GuiShell`.
- `DocumentController` applies semantic document commands and source edits; it should not own preview/UI stores directly except through command outcomes/effects.

Removed:

- Production use of `document_store.source.document.*` deep path.
- Source-derived caches in `SourceStore`.
- Direct panel mutation of document/timeline/composition.

### Test strategy

Update tests:

- Existing store tests to assert new ownership boundaries.
- Runtime-dependent tests use mock `PreviewRenderer`.

New tests:

- `GuiShell::frame()` can run without WGPU.
- Mock renderer receives render request for current snapshot generation.
- Audio service receives timeline/composition audio from snapshot.
- Persistence migrates old workspace layout to `WorkspacePersistenceV2`.
- No source/derived caches remain in `SourceStore`.

Commands:

- `cargo test -p animatix-gui`
- `cargo test -p animatix`
- Manual: open file, edit source, drag actor, undo/redo, export, hot reload, screenshot.

### Migration path

1. Extract service traits without changing behavior.
2. Wrap existing `PreviewSurface` and `AudioEngine`.
3. Move caches from `SourceStore` to `DocumentStore`.
4. Remove compatibility fields/accessors one cluster at a time.
5. Delete `WorkspaceBehavior` adapter and deep access paths.
6. Keep `app/mod.rs` as facade until final cleanup; do not rename it unless churn is low.

### Risk assessment

- This is the cleanup release with the most deletion risk. Keep compatibility shims until grep proves no callers.
- Runtime service extraction can accidentally change texture lifetime; isolate renderer wrapper and keep current registration/update logic intact.
- Persistence migration can corrupt layouts; read old format and write new format only after successful load.

---

## Cross-Release Acceptance Criteria

### Composition correctness

- No production panel/handler directly chooses `document.timeline` when the operation means "editable timeline".
- Active timeline resolution is centralized.
- Selection, lock, vertex edit, motion path, layout reorder, inspector property edit, keyframe move, actor creation, and label generation work in the active scene of a composition.
- GUI export supports whole composition and active-scene export.

### Rebuild responsiveness

- Editor changes mark document/diagnostics/preview stale immediately.
- Rebuild work runs off the UI frame.
- Older rebuild responses are rejected by source epoch/token.
- Last-good preview remains visible on rebuild failure.

### Snapshot safety

- Derived document state is immutable after publication.
- `DocumentGeneration` increments on accepted rebuild success or failure.
- Hit regions/actor bounds/keyframes carry generation and scene key.
- Mutating commands reject or disable stale generation-sensitive edits.

### Undo/redo

- Undo/redo restore source plus active scene, selection, playhead, loop region, timeline zoom/scroll, preview zoom/pan, tool mode, sidebar tab, and inspector modes.
- Drag edits produce one undo entry.
- Undo/redo schedule immediate rebuild and mark preview stale.

### Panel boundaries

- Panels receive immutable models and a command bus.
- Mutations flow through shell/controller.
- `WorkspaceBehavior` no longer passes a broad mutable context.

---

## Files to Touch Summary

### Core document/store

- `crates/animatix-gui/src/document.rs` — compatibility document build/session logic; active timeline and snapshot bridge.
- `crates/animatix-gui/src/app/document/mod.rs` — new GUI document-core module.
- `crates/animatix-gui/src/app/document/active_timeline.rs` — active scene/timeline resolution.
- `crates/animatix-gui/src/app/document/export_target.rs` — timeline/composition export resolution.
- `crates/animatix-gui/src/app/document/version.rs` — source/document generation types.
- `crates/animatix-gui/src/app/document/source_change.rs` — source change metadata.
- `crates/animatix-gui/src/app/document/snapshot.rs` — immutable document snapshots.
- `crates/animatix-gui/src/app/document/caches.rs` — generation-tagged derived caches.
- `crates/animatix-gui/src/app/document/rebuild_output.rs` — owned rebuild outputs/failures.
- `crates/animatix-gui/src/app/document/rebuild.rs` — background rebuild worker.
- `crates/animatix-gui/src/app/document/scheduler.rs` — rebuild scheduling helper.
- `crates/animatix-gui/src/app/document/history.rs` — UI snapshots/history policy.
- `crates/animatix-gui/src/app/document/session.rs` — optional compatibility facade.

### Stores

- `crates/animatix-gui/src/app/stores/source_store.rs` — canonical source ownership, then cache removal.
- `crates/animatix-gui/src/app/stores/document_store.rs` — snapshots, generations, rebuild worker, diagnostics.
- `crates/animatix-gui/src/app/stores/history_store.rs` — source/UI undo entries.
- `crates/animatix-gui/src/app/stores/preview_store.rs` — stale/render generation state.
- `crates/animatix-gui/src/app/stores/ui_store.rs` — `UiSnapshot`, scene-aware selection, UI-only state.
- `crates/animatix-gui/src/app/stores/export_store.rs` — export scope/state.

### Shell/runtime/commands

- `crates/animatix-gui/src/app/mod.rs` — shell frame pipeline and module wiring.
- `crates/animatix-gui/src/app/runtime.rs` — render/audio adapter use and snapshot rendering.
- `crates/animatix-gui/src/app/shell/mod.rs` — command dispatch.
- `crates/animatix-gui/src/app/shell/export_dialog.rs` — composition export.
- `crates/animatix-gui/src/app/shell/toolbar.rs` — rebuild/stale UI indicators.
- `crates/animatix-gui/src/app/shell/insertion_palette.rs` — active timeline insertion/labels.
- `crates/animatix-gui/src/app/shell/settings.rs` — undo/settings additions.
- `crates/animatix-gui/src/app/commands.rs` — command split and history entry migration.
- `crates/animatix-gui/src/app/command_bus.rs` — command bus.
- `crates/animatix-gui/src/app/effects.rs` — optional effect extraction.
- `crates/animatix-gui/src/app/document_controller.rs` — source command executor.
- `crates/animatix-gui/src/app/actions/mod.rs` — drag/property source commits and coalescing.

### Handlers

- `crates/animatix-gui/src/app/handlers/file.rs` — source store save/load/rebuild scheduling.
- `crates/animatix-gui/src/app/handlers/playback.rs` — stale diagnostics and active scene sync.
- `crates/animatix-gui/src/app/handlers/ui.rs` — undo/redo snapshots and export open.
- `crates/animatix-gui/src/app/handlers/actor.rs` — active timeline and history recording.
- `crates/animatix-gui/src/app/handlers/keyframe.rs` — active scene/keyframe commands.
- `crates/animatix-gui/src/app/handlers/property.rs` — scene/property command recording.
- `crates/animatix-gui/src/app/handlers/scene.rs` — UI snapshot/history integration.

### Panels/preview

- `crates/animatix-gui/src/app/panels/behavior.rs` — adapter then deletion.
- `crates/animatix-gui/src/app/panels/workspace.rs` — new workspace/view-model assembly.
- `crates/animatix-gui/src/app/panels/preview_model.rs` — preview view model.
- `crates/animatix-gui/src/app/panels/timeline_model.rs` — timeline view model.
- `crates/animatix-gui/src/app/panels/preview_panel.rs` — model + command bus.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs` — model + command bus.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs` — model + command bus.
- `crates/animatix-gui/src/app/panels/inspector/model.rs` — inspector model.
- `crates/animatix-gui/src/app/panels/inspector/property_groups.rs` — emit commands only.
- `crates/animatix-gui/src/app/panels/inspector/keyframe_table.rs` — scene-aware keyframe ids.
- `crates/animatix-gui/src/app/panels/sidebar.rs` — active timeline and command bus.
- `crates/animatix-gui/src/app/panels/editor.rs` — source edit event emission.
- `crates/animatix-gui/src/app/preview/context.rs` — active timeline overlays.
- `crates/animatix-gui/src/app/preview/drag_handler.rs` — command emission and active timeline.
- `crates/animatix-gui/src/app/preview/property_popup.rs` — command emission.
- `crates/animatix-gui/src/app/preview/selection.rs` — scene-aware selection.
- `crates/animatix-gui/src/app/components/diagnostics.rs` — stale diagnostics display.

### Services/persistence

- `crates/animatix-gui/src/app/services/mod.rs` — service module root.
- `crates/animatix-gui/src/app/services/renderer.rs` — preview renderer trait.
- `crates/animatix-gui/src/app/services/audio.rs` — audio preview trait.
- `crates/animatix-gui/src/app/services/screenshots.rs` — screenshot service.
- `crates/animatix-gui/src/preview_surface.rs` — renderer adapter backing implementation.
- `crates/animatix-gui/src/app/audio.rs` — audio adapter or move.
- `crates/animatix-gui/src/app/persistence.rs` — workspace persistence v2.

### Cargo

- `crates/animatix-gui/Cargo.toml` — add `crossbeam-channel = "0.5"` for R5 unless `std::sync::mpsc` is chosen.

---

## Global Risks

- Source edits are AST-wide today and not fully scene-targeted; R1/R2 fix active runtime selection, while R7 should make scene target explicit in commands.
- Background rebuild depends on thread-safety of build products; validate `Send` early in R5.
- Temporary compatibility mirrors can drift; every release should reduce direct `document_store.source.document.*` accesses.
- Import/module cache invalidation is already suspect; do not rely on source hash alone for imported file changes.
- Exporting stale source must be explicit until snapshots/rebuild worker enforce accepted generations.
- Large releases can create borrow-checker churn; land panel/handler migrations one cluster at a time.
