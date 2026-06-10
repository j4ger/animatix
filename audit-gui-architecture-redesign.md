# Animatix GUI Architecture Redesign

This document proposes a concrete, incremental redesign for `crates/animatix-gui` based on the findings in `audit-gui-uiux.md`. The goal is not a visual rewrite. The goal is to make the GUI correct for both single-scene timelines and multi-scene compositions, responsive under large documents, and maintainable as the app grows into a production animation workspace.

The core direction is:

- Source text is the canonical saved document.
- Parsed AST, type information, timelines, compositions, source indexes, hit regions, actor bounds, keyframe lists, diagnostics, and render products are derived state.
- Derived state is rebuilt as versioned snapshots, never mutated ad hoc by panels.
- Panels emit typed commands/events. A shell/controller layer routes commands, mutates source/UI state, and schedules rebuilds.
- Every editing surface resolves the active timeline through one composition-aware API.
- Long-running rebuild and render work leaves the UI thread.

## Current Failure Modes This Design Must Eliminate

The current GUI already has useful separation into stores, panels, handlers, preview code, and shell code, but the separation is incomplete. The most important correctness bugs are caused by these architectural patterns:

1. `DocumentSession` is both canonical source owner and derived-state owner. It stores source text, AST, indexes, timelines, composition, diagnostics, module graph, caches, active scene, dirty state, dimensions, duration, and rebuild caches.
2. Panels and handlers directly inspect `document.timeline` or duplicate fallback logic for `document.composition + active_scene`.
3. Rebuilds are debounced but still run synchronously on the UI frame.
4. Undo/redo records only source text, while many user-visible editing decisions live in UI state.
5. Multiple caches represent the same derived information without generation checks.
6. Runtime concerns are tangled: `AnimatixApp` owns shell state, WGPU preview surface state, egui texture state, screenshot state, and audio state.

This redesign keeps the current app recognizable while making data ownership, invalidation, and edit routing explicit.

## Design Principles

### Source-First, Derived-State-Second

`.amx` source text is the only canonical document model. Anything that can be recomputed from source is a derived snapshot with a generation number.

Canonical document state:

- Source text
- File path
- Dirty flag
- Project/workspace metadata that is intentionally persisted outside `.amx`, such as panel layout and user preferences

Derived document state:

- Raw AST
- Expanded AST
- Module graph outputs
- Namespaces/components/actions
- Source index
- Timeline or composition build target
- Diagnostics
- Duration and scene dimensions
- Timeline index and keyframe lines
- Actor/keyframe/hit-test caches

UI state:

- Active scene
- Selected actors/keyframes
- Tool mode
- Timeline zoom/scroll
- Playhead and loop region
- Sidebar tab and tile layout
- In-progress drag/edit interactions

Preview/runtime state:

- Playback clock
- Render surface resources
- Render quality
- Audio scheduling
- Last rendered generation
- Stale-preview status

### No Panel Mutates Document State Directly

Panels are views. They read immutable snapshots and emit typed actions.

Allowed panel behavior:

```rust
commands.push(Command::PropertyEdit(PropertyEdit { ... }));
commands.push(Command::SelectActor { actor, extend });
commands.push(Command::ScrubTo(time_s));
```

Disallowed panel behavior:

```rust
ctx.document.source_text = new_text;
ctx.document.timeline.as_mut().unwrap().tracks.insert(...);
ctx.ui_store.selection.selected_actors.clear();
```

The shell/controller pipeline is the only place that mutates canonical source or durable UI state.

### Composition Awareness Is Mandatory

A single-scene `.amx` and a composition `.amx` must use the same editing contract. The GUI should not ask each panel to decide whether `document.timeline` or `document.composition.scenes[active_scene]` is valid.

All editing surfaces use:

```rust
DocumentSession::active_timeline(&self) -> Option<ActiveTimelineRef<'_>>
DocumentSession::active_timeline_mut(&mut self) -> Option<ActiveTimelineMut<'_>>
```

The method owns all fallback and scene-resolution behavior.

### Stale Data Must Be Visible and Safe

While source text changes or a rebuild is running, the UI may continue showing last-known-good derived state. That state must be marked stale with generation tokens. Stale state can be displayed, but mutating commands must either:

- apply to source using source positions from a matching generation, or
- return a typed stale-state error and ask the user to wait for rebuild.

Silent mutation against the wrong timeline is not acceptable.

## Target Store Architecture

The shell owns five primary stores and one controller layer:

```rust
pub struct GuiShell {
    source: SourceStore,
    document: DocumentStore,
    ui: UiStore,
    history: HistoryStore,
    preview: PreviewStore,
    controller: DocumentController,
    command_bus: CommandBus,
}
```

During migration this can be introduced inside the existing `DocumentStore` facade to avoid a big-bang rewrite.

### `SourceStore`

`SourceStore` owns canonical source and file identity. It does not own AST, timeline, composition, diagnostics, or render caches.

```rust
pub struct SourceStore {
    file_path: PathBuf,
    text: Arc<str>,
    saved_text_hash: SourceHash,
    edit_epoch: SourceEpoch,
    dirty: bool,
    external_revision: Option<FileRevision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceEpoch(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceHash(pub u64);
```

Core API:

```rust
impl SourceStore {
    pub fn text(&self) -> &str;
    pub fn file_path(&self) -> &Path;
    pub fn epoch(&self) -> SourceEpoch;
    pub fn is_dirty(&self) -> bool;

    pub fn replace_text(&mut self, text: String) -> SourceChange;
    pub fn apply_text_edit(&mut self, edit: TextEdit) -> SourceChange;
    pub fn mark_saved(&mut self);
    pub fn load_from_disk(path: PathBuf) -> Result<Self, GuiError>;
    pub fn save_to_disk(&mut self) -> Result<(), GuiError>;
}

pub struct SourceChange {
    pub before: SourceEpoch,
    pub after: SourceEpoch,
    pub hash: SourceHash,
    pub diff: TextDiffSummary,
}
```

Rules:

