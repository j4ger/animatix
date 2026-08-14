# Presenterm-Inspired Design Roadmap

Evaluation date: 2026-08-12.

This document records the design lessons taken from
[mfontanini/presenterm](https://github.com/mfontanini/presenterm) and how they
map onto Animatix. The short canonical list of remaining work is
`docs/roadmap.md`; this file holds the detailed evaluation and sequencing.

We do not copy presenterm's TUI architecture or its markdown-specific choices.
We borrow six design patterns and keep the comment-directive question (item 7)
open for a separate product decision.

## Decision Summary

| ID | Pattern | Current Animatix State | Benefit | Feasibility | Necessity | Decision |
|----|---------|------------------------|---------|-------------|-----------|----------|
| P1 | Render operation / overlay IR | `RenderCommand` exists; GUI overlays now generate `PreviewOverlayOp`, and `Timeline` exposes `evaluate_program_with_debug` | Unifies preview/export/offscreen paths and makes overlays testable | Medium; do not rewrite scene_eval in one pass | Medium-high | Done |
| P2 | Hot-reload diff and UI state preservation | `RebuildWorker` is async; rebuild output is full state; playhead is clamped but not diffed, selection/scene survival is incidental | Editing does not disturb playback/scene/selection; removed actors become actionable diagnostics | High | High | Schedule first |
| P3 | Command-driven app state | `ShellAction`, `Command`, handlers, `Effect`, `ShortcutRegistry`, and command palette already exist | Remaining value is configurable keybindings and external command input, not a rewrite | High | Low-medium | Schedule only convergence gaps |
| P4 | Raw theme + inheritance + resolved runtime theme | eparts has `ThemeFile`, partial overrides, and `ThemeWatcher`; no `extends`, no named palette resolution | Reusable theme bases, dependency checks, hot reload through a base chain | High | Medium | Schedule as eparts follow-up |
| P5 | Unified asset/resource store | `AssetCache` caches SVG/image/glyph entries by string key; no usage tracking or targeted invalidation | Target reloads, inspector usage, cache invalidation | Medium | Medium | Schedule after P2 |
| P6 | Pollable async render pattern | `RebuildWorker` and streaming export already provide stronger async patterns | Useful only if file-backed asset loading becomes non-blocking | High if scoped to P5 | Low | Closed by design |

## P1: Render Operation / Overlay IR

### Current State

- `crates/animatix/src/primitives/mod.rs` already defines `RenderCommand`
  (`Paths`, `Text`, `Image`, `HighlightLayer`) and each command can execute into
  a Vello scene.
- `Timeline::evaluate_with_debug` in
  `crates/animatix/src/timeline/scene_eval.rs` returns `vello::Scene` directly
  and caches that scene.
- GUI overlays in `crates/animatix-gui/src/app/panels/preview_panel.rs` and
  `crates/animatix-gui/src/app/preview/overlay.rs` draw selection, hover,
  motion paths, scene bounds, grid, guides, and layout debug directly through
  egui painters.

### Why It Is Valuable

- Preview interaction overlays are currently hard to unit test without egui and
  hard to replay outside the GUI.
- Export, offscreen rendering, CLI preview, and GUI preview each need to agree
  on what a frame is; an observable scene program gives them one contract.
- A structured command stream lets future backends or screenshot harnesses
  render the same frame deterministically.

### Feasibility And Risk

- Medium. The primitive command layer already exists, so this is mostly about
  introducing a scene-level IR and moving direct Vello writes behind an
  executor.
- The main risks are the frame cache, Filter/offscreen sub-scenes, and
  transition compositing. Those must keep using their current GPU resources.
- Do not attempt to convert `scene_eval` to a fully retained render graph in
  one pass.

### Plan

Phase 1: GUI overlay IR (small, no runtime refactor)

- Add `PreviewOverlayOp` with scene-coordinate primitives: rect, line, label,
  path, icon, snap line.
- Convert existing preview overlays to builders that produce `PreviewOverlayOp`
  lists.
- Convert `preview_panel.rs` into an executor over those operations.
- Add unit tests for selection, hover, reorder ghost/drop line, guides, and
  motion paths that do not require wgpu.

Phase 2: Observable scene program (runtime)

- Introduce `SceneProgram { items, overlays, precise_bounds, diagnostics }`.
- Add `SceneItem { transform, opacity, commands }`.
- Add `Timeline::evaluate_program_with_debug`; keep `evaluate_with_debug` as a
  thin Vello executor wrapper so CLI/export/GUI do not need to change at once.
- Store the program in the frame cache, not only the encoded scene.
- Add golden-program tests for representative shapes, text, plots, containers,
  and filters.

Phase 3: Structured scene boundaries (only if needed)

- Add group/layer commands or command-level filter markers only when a concrete
  backend needs them.
- Verify GUI preview, offscreen export, and transition compositing consume the
  same program for ordinary frames.

### Acceptance Criteria

- Overlay behavior tests pass without a GPU.
- Normal preview and export paths render unchanged.
- Debug overlays can be generated by tests from the same operations the GUI
  draws.

### Status (2026-08-12)

Phase 1 implemented. `crates/animatix-gui/src/app/preview/overlay_ops.rs`
defines `PreviewOverlayOp` and builders for selection, multi-selection, hover,
cycle indicator, motion paths, ghost, reorder, scene bounds, actor labels, grid,
snap guides, and layout debug. `preview_panel.rs` and `preview/context.rs`
generate ops and execute them through `execute_overlay_ops`. The overlay IR is
scene-coordinate based with explicit screen-space decoration variants; behavior
tests pass without a GPU. A few HUD/direct painter helpers (marquee, performance
HUD, vertex/callout handles) intentionally remain direct egui draws rather than
being converted into the shared IR.

Phase 2 implemented. `crates/animatix/src/timeline/scene_program.rs` defines
`SceneProgram`/`SceneItem`; `Timeline` gained `evaluate_program_with_debug`, and
frame cache entries now store the program. Primitive actors are collected as
observable `SceneItem`s, while the authoritative encoded scene remains the exact
render target for filters, masks, static subtrees, and legacy paths. GUI preview,
offscreen rendering, and transition compositing consume the same scene API. A
later architecture review removed the prematurely exposed
`SceneProgramOp`/`execute_into` surface because no production path emitted those
ops; the structured op layer should be reintroduced only when a concrete backend
needs it. The two public evaluate paths now share one cache-aware program
evaluation helper, and item collection is part of the cache key so scene-only
calls cannot poison later program requests.

## P2: Hot-Reload Diff And UI State Preservation

### Current State

- `crates/animatix-gui/src/app/document/rebuild.rs` already runs rebuilds on a
  background thread with cancellation and token ordering.
- `RebuildOutput` carries the full rebuilt AST, timeline/composition,
  diagnostics, duration, dimensions, and timeline index.
- After a rebuild, `sync_preview_from_document` clamps the current time, but
  there is no change-aware policy for duration changes, active scene, or
  selection.

### Why It Is Valuable

- Editing an `.amx` while scrubbing should not disrupt the current view.
- If a scene, actor, or keyframe disappears, the GUI should say why instead of
  silently dropping the selection.
- A diff also feeds the archived "source-diff preview sidecar" idea later.

### Feasibility And Risk

- High. The rebuild pipeline is already healthy; we only add a diff summary and
  apply a policy on acceptance.
- The diff should be computed from old/new AST and timeline index rather than
  textual diff to keep renames and structural changes meaningful.
- Preserve current time exactly when the timeline still covers it; otherwise
  clamp to the new duration or jump to the nearest surviving keyframe.

### Plan

1. Add `TimelineDiff` with `added_actors`, `removed_actors`,
   `renamed_actors`, `added_scenes`, `removed_scenes`, `duration_ms_delta`,
   and `keyframe_line_changes`.
2. Compute `TimelineDiff` during rebuild acceptance from the previous snapshot
   and the new `RebuildOutput`.
3. Keep current playback time when the new duration still contains it. If it no
   longer fits, clamp to duration or move to the nearest surviving keyframe.
4. Keep the active scene if it still exists; otherwise fall back to the first
   scene and emit a status/toast.
5. Keep selected actors that still exist. For removed actors, clear the
   selection and surface one diagnostic/status line naming the removed label.
6. Add handler tests for duration shrink/grow, scene removal, actor removal,
   and no-op edits.

### Acceptance Criteria

- Hot reload of unrelated edits preserves time, scene, and selection.
- A change that removes the current actor reports it instead of silently
  dropping it.
- `TimelineDiff` tests cover single-scene and multi-scene documents.

### Status (2026-08-12)

Implemented. `TimelineDiff`/`TimelineFingerprint` live in
`crates/animatix-gui/src/app/document/timeline_diff.rs`. Both synchronous rebuild
and background rebuild acceptance capture the prior compiled target and view
state, then preserve the playhead when the new duration still covers it, move to
the nearest surviving keyframe otherwise, keep the active scene and selections
when they survive, and report removed scenes/actors in the preview status. The
keyframe identity is `(actor, property, time_ms)`, so a property removed at the
same time as another surviving property no longer keeps the stale selection;
removing the active scene clears keyframe selections because the current UI
selection is not scene-qualified. Unit and handler tests cover single-scene,
multi-scene, duration shrink, removed property keyframes, and removed
selection/scene cases.

## P3: Command Layer Convergence

### Current State

- Animatix already has a command-driven shell: `ShellAction`, domain
  `Command`, handlers, `Effect`, `ShortcutRegistry`, command palette, and
  undo/redo labels live in `crates/animatix-gui/src/app/commands/` and
  `crates/animatix-gui/src/app/shell/`.
- `PlaybackController` is already stateful and unit-tested.
- Presenterm's command model does not need to be adopted as an architecture.

### Why It Is Still Worth A Small Track

- `ShortcutRegistry` is hardcoded in
  `crates/animatix-gui/src/app/interaction/keyboard.rs`.
- There is no external command source, so integration tests and future remote
  control must drive egui events.
- A user-configurable keymap plus a command bus gives most of presenterm's
  benefit without changing the existing architecture.

### Feasibility And Risk

- High. This is incremental work on an already well-separated layer.
- Low risk if we do not rename existing command types or rewrite handlers.

### Plan

1. Move shortcut definitions into a persisted settings table.
2. Add conflict detection for multi-key and `<number>` style bindings, modeled
   on presenterm's key matcher.
3. Update command palette and shortcut cheat sheet to display the current
   bindings.
4. Add an `ExternalCommand` queue or test-only sender that pushes `ShellAction`
   into the existing pending action queue.
5. Keep `PlaybackController` as is; add tests only for new keymap and external
   command surfaces.

### Acceptance Criteria

- Shortcuts can be rebound from settings and conflicts are rejected.
- Command palette and cheat sheet reflect active bindings.
- Integration tests can drive commands without simulating raw egui input.

### Status (2026-08-12)

Implemented configurable keybindings and persistence. `SavedShortcut` provides a
stable serialized shortcut representation, `ShortcutRegistry::with_overrides`
applies overrides by stable binding name and rejects unknown bindings/conflicts,
and `GuiShell` owns the active registry, initialized from persisted settings at
startup. The Settings dialog can record a new key for each binding; accepted
changes replace the shell-owned registry and are saved to workspace persistence.
The cheat sheet and toolbar read the registry through shell-owned state, so they
reflect active bindings automatically. The previous process-wide
`SHORTCUT_REGISTRY` static was removed to keep tests isolated and avoid global
mutable state. An external command queue is intentionally not added yet because
there is no concrete integration consumer; the existing `pending_actions` queue
already provides that seam.

## P4: Theme Inheritance And Resolved Runtime Theme

### Current State

- `crates/eparts/src/tokens/theme_json.rs` already has `ThemeFile` and
  `PartialTheme`, so JSON themes are override files against a base.
- `crates/eparts/src/tokens/theme_watcher.rs` already hot-reloads one theme
  file.
- There is no `extends`, no named palette/class references, and no dependency
  graph/registry.

### Why It Is Valuable

- A project can define small theme deltas instead of full theme files.
- Inherited themes can be validated for missing bases and cycles at load time.
- Hot reload can watch the whole dependency closure instead of only the leaf
  file.

### Feasibility And Risk

- High. The partial override mechanics already exist.
- The main design decision is whether theme inheritance belongs only in eparts
  or also in `.amx` config. Start with eparts and do not expose a DSL theme
  surface until there is a concrete user story.

### Plan

1. Add `extends: Option<String>` to `ThemeFile`.
2. Add a theme registry that loads a directory, builds a dependency graph, and
   reports missing base, duplicate name, and extension loop errors.
3. Introduce `RawTheme` / `ResolvedTheme` separation if named colors or classes
   are added; resolve raw values into the existing runtime `Theme`.
4. Extend `ThemeWatcher` to watch every file in the dependency closure.
5. Add tests for inheritance, missing bases, cycles, and base-file hot reload.

### Acceptance Criteria

- A child theme can inherit from a base theme and override only selected slots.
- Invalid inheritance is rejected with a clear error.
- Editing a base theme reloads all dependent themes.

### Status (2026-08-12)

Implemented the eparts framework foundation. `ThemeFile` now supports
`extends`, `ThemeRegistry` loads a directory, resolves the full inheritance
chain, and rejects duplicate names, missing bases, and extension loops.
`ThemeRegistryWatcher` reloads the whole registry when any file in the theme
directory changes, so editing a base theme refreshes dependents. Schema now
accepts `extends`. GUI integration is intentionally deferred: the GUI does not
enable `theme-json` or own a theme directory/name selector yet, so this remains
a framework capability until a concrete user story exists. No `.amx` DSL theme
surface was added.

## P5: Unified Asset Store And Usage Tracking

### Current State

- `crates/animatix/src/timeline/assets.rs` exposes `AssetCache`, which caches
  SVG paths, images, and text glyphs by string key.
- Image, SVG, text, and audio loading live in separate paths with separate
  invalidation behavior.
- The archived "Asset usage tracking" idea has no current consumer.

### Why It Is Valuable

- A normalized asset identity makes hot reload target only files that changed.
- Usage tracking lets the inspector show which actors reference an image/SVG.
- Cache invalidation can be per-asset instead of `clear()`.

### Feasibility And Risk

- Medium. The loaders are already centralized enough to expose a single store,
  but text glyph compilation and audio decode have different caching
  requirements.
- Do not block rendering on network assets; keep deterministic behavior by
  showing last-good content until a reloaded asset is ready.

### Plan

1. Add `AssetStore` keyed by normalized path and content hash.
2. Register loaders for image, SVG, text/glyph, and audio.
3. Expose `Timeline::assets()` and per-actor usage collected at build/eval.
4. Let the GUI watcher invalidate only changed asset IDs and schedule a
   targeted rebuild.
5. Re-open "Asset usage tracking" once the store is available and show actor
   references in the inspector.

### Acceptance Criteria

- Asset cache invalidation is scoped to the changed path.
- Rebuilds preserve unaffected asset entries.
- Usage tracking tests cover images, SVGs, and audio.

### Status (2026-08-12)

Implemented the runtime/tooling foundation. `AssetCache` now exposes
`load_svg_for`/`load_image_for`/`record_usage`, `asset_usage`, and
`assets_for(actor)`. Image, SVG, and audio declaration/assignment paths record
the real actor label through the shared cache, and `Timeline::asset_usage`
exposes the map. The inspector shows referenced asset paths for the selected
actor. Text glyph compilation and GUI audio remain separate caches by design
because their keys and eviction policies differ. An architecture review removed
the never-used `invalidate_asset`/`get_or_load_*`/`clear` surface: the GUI
rebuilds a fresh `Timeline`/`AssetCache`, so usage is naturally re-derived from
the current source instead of being manually invalidated. If a future asset hot
reload needs cache survival, it should explicitly carry `Arc<AssetCache>` across
rebuilds and then add a real invalidation contract.

## P6: Async Loading Pattern (Closed By Design)

### Current State

- Presenterm models dynamic content as `RenderAsync` / `Pollable`.
- Animatix already has a background `RebuildWorker`, streaming parallel export,
  progress, and cancellation.

### Why It Is Closed

- The general `Pollable` pattern is unnecessary while rendering is synchronous.
- The only concrete future need is non-blocking file-backed asset loading, and
  that belongs inside the P5 `AssetStore`, not in the renderer.
- Introducing async render operations now would complicate the frame cache and
  random-access timeline guarantee without a current user-facing benefit.
- P5 is implemented and provides `invalidate_asset` plus deterministic
  last-good content; it is the documented seam for any future async loading.

### Plan (if reopened)

- Do not add a renderer `Pollable` trait.
- Add optional async load handles only for file-backed assets, with
  deterministic last-good fallback while a load is in flight.
- Revisit only if a real user story requires progressive asset loading or
  long-running per-frame work.

### Acceptance Criteria (future)

- Missing/slow assets do not stall the UI thread.
- Evaluation remains deterministic for a given cache state.

### Status (2026-08-12)

Closed as a design decision. No open implementation remains.

## Item 7: Comment Directives Through DSL (Open Discussion)

Presenterm uses `<!-- ... -->` directives because markdown already has a parser
and it avoids building a second DSL. That tradeoff does not apply to Animatix,
which owns a semantic `.amx` DSL, two parsers, and round-trippable source.
Copying comment directives would create a parallel, weakly typed syntax layer.

The valuable presenterm commands should instead be mapped to native `.amx`
features. The initial mapping:

| presenterm command | native Animatix equivalent |
|--------------------|----------------------------|
| `include` | `import` |
| `end_slide` | scene declarations |
| slide title / headings | scene metadata or title actors |
| layout columns | Row/Col/Grid/Stack containers |
| LaTeX / typst | Typst primitive |
| theme | `config { colorscheme: ... }` and eparts theme files |
| pause / incremental reveal | keyframes, actions, and stagger |
| speaker notes | no native equivalent; candidate `notes` metadata |
| code execution | no native equivalent; candidate `run`/`exec` modifier or CLI subcommand |
| export control | CLI flags or `config` metadata, not source comments |

Candidate extensions worth discussing, in order of product value:

1. **Speaker notes**: scene/actor metadata consumed by the GUI and a future
   presentation/export mode. This is the strongest candidate because it is
   declarative and does not affect rendering.
2. **Export presets**: `config`-level metadata for preferred resolution/fps/
   codec, so the CLI and GUI export dialog share defaults.
3. **Code execution**: only if Animatix becomes an interactive teaching tool;
   otherwise it fights the deterministic timeline model.
4. **Pause/chunking**: do not add a comment command. The timeline already
   expresses reveal state; a presentation mode should derive "steps" from
   keyframes/actions, not from a second control channel.

Recommendation: do not add comment directives. If speaker notes or export
presets get a concrete user story, implement them as first-class DSL metadata
so parser, analyzer, GUI, CLI, and tokenizer stay in sync.