- Every successful source mutation increments `edit_epoch`.
- `SourceStore` does not parse or rebuild.
- `EditorBuffer` should eventually be treated as a view buffer synchronized from/to `SourceStore`, not as a second canonical source. During migration, editor text can remain adjacent, but source writes must route through `SourceStore`.
- File save writes `SourceStore::text()`, not `EditorBuffer::text()` directly.

### `DocumentStore`

`DocumentStore` owns the latest derived document snapshot, rebuild status, and generation counters. It is rebuilt as a unit from a `SourceStore` snapshot.

```rust
pub struct DocumentStore {
    current: Option<Arc<DocumentSnapshot>>,
    last_good: Option<Arc<DocumentSnapshot>>,
    rebuild: RebuildState,
    worker: RebuildWorker,
}

pub struct DocumentSnapshot {
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub status: SnapshotStatus,

    pub raw_statements: Option<Arc<[Stmt]>>,
    pub expanded_statements: Option<Arc<[Stmt]>>,
    pub namespaces: Arc<HashMap<String, Namespace>>,
    pub components: Arc<HashMap<String, ComponentEntry>>,
    pub module_actions: Arc<HashMap<String, ActionTemplate>>,
    pub source_index: Option<Arc<SourceIndex>>,
    pub timeline_index: TimelineIndex,
    pub keyframe_lines: Versioned<Vec<usize>>,

    pub target: BuildTargetSnapshot,
    pub diagnostics: Arc<[Diagnostic]>,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
    pub caches: DerivedCaches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentGeneration(pub u64);

pub enum SnapshotStatus {
    Clean,
    Stale { current_source_epoch: SourceEpoch },
    Failed { error: String },
}

pub enum BuildTargetSnapshot {
    Empty,
    Timeline(Arc<Timeline>),
    Composition(Arc<Composition>),
}
```

`current` is the newest completed rebuild, including failed snapshots with diagnostics. `last_good` is the newest snapshot that has a usable timeline or composition. Preview can continue rendering `last_good` while `current` is failed or stale.

Rules:

- A snapshot is immutable after publication.
- `DocumentGeneration` increments only when a rebuild result is accepted by the UI thread.
- A failed rebuild still produces a new generation with diagnostics and no active build target.
- Consumers must not keep raw references across frames. They may keep `(Arc<DocumentSnapshot>, DocumentGeneration)`.

### `DerivedCaches`

Derived collections carry their origin generation and optional scene id.

```rust
pub struct DerivedCaches {
    pub actors: VersionedSceneMap<Vec<ActorSummary>>,
    pub keyframes: VersionedSceneMap<Vec<ActorKeyframeSummary>>,
    pub hit_regions: VersionedSceneMap<Vec<HitRegion>>,
    pub actor_bounds: VersionedSceneMap<HashMap<String, Rect>>,
    pub motion_paths: VersionedSceneMap<HashMap<String, MotionPathCache>>,
}

pub struct Versioned<T> {
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub value: T,
}

pub struct VersionedSceneMap<T> {
    pub generation: DocumentGeneration,
    pub entries: HashMap<SceneKey, T>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SceneKey {
    SingleScene,
    Scene(String),
}
```

Cache access should look like this:

```rust
let timeline = document.active_timeline(ui.active_scene())?;
let actors = document.caches().actors.for_scene(timeline.scene_key())?;
if actors.generation != timeline.generation {
    return Err(StaleDataError::Actors);
}
```

During migration, existing `SourceStore::cached_actor_labels`, `cached_actor_keyframes`, `cached_hit_regions`, and `cached_actor_bounds` can be wrapped with generation fields before they are moved into `DocumentStore`.

### `UiStore`

`UiStore` owns durable UI state and transient interaction state. It should not own source text or derived document caches.

```rust
pub struct UiStore {
    pub selection: SelectionStore,
    pub interaction: InteractionStore,
    pub viewport: ViewportStore,
    pub workspace: WorkspaceLayoutStore,
    pub inspector: InspectorUiStore,
    pub timeline: TimelineUiStore,
    pub editor: EditorUiStore,
    pub panels: PanelVisibilityStore,
    pub settings: GuiSettings,
    pub pending_actions: ActionQueue,
    pub toasts: ToastQueue,
}
```

Important snapshot type for undo/redo:

```rust
#[derive(Debug, Clone)]
pub struct UiSnapshot {
    pub active_scene: Option<String>,
    pub selected_actors: Vec<String>,
    pub selected_keyframes: Vec<KeyframeId>,
    pub playhead_time_s: f64,
    pub loop_region: Option<TimeRange>,
    pub timeline_zoom: f32,
    pub timeline_scroll_offset: f32,
    pub preview_zoom: PreviewZoomMode,
    pub preview_pan: egui::Vec2,
    pub tool_mode: ToolMode,
    pub sidebar_tab: SidebarTab,
    pub property_view_mode: PropertyViewMode,
    pub keyframe_view_mode: KeyframeViewMode,
}
```

Do not include ephemeral state that should be cancelled by undo/redo, such as an active drag. Undo/redo should explicitly clear drag state.

### `HistoryStore`

`HistoryStore` owns undo/redo entries with source and UI snapshots. It also owns configurable depth.

```rust
pub struct HistoryStore {
    undo_stack: VecDeque<UndoEntry>,
    redo_stack: VecDeque<UndoEntry>,
    undo_limit: usize,
}

pub struct UndoEntry {
    pub label: String,
    pub command: CommandKind,
    pub source_before: Arc<str>,
    pub source_after: Arc<str>,
    pub ui_before: UiSnapshot,
    pub ui_after: UiSnapshot,
    pub source_epoch_before: SourceEpoch,
    pub source_epoch_after: SourceEpoch,
}
```

For high-frequency drags, history should coalesce updates:

```rust
pub enum HistoryPolicy {
    RecordImmediate,
    BeginCoalesced { group: HistoryGroup, label: String },
    UpdateCoalesced { group: HistoryGroup },
    CommitCoalesced { group: HistoryGroup },
    Skip,
}
```

Settings:

```rust
pub struct HistorySettings {
    pub undo_limit: usize,
    pub coalesce_drag_edits: bool,
    pub max_snapshot_bytes: usize,
}
```

### `PreviewStore`

`PreviewStore` owns runtime playback and render state, not source or document build products.

```rust
pub struct PreviewStore {
    pub playback: PlaybackState,
    pub render: RenderState,
    pub quality: PreviewQuality,
    pub viewport: PreviewViewport,
    pub stale: StalePreviewState,
    pub audio: AudioPreviewState,
}

pub struct StalePreviewState {
    pub rendered_generation: Option<DocumentGeneration>,
    pub requested_generation: Option<DocumentGeneration>,
    pub stale_since: Option<Instant>,
    pub reason: Option<StaleReason>,
}

pub enum StaleReason {
    SourceEdited,
    RebuildRunning,
    RebuildFailed,
    RenderRunning,
}
```

The preview can render from `DocumentStore::last_good()` and overlay a watermark:

- `Preview is stale: rebuilding...`
- `Preview is stale: source has errors`
- `Preview is stale: render pending`

## Unified Timeline Access

`DocumentSession` or its replacement must expose one API for resolving the editable timeline. This is the first migration target.

### Types

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActiveSceneId {
    SingleScene,
    Scene(String),
}

pub struct ActiveTimelineRef<'a> {
    pub id: ActiveSceneId,
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub timeline: &'a Timeline,
    pub composition: Option<&'a Composition>,
    pub scene_name: Option<&'a str>,
    pub duration_s: f64,
    pub dimensions: SceneDimensions,
}

pub struct ActiveTimelineMut<'a> {
    pub id: ActiveSceneId,
    pub timeline: &'a mut Timeline,
    pub scene_name: Option<String>,
}
```

### API

For the current `DocumentSession` during migration:

```rust
impl DocumentSession {
    pub fn active_timeline(&self) -> Option<ActiveTimelineRef<'_>> {
        if let Some(timeline) = self.timeline.as_ref() {
            return Some(ActiveTimelineRef {
                id: ActiveSceneId::SingleScene,
                generation: self.generation(),
                source_epoch: self.source_epoch(),
                timeline,
                composition: None,
                scene_name: None,
                duration_s: self.duration_s,
                dimensions: self.scene_dimensions,
            });
        }

        let composition = self.composition.as_ref()?;
        let scene_name = self.resolve_active_scene_name(composition)?;
        let scene = composition.scenes.get(scene_name)?;

        Some(ActiveTimelineRef {
            id: ActiveSceneId::Scene(scene_name.to_string()),
            generation: self.generation(),
            source_epoch: self.source_epoch(),
            timeline: &scene.timeline,
            composition: Some(composition),
            scene_name: Some(scene_name),
            duration_s: scene.timeline.duration_s().max(0.1),
            dimensions: scene.timeline.scene_dimensions,
        })
    }

    pub fn active_timeline_mut(&mut self) -> Option<ActiveTimelineMut<'_>> {
        if let Some(timeline) = self.timeline.as_mut() {
            return Some(ActiveTimelineMut {
                id: ActiveSceneId::SingleScene,
                timeline,
                scene_name: None,
            });
        }

        let scene_name = {
            let composition = self.composition.as_ref()?;
            self.resolve_active_scene_name(composition)?.to_string()
        };

        let scene = self.composition.as_mut()?.scenes.get_mut(&scene_name)?;
        Some(ActiveTimelineMut {
            id: ActiveSceneId::Scene(scene_name.clone()),
            timeline: &mut scene.timeline,
            scene_name: Some(scene_name),
        })
    }

    fn resolve_active_scene_name<'a>(&'a self, composition: &'a Composition) -> Option<&'a str> {
        if let Some(active) = self.active_scene.as_deref() {
            if composition.scenes.contains_key(active) {
                return Some(active);
            }
        }
        composition.entry_scene.as_deref()
            .or_else(|| composition.scene_order.first().map(String::as_str))
            .or_else(|| composition.scenes.keys().next().map(String::as_str))
    }
}
```

For the target immutable snapshot model:

```rust
impl DocumentSnapshot {
    pub fn active_timeline(&self, active_scene: Option<&str>) -> Option<ActiveTimelineRef<'_>>;
    pub fn timeline_for_scene(&self, scene: &ActiveSceneId) -> Option<ActiveTimelineRef<'_>>;
    pub fn export_target(&self, requested_scene: Option<&str>) -> Option<ExportTargetRef<'_>>;
}
```

### Required Adoption

These code paths must stop reading `document.timeline` directly:

- Preview panel context
- Preview drag handlers
- Preview overlay/motion path rendering
- Vertex handle drawing/editing
- Inspector property groups
- Timeline panel
- Layers tab
- Actor creation/insertion
- Label generation
- Layout reorder
- Keyframe editor
- Export dialog duration preview
- Export start

The rule is simple: if a panel or handler asks, "which timeline am I editing?", it must call the shared API.

## Rebuild Pipeline

The rebuild pipeline must move parse/typecheck/build work off the egui frame. The UI thread should submit rebuild jobs, poll for completed results, and atomically publish accepted snapshots.

### Worker Model

Use either `std::thread + channels` or `tokio`. The simplest robust choice is a dedicated standard thread because the GUI already has a single UI runtime and rebuilds are CPU/filesystem-bound.

```rust
pub struct RebuildWorker {
    tx: crossbeam_channel::Sender<RebuildRequest>,
    rx: crossbeam_channel::Receiver<RebuildResponse>,
    latest_requested: Option<RebuildToken>,
    latest_accepted: Option<RebuildToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RebuildToken(pub u64);

pub struct RebuildRequest {
    pub token: RebuildToken,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub file_path: PathBuf,
    pub source_text: Arc<str>,
    pub quality: BuildQuality,
    pub cancellation: CancellationToken,
    pub cache_seed: RebuildCacheSeed,
}

pub struct RebuildResponse {
    pub token: RebuildToken,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub result: Result<RebuildOutput, RebuildFailure>,
    pub timings: RebuildTimings,
}
```

`CancellationToken` can be implemented with `Arc<AtomicU64>`:

```rust
#[derive(Clone)]
pub struct CancellationToken {
    generation: u64,
    shared_latest: Arc<AtomicU64>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.shared_latest.load(Ordering::Relaxed) != self.generation
    }
}
```

When source changes:

1. `SourceStore` increments `SourceEpoch`.
2. `DocumentStore` marks current snapshot stale.
3. `PreviewStore` marks rendered preview stale.
4. Debounce timer starts or resets.
5. When debounce expires, shell submits a rebuild request with the latest source epoch.
6. Submitting a new request cancels older tokens.

### Rebuild Output

```rust
pub struct RebuildOutput {
    pub raw_statements: Vec<Stmt>,
    pub expanded_statements: Vec<Stmt>,
    pub namespaces: HashMap<String, Namespace>,
    pub components: HashMap<String, ComponentEntry>,
    pub module_actions: HashMap<String, ActionTemplate>,
    pub source_index: SourceIndex,
    pub target: BuildTargetSnapshotOwned,
    pub diagnostics: Vec<Diagnostic>,
    pub timeline_index: TimelineIndex,
    pub keyframe_lines: Vec<usize>,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
    pub caches: DerivedCachesOwned,
}

pub struct RebuildFailure {
    pub error: String,
    pub diagnostics: Vec<Diagnostic>,
    pub partial_source_index: Option<SourceIndex>,
}
```

### Acceptance Protocol

The UI thread must reject stale worker responses.

```rust
impl DocumentStore {
    pub fn poll_rebuilds(&mut self, source: &SourceStore) -> Vec<Effect> {
        let mut effects = Vec::new();
        while let Ok(response) = self.worker.rx.try_recv() {
            if response.source_epoch != source.epoch() {
                continue;
            }
            if Some(response.token) != self.worker.latest_requested {
                continue;
            }
            self.accept_rebuild(response, source.epoch(), source.hash(), &mut effects);
        }
        effects
    }
}
```

Acceptance increments `DocumentGeneration` and publishes a new immutable snapshot.

```rust
fn accept_rebuild(&mut self, response: RebuildResponse, epoch: SourceEpoch, hash: SourceHash) {
    let generation = self.next_generation();
    let snapshot = Arc::new(DocumentSnapshot::from_response(generation, epoch, hash, response));
    if snapshot.has_renderable_target() {
        self.last_good = Some(snapshot.clone());
    }
    self.current = Some(snapshot);
    self.rebuild = RebuildState::Idle;
}
```

### Progressive Rendering

Rebuild and render are separate stages:

1. Source edit makes document snapshot stale.
2. Preview continues showing `last_good` render.
3. Rebuild completes and publishes generation `N`.
4. Preview marks render requested for generation `N`.
5. Render surface renders generation `N`.
6. Preview updates `rendered_generation = N` and clears stale watermark.

If rebuild fails, preview keeps `last_good` and displays diagnostics + stale watermark. It should not clear the preview to blank unless there has never been a good render.

### Diagnostics During Rebuild

Do not clear diagnostics optimistically on edit. Replace current behavior with:

```rust
pub enum DiagnosticFreshness {
    Current(DocumentGeneration),
    Stale { from_generation: DocumentGeneration, current_source_epoch: SourceEpoch },
    Failed(DocumentGeneration),
}
```

The diagnostics panel should display stale diagnostics dimmed with a `stale` badge until rebuild completes.

## Undo/Redo Redesign

Undo must restore both source text and relevant UI state.

### Snapshot-Based Source Undo

For text-backed edits, undo entries include `source_before`, `source_after`, `ui_before`, and `ui_after`.

```rust
impl HistoryStore {
    pub fn record_source_change(
        &mut self,
        command: CommandKind,
        before_source: Arc<str>,
        after_source: Arc<str>,
        before_ui: UiSnapshot,
        after_ui: UiSnapshot,
    );

    pub fn undo(&mut self, source: &mut SourceStore, ui: &mut UiStore) -> Option<UndoResult>;
    pub fn redo(&mut self, source: &mut SourceStore, ui: &mut UiStore) -> Option<UndoResult>;
}
```

Undo protocol:

1. Cancel active drags and inspector edits.
2. Pop undo entry.
3. Replace `SourceStore` text with `source_before`.
4. Restore `UiSnapshot::before`.
5. Push entry onto redo stack.
6. Schedule rebuild immediately, not only after debounce.
7. Mark preview stale.

Redo mirrors this with `source_after` and `ui_after`.

### Command Pattern for Non-Text Edits

Visual edits should be represented as commands that know how to produce source diffs. The command itself is the semantic operation; the persisted undo entry still stores source snapshots for safety.

```rust
pub trait DocumentCommand {
    fn label(&self) -> &'static str;
    fn history_policy(&self) -> HistoryPolicy;
    fn apply(&self, ctx: &mut CommandContext<'_>) -> Result<CommandOutcome, CommandError>;
}

pub struct CommandContext<'a> {
    pub source: &'a mut SourceStore,
    pub document: &'a DocumentSnapshot,
    pub ui: &'a mut UiStore,
    pub source_edits: &'a SourceEditEngine,
}

pub struct CommandOutcome {
    pub source_changed: bool,
    pub ui_changed: bool,
    pub rebuild: RebuildRequestKind,
    pub effects: Vec<Effect>,
}
```

Examples:

```rust
pub struct MoveKeyframeCommand {
    pub scene: ActiveSceneId,
    pub actor: String,
    pub property: String,
    pub old_time_s: f64,
    pub new_time_s: f64,
}

pub struct SetPropertyCommand {
    pub scene: ActiveSceneId,
    pub actor: String,
    pub property: String,
    pub value: PropertyValue,
    pub create_keyframe: bool,
    pub time_s: Option<f64>,
}

pub struct ReorderLayoutChildrenCommand {
    pub scene: ActiveSceneId,
    pub container: String,
    pub child_order: Vec<String>,
}
```

`SourceEdit` remains the mechanism for updating `.amx`, but it should be called from commands/controllers, not panels.

### High-Frequency Drag Coalescing

For drag operations:

- `DragStarted`: capture `source_before` and `ui_before`.
- `DragUpdated`: update in-memory preview overlay if safe, and optionally apply temporary source edits to a draft buffer.
- `DragEnded`: apply final source edit, capture `source_after` and `ui_after`, record one undo entry.
- `DragCancelled`: restore `source_before` and `ui_before` without recording history.

This avoids one undo entry per pointer move and makes source changes predictable.

## Panel and Widget Communication

The current `WorkspaceBehavior` passes a large mutable context into panels. Replace it with a typed view model + command/event bus.

### Command Bus

```rust
pub struct CommandBus {
    queue: VecDeque<ShellAction>,
}

impl CommandBus {
    pub fn emit(&mut self, action: impl Into<ShellAction>);
    pub fn drain(&mut self) -> impl Iterator<Item = ShellAction> + '_;
}
```

Actions split by responsibility:

```rust
pub enum ShellAction {
    Document(DocumentCommandKind),
    Playback(PlaybackCommand),
    Selection(SelectionCommand),
    View(ViewCommand),
    File(FileCommand),
    Export(ExportCommand),
}
```

Document commands can mutate source. View/selection/playback commands mutate UI/runtime state. File/export commands may cause side effects.

### View Models

Panels receive immutable view models and a mutable command bus, not store references.

```rust
pub struct PreviewPanelModel<'a> {
    pub timeline: Option<ActiveTimelineRef<'a>>,
    pub composition: Option<&'a Composition>,
    pub snapshot_generation: Option<DocumentGeneration>,
    pub render_texture: Option<egui::TextureId>,
    pub playback: &'a PlaybackState,
    pub selection: &'a SelectionView,
    pub viewport: &'a PreviewViewport,
    pub stale: &'a StalePreviewState,
    pub hit_regions: Option<&'a Versioned<Vec<HitRegion>>>,
}

pub fn preview_panel_ui(ui: &mut egui::Ui, model: PreviewPanelModel<'_>, bus: &mut CommandBus);
```

Timeline panel:

```rust
pub struct TimelinePanelModel<'a> {
    pub active_timeline: Option<ActiveTimelineRef<'a>>,
    pub actors: Option<&'a Versioned<Vec<ActorSummary>>>,
    pub keyframes: Option<&'a Versioned<Vec<ActorKeyframeSummary>>>,
    pub playback: &'a PlaybackState,
    pub timeline_ui: &'a TimelineUiStore,
    pub selection: &'a SelectionView,
    pub snap: SnapSettings,
}
```

Inspector:

```rust
pub struct InspectorModel<'a> {
    pub active_timeline: Option<ActiveTimelineRef<'a>>,
    pub selected_actors: &'a [String],
    pub components: &'a HashMap<String, ComponentEntry>,
    pub property_view_mode: PropertyViewMode,
    pub keyframe_view_mode: KeyframeViewMode,
    pub current_time_s: f64,
}
```

Benefits:

- Panels become easier to test because their inputs are explicit.
- Panels cannot accidentally mutate document state.
- Composition resolution happens before panel rendering.
- The shell owns the frame transaction.

### Frame Pipeline

Target frame loop:

```rust
fn update(&mut self, ctx: &egui::Context) {
    self.shell.poll_external_events();
    self.shell.poll_rebuilds();
    self.shell.poll_render_jobs();
    self.shell.tick_playback(ctx.input(|i| i.stable_dt));

    egui::CentralPanel::default().show(ctx, |ui| {
        let models = self.shell.build_view_models();
        workspace_ui(ui, models, &mut self.shell.command_bus);
    });

    self.shell.dispatch_actions();
    self.shell.schedule_rebuilds_and_renders();
    self.shell.persist_if_needed();
}
```

Ordering matters:

1. Poll completed background work first.
2. Tick playback against the latest accepted snapshot.
3. Render UI from immutable models.
4. Dispatch queued commands after UI.
5. Schedule side effects after mutations.

## Composition-Aware Editing

Every edit command must include or resolve a scene target.

### Scene Target Resolution

```rust
pub enum SceneTarget {
    Active,
    SingleScene,
    Scene(String),
}

impl SceneTarget {
    pub fn resolve(self, snapshot: &DocumentSnapshot, ui: &UiStore) -> Result<ActiveSceneId, CommandError>;
}
```

Default panel commands should use `SceneTarget::Active`. Commands from storyboard/scene list can use explicit `SceneTarget::Scene(name)`.

### Drag Handlers

Preview drag handlers should receive an `ActiveTimelineRef`, not `Option<&Timeline>`.

```rust
pub struct DragContext<'a> {
    pub timeline: ActiveTimelineRef<'a>,
    pub hit_regions: &'a Versioned<Vec<HitRegion>>,
    pub selection: &'a SelectionView,
    pub tool_mode: ToolMode,
    pub snap: SnapSettings,
}
```

Generated commands include the resolved scene id:

```rust
Command::Document(DocumentCommandKind::SetProperty {
    scene: drag_ctx.timeline.id.clone(),
    actor,
    property,
    value,
    create_keyframe,
    time_s,
})
```

Lock checks, vertex editing, motion-path keyframe hit testing, and layout reorder must all use `drag_ctx.timeline.timeline` from the active timeline API.

### Inspector

Inspector property rows use `InspectorModel::active_timeline`. If selected actors span scenes in future multi-scene selection, the inspector should show a mixed-scene state and require an explicit target. For now, selection should be scene-local:

- Changing active scene clears actor selection unless labels are known to exist in the new scene and the command explicitly preserves them.
- `SelectionStore` includes `scene: ActiveSceneId`.

```rust
pub struct SelectionStore {
    pub scene: Option<ActiveSceneId>,
    pub selected_actors: BTreeSet<String>,
    pub selected_keyframes: BTreeSet<KeyframeId>,
}
```

### Keyframe Editor and Timeline

Keyframe commands must identify scene, actor, property, and old/new time.

```rust
pub struct KeyframeId {
    pub scene: ActiveSceneId,
    pub actor: String,
    pub property: String,
    pub time_ms: u64,
}
```

Frame snapping should use project/UI settings, not hardcoded 60fps:

```rust
pub struct SnapSettings {
    pub enabled: bool,
    pub fps: f64,
    pub mode: SnapMode,
    pub modifier_bypass: egui::Modifiers,
}

pub enum SnapMode {
    Frame,
    Grid { interval_s: f64 },
    Keyframes,
    Markers,
}
```

### Motion Paths, Vertex Handles, Ghost Overlays

All overlays should be scene-aware:

```rust
pub struct OverlayContext<'a> {
    pub active_timeline: ActiveTimelineRef<'a>,
    pub playback_time_s: f64,
    pub selected_actors: &'a BTreeSet<String>,
    pub caches: &'a DerivedCaches,
    pub viewport: &'a PreviewViewport,
}
```

Overlay drawing rules:

- If cache generation matches active timeline generation, draw normally.
- If cache is stale but belongs to the same scene, draw dimmed or skip handles that would allow mutation.
- If cache belongs to a different scene, ignore it.

### Layout Reorder

Layout reorder should be a source command:

```rust
pub struct ReorderChildrenCommand {
    pub scene: ActiveSceneId,
    pub container_label: String,
    pub child_order: Vec<String>,
}
```

For live preview during drag, use a temporary overlay model or a draft timeline clone. Do not mutate `document.timeline` only for single-scene documents.

### Actor Creation and Label Generation

Actor insertion command:

```rust
pub struct CreateActorCommand {
    pub scene: SceneTarget,
    pub parent: Option<String>,
    pub actor_type: ActorType,
    pub requested_label: Option<String>,
    pub position: Option<[f32; 2]>,
    pub props: Vec<Property>,
}
```

Label generation must use the resolved active timeline plus source index:

```rust
pub fn unique_actor_label(
    snapshot: &DocumentSnapshot,
    scene: &ActiveSceneId,
    requested: &str,
) -> String;
```

Collision policy:

- Avoid collisions inside the target scene.
- Warn if the label collides with another scene when commands or source references are scene-ambiguous.
- Prefer source-index-backed checks when raw AST is available.

### Export

Export must support both target forms:

```rust
pub enum ExportTargetRef<'a> {
    Timeline {
        scene: ActiveSceneId,
        timeline: &'a Timeline,
        duration_s: f64,
        dimensions: SceneDimensions,
    },
    Composition {
        composition: &'a Composition,
        duration_s: f64,
        dimensions: SceneDimensions,
    },
}

pub enum ExportScope {
    ActiveScene,
    WholeComposition,
    Scene(String),
    WorkArea(TimeRange),
}
```

Export dialog behavior:

- If document is single-scene, default scope is `ActiveScene` and target is `Timeline`.
- If document is composition, default scope is `WholeComposition`.
- The dialog can offer `Active scene only` as an explicit option.
- Duration preview uses `ExportTargetRef::duration_s`.
- Export worker receives an owned target clone from the latest accepted generation.
- If source has unaccepted edits, show `Exporting last rebuilt version` or require rebuild before export, depending on user setting.

## Layout and Workspace

### Persistent Panel Layout

Persist layout as versioned workspace UI state:

```rust
pub struct WorkspacePersistenceV2 {
    pub schema_version: u32,
    pub window: WindowPersistence,
    pub tiles: TilePersistence,
    pub panels: PanelPersistence,
    pub timeline: TimelinePersistence,
    pub preview: PreviewPersistence,
    pub editor: EditorPersistence,
}

pub struct PanelPersistence {
    pub sidebar_tab: SidebarTab,
    pub inspector_open: bool,
    pub diagnostics_open: bool,
    pub command_palette_recent: Vec<String>,
}
```

Persistence should be saved:

- On clean shutdown
- After layout changes with debounce
- After active sidebar tab changes
- After timeline/preview zoom changes

Migration should keep reading the existing layout format and write the new format after the first successful save.

### Flexible Preview Sizing

Preview panel should allocate the available pane and fit content inside it, not shrink the canvas allocation to scene size.

```rust
pub enum PreviewZoomMode {
    Fit,
    Fill,
    Percent(f32),
    ActualSize,
}

pub struct PreviewViewport {
    pub zoom_mode: PreviewZoomMode,
    pub resolved_zoom: f32,
    pub pan: egui::Vec2,
    pub min_zoom: f32,
    pub max_zoom: f32,
}
```

Sizing algorithm:

1. Allocate all available panel space, respecting only a small minimum that keeps controls usable.
2. Compute scene-to-panel transform from `PreviewZoomMode`.
3. Center the scene for `Fit`, crop for `Fill`, and preserve pan for custom zoom.
4. Hit testing maps pointer coordinates through the same transform.
5. Store zoom/pan in `UiStore::viewport` and persist it.

### DPI and Window State

Egui handles most DPI scaling, but persistence must store logical sizes and validate them on restore.

```rust
pub struct WindowPersistence {
    pub logical_size: [f32; 2],
    pub maximized: bool,
    pub pixels_per_point: Option<f32>,
}
```

Restore rules:

- Clamp restored size to current monitor work area.
- Enforce minimum logical size, for example `900x600`.
- If restored position is off-screen, center the window.
- Persist maximized state separately from logical size.

## Runtime Decoupling

`AnimatixApp` should become a thin eframe adapter. It owns eframe-specific resources and delegates business logic to shell/runtime services.

Target split:

```rust
pub struct AnimatixEframeApp {
    shell: GuiShell,
    surface: EguiPreviewSurface,
    texture: PreviewTextureHandle,
    screenshots: ScreenshotService,
}

pub trait PreviewRenderer {
    fn render(&mut self, request: RenderRequest<'_>) -> Result<RenderResult, RenderError>;
    fn texture_id(&self) -> Option<egui::TextureId>;
    fn resize(&mut self, size: [u32; 2]);
}
```

The shell should not know whether preview rendering is eframe WGPU, a mock renderer, an offscreen renderer for screenshots, or a future non-eframe backend.

Audio should also be a service:

```rust
pub trait AudioPreviewEngine {
    fn sync(&mut self, composition_or_timeline: AudioSourceRef<'_>, playback: &PlaybackState);
    fn stop(&mut self);
    fn set_enabled(&mut self, enabled: bool);
}
```

This makes tests and future preview backends easier.

## Module Structure

Target `crates/animatix-gui/src/` structure:

```text
src/
  main.rs
  lib.rs
  app/
    runtime.rs              # thin eframe adapter; owns shell + renderer services
    shell.rs                # central coordinator, frame pipeline, command dispatch
    command_bus.rs          # typed action queue
    effects.rs              # toasts, repaint, editor focus, modal effects
    document/
      mod.rs
      session.rs            # temporary compatibility facade during migration
      snapshot.rs           # immutable DocumentSnapshot and active timeline API
      controller.rs         # applies commands and source edits
      rebuild.rs            # worker, request/response, cancellation, acceptance
      export_target.rs      # composition/timeline export resolution
      stores/
        source_store.rs
        document_store.rs
        history_store.rs
        preview_store.rs
        ui_store.rs
        workspace_store.rs
    handlers/
      mod.rs
      file.rs               # file system side effects only
      playback.rs           # playback commands only
      selection.rs
      export.rs
      commands.rs           # Command enum definitions and conversion glue
    panels/
      workspace.rs          # tiling logic and view-model assembly
      toolbar.rs
      sidebar.rs
      preview_panel.rs
      timeline_panel.rs
      diagnostics_panel.rs
      inspector/
        mod.rs
        model.rs
        property_groups.rs
        keyframe_table.rs
        graph_editor.rs
    preview/
      mod.rs
      context.rs            # viewport transforms and overlay model
      drag_handler.rs       # emits commands, no document mutation
      grid.rs
      overlay.rs
      property_popup.rs
      selection.rs
      time_lens.rs
    components/
      button.rs
      context_menu.rs
      diagnostics.rs
      easing_curve_editor.rs
      layout.rs
      row.rs
      timeline.rs
      toast.rs
    services/
      renderer.rs           # PreviewRenderer trait and render requests
      audio.rs              # AudioPreviewEngine trait/adapters
      screenshots.rs
      persistence.rs
    design_tokens.rs
  cell_editor/
  editor/
  source_edit/
  preview_surface.rs        # eframe/wgpu implementation; eventually services/renderer adapter
  completion_popup.rs
  error.rs
  highlighting.rs
  hot_reload.rs
  text_diff.rs
  validation.rs
```

Migration-friendly intermediate structure:

- Keep `app/mod.rs` as a facade initially.
- Add `app/document/` and move new snapshot/rebuild/controller code there.
- Re-export old names from `app/stores/mod.rs` while callers move.
- Rename `app/mod.rs` to `app/shell.rs` only after command routing is stable.

## Invalidation Protocols

### Source Mutation

When source text changes:

```rust
let change = source.apply_text_edit(edit);
document.mark_source_stale(change.after);
preview.mark_stale(StaleReason::SourceEdited);
ui.editor.mark_synced_to_epoch(change.after);
scheduler.schedule_rebuild(change.after, debounce);
```

Immediate invalidation:

- Derived document snapshot status becomes stale.
- Hit regions, actor bounds, and keyframe lists remain displayable only with stale generation markers.
- Commands requiring exact source positions must check snapshot epoch.

### Rebuild Completion

When rebuild succeeds:

```rust
document.accept_success(output);
preview.request_render(document.current_generation());
ui.reconcile_selection(document.current_snapshot());
history.update_epoch_after_rebuild(source.epoch());
```

Selection reconciliation:

- If active scene no longer exists, switch to entry scene and clear scene-local selection.
- Remove selected actors that no longer exist in active timeline.
- Remove selected keyframes that no longer exist.
- Preserve playhead time but clamp to new duration.

### Rebuild Failure

When rebuild fails:

```rust
document.accept_failure(failure);
preview.mark_stale(StaleReason::RebuildFailed);
ui.keep_selection_but_disable_mutating_tools();
```

Rules:

- Diagnostics are current for the failed source epoch.
- Last-good preview remains visible.
- Inspector/timeline can show last-good data but mutation commands must either target source text directly or be disabled with an explanation.

### Render Completion

When render succeeds:

```rust
if result.document_generation == document.current_generation() {
    preview.rendered_generation = Some(result.document_generation);
    preview.clear_stale_if_current();
    ui.selection.hit_regions = result.hit_regions.versioned(result.document_generation);
}
```

Hit regions from render must carry generation and scene key. Selection commands check this before using them.

## Migration Path

This should be a strangler-fig migration, not a rewrite.

### Phase 0: Add Compatibility Generation Fields

Add lightweight generation/epoch fields to existing `DocumentSession` and stores.

```rust
pub struct DocumentSession {
    // existing fields...
    pub source_epoch: SourceEpoch,
    pub document_generation: DocumentGeneration,
}
```

Increment `source_epoch` in `set_source_text`, `commit_source`, editor change handling, undo, redo, reload, and any source-edit path. Increment `document_generation` after successful or failed `rebuild`.

Wrap existing caches:

```rust
pub struct CacheState<T> {
    pub generation: DocumentGeneration,
    pub value: T,
}
```

This makes stale-state bugs observable before moving to background rebuilds.

### Phase 1: Centralize Active Timeline Resolution

Add `DocumentSession::active_timeline()` and `active_timeline_mut()` exactly as compatibility APIs.

Then migrate callers one by one:

1. `app/panels/behavior.rs`
2. `app/panels/preview_panel.rs`
3. `app/preview/drag_handler.rs`
4. `app/preview/context.rs`
5. `app/panels/inspector/mod.rs`
6. `app/panels/timeline_panel.rs`
7. `app/actions/mod.rs`
8. `app/document_controller.rs`
9. `app/shell/export_dialog.rs`

Acceptance criteria:

- No panel reads `document.timeline.as_ref()` directly except inside the active-timeline API or explicit export target resolution.
- Composition documents can select locked actors correctly, draw motion paths, edit vertices, reorder layout, create actors, edit inspector properties, and move keyframes in the active scene.
- GUI export can export whole compositions and active scenes.

### Phase 2: Extract `SourceStore` Canonical Ownership

Move `file_path`, `source_text`, and `is_dirty` out of `DocumentSession` into `SourceStore` as canonical fields.

Compatibility shim:

```rust
impl DocumentSession {
    pub fn source_text(&self, source: &SourceStore) -> &str { source.text() }
}
```

Update save/open/reload/editor paths so they call `SourceStore` methods. `EditorBuffer` remains but is synchronized through source changes.

Acceptance criteria:

- Save always writes `SourceStore::text()`.
- Rebuild consumes a source snapshot from `SourceStore`.
- Undo/redo replaces `SourceStore` text, not `DocumentSession.source_text` directly.

### Phase 3: Extract Immutable `DocumentSnapshot`

Create `DocumentSnapshot` and make `DocumentSession::rebuild` produce a snapshot internally.

Intermediary API:

```rust
impl DocumentSession {
    pub fn rebuild_snapshot(&mut self, source: &SourceStore) -> Result<DocumentSnapshot, GuiError>;
    pub fn apply_snapshot(&mut self, snapshot: DocumentSnapshot);
}
```

Then move derived fields from `DocumentSession` into `DocumentStore::current`.

Acceptance criteria:

- Panels can be given `&DocumentSnapshot` instead of `&DocumentSession`.
- `DocumentSession` becomes either a compatibility facade or disappears.
- Derived caches live under the snapshot generation.

### Phase 4: Background Rebuild Worker

Move rebuild execution to `app/document/rebuild.rs`.

Start with `std::thread + crossbeam_channel` because it is simple and avoids introducing async runtime coupling. Add cancellation tokens. Keep the old synchronous path behind a debug setting for easier bisecting.

Acceptance criteria:

- Typing large documents no longer blocks frame rendering.
- Rapid typing cancels older rebuilds and accepts only the newest source epoch.
- Diagnostics are marked stale instead of cleared.
- Preview displays last-good render with a stale watermark while rebuild runs.

### Phase 5: Undo/Redo with UI Snapshots

Add `UiSnapshot` and change `HistoryStore::snapshot` to capture source + UI before/after.

Migrate commands:

1. Text editor changes
2. Inspector property edits
3. Canvas drags
4. Keyframe moves
5. Scene selection/reorder
6. Timeline zoom/loop/playhead commands as needed

Acceptance criteria:

- Undo restores active scene, selected actors, playhead, loop region, timeline zoom/scroll, and tool mode for document-affecting edits.
- Drag edits produce one undo entry.
- Undo depth is configurable in settings/persistence.

### Phase 6: Replace `WorkspaceBehavior` Mutation Contexts

Introduce panel view models and gradually change panels to accept immutable models + command bus.

Start with panels that already mostly emit commands:

1. Timeline panel
2. Preview panel
3. Inspector
4. Sidebar
5. Editor tab

Acceptance criteria:

- `WorkspaceBehavior` no longer passes `&mut DocumentStore` to panels.
- Panels cannot mutate source, timeline, composition, or derived caches.
- All mutations go through shell/controller dispatch.

### Phase 7: Runtime Service Extraction

Move WGPU/eframe-specific render handling behind `PreviewRenderer`. Move audio behind `AudioPreviewEngine`.

Acceptance criteria:

- `GuiShell` can be tested without constructing eframe WGPU state.
- Render surface can be swapped for a mock in unit tests.
- Screenshot/export paths can share render request types where appropriate.

## Testing Strategy

### Unit Tests

Add tests for:

- `active_timeline()` single-scene fallback.
- `active_timeline()` composition active-scene resolution.
- Invalid active scene falls back to entry/first scene.
- Source epoch increments on all source mutation paths.
- Rebuild response rejection for stale source epochs.
- History undo/redo restores `UiSnapshot`.
- Export target resolution for timeline, active scene, and whole composition.

### Integration Tests

Use small `.amx` fixtures:

- Single-scene document with actor/keyframes.
- Composition with two scenes and transitions.
- Composition with duplicate actor labels across scenes.
- Invalid source after a valid source.

Scenarios:

- Edit property in active composition scene and verify source diff targets the correct scene.
- Drag a locked actor in a composition scene and verify drag is rejected.
- Move a keyframe in a composition scene and verify the correct scene source changes.
- Export whole composition and active scene.
- Undo a canvas drag and verify source, active scene, selection, playhead, and zoom restore.

### Performance Tests

Add instrumentation first, then optimize:

```rust
pub struct RebuildTimings {
    pub parse_ms: f32,
    pub module_ms: f32,
    pub typecheck_ms: f32,
    pub build_ms: f32,
    pub cache_ms: f32,
    pub total_ms: f32,
}
```

Expose this in a performance HUD before doing deeper optimization.

## Architectural End State

The end state is a GUI where every frame follows one predictable transaction:

1. Poll background document/render/export work.
2. Publish immutable derived snapshots with generation tokens.
3. Build panel view models from source, document, UI, and preview stores.
4. Render panels as pure views.
5. Dispatch typed commands.
6. Mutate canonical source/UI state through controllers.
7. Schedule rebuild/render/export side effects.
8. Persist layout/settings as needed.

This gives Animatix a maintainable base for larger animation features: real dope sheet editing, storyboard composition workflows, property spreadsheets, source-diff previews, component authoring tools, and layout debugging overlays. Most importantly, it fixes the trust problem: the preview, inspector, timeline, drag handlers, and export dialog all operate on the same active scene and the same versioned document snapshot.
