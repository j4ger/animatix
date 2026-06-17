# Phase 3: Command System Split — Implementation Plan

Status: implementation-ready plan for Phase 3 (PLAN.md steps 12–14).
Baseline: commit `71bdb2` (Phase 1 tokens + Phase 2 Button/TextRole done).

## Goal

Split the flat 61-variant `Command` enum in `crates/animatix-gui/src/app/commands.rs`
into six domain command packages, separate undoable document mutations from
non-undoable view/playback actions at the type level, and migrate all panel
emission sites to domain command constructors — while keeping `cargo check` and
`cargo test -p animatix-gui` green after every step.

## Key architectural finding (drives the whole design)

**Undo/redo never replays the stored `Command`.** `handle_undo`/`handle_redo` in
`handlers/ui.rs` restore `entry.source_before` / `entry.source_after` via
`document_store.replace_text(...)` and restore a `UiSnapshot`. The `command`
field stored in `UndoEntry` / `PendingSnapshot` is **purely a diagnostic label**.

Consequences:
- The `Command` stored for undo can be replaced by a narrower `UndoLabel` type
  with zero behavior change.
- "Undo stack accepts only undoable document commands" (Step 13) is achieved by
  tightening `DocumentStore::snapshot()`'s signature to take `UndoLabel`, so
  view/playback commands cannot be passed to it. The runtime bypass is already
  structural (only mutation handlers call `snapshot()`).
- Exactly **24** `snapshot(Command::…)` call sites exist today (verified by grep).
  Non-snapshotting source mutations (`ReorderScenes`, `ToggleActorVisibility`,
  `ToggleActorLock`, `SelectScene`) are **pre-existing undo gaps, out of scope
  for Phase 3** — flagged below as a follow-up.

## Two-axis design

Phase 3 introduces two orthogonal type axes:

1. **Domain package** (Step 12): `DocumentCommand`, `ActorCommand`,
   `KeyframeCommand`, `SceneCommand`, `PlaybackCommand`, `ViewCommand`. These
   are *additive parallel enums*; the old `Command` enum is kept intact as the
   union/wrapper type that `ShellAction::Command(Command)` still holds.
2. **Undoability** (Step 13): `UndoLabel` — a flat enum of exactly the 24
   undoable operations, used only as the `snapshot()` label type.

The old `Command` enum and `shell/mod.rs` dispatch are **untouched through
Phase 3**. Domain commands enter the system via `From<DomainCommand> for
Command` (and `From<DomainCommand> for ShellAction`). `Command` is deleted in a
later phase once `ShellAction` is changed to hold a domain sum type directly.

---

## 1. Domain packages

Six packages, aligned to the existing `handlers/` modules where possible. The
one forced exception is `handlers/property.rs`, which splits into
`SceneCommand` (scene-structure edits) and `DocumentCommand` (`PropertyEdit`).

| Package | Maps to handler module | Variants |
|---|---|---|
| `DocumentCommand` (11) | `file.rs` + `PropertyEdit` from `property.rs` | `OpenFile`, `Save`, `Reload`, `Rebuild`, `SwitchWorkspace`, `ToggleExpandDir`, `Undo`, `Redo`, `InsertionFromPalette`, `FindReplaceAll`, `PropertyEdit` |
| `ActorCommand` (15) | `actor.rs` | `CreateActor`, `RenameActor`, `DuplicateActor`, `DuplicateSelectedActors`, `DeleteSelectedActors`, `ReparentActor`, `ExtractScene`, `MoveToScene`, `ToggleActorVisibility`, `ToggleActorLock`, `PasteActors`, `AlignActors`, `DistributeActors`, `GroupSelectedActors`, `UngroupSelectedActors` |
| `KeyframeCommand` (4) | `keyframe.rs` | `SetKeyframeEasing`, `DeleteKeyframe`, `MoveKeyframe`, `ResizeAction` |
| `SceneCommand` (7) | `scene.rs` + scene-structure from `property.rs` | `SelectScene`, `ReorderScenes`, `DuplicateScene`, `DeleteScene`, `SetTransition`, `SetPlayTarget`, `SetSceneDuration` |
| `PlaybackCommand` (8) | `playback.rs` | `TogglePlayback`, `ScrubTo`, `PrevKeyframe`, `NextKeyframe`, `FrameStepForward`, `FrameStepBackward`, `ToggleEditorSync`, `EditorChanged` |
| `ViewCommand` (16) | `ui.rs` | `ScrollToLine`, `ZoomToSelection`, `ZoomToAll`, `SetTimelineZoom`, `SetTimelineScroll`, `SetLoopRegion`, `ToggleCollapseActor`, `TogglePropertyLane`, `SetPreviewZoom`, `SetPreviewZoomCentered`, `SetPreviewPan`, `SetToolMode`, `SetSidebarTab`, `SetPropertyViewMode`, `SetKeyframeViewMode`, `SetPivotOffset` |

Total: 11 + 15 + 4 + 7 + 8 + 16 = **61** — matches the current `Command` enum
exactly.

Notes:
- `EditorChanged` / `ToggleEditorSync` stay in `PlaybackCommand` to keep
  package ↔ `handlers/playback.rs` aligned. `EditorChanged` is semantically a
  document-sync event; relocating its *handler* is out of scope (flagged
  follow-up).
- `PropertyEdit` / `PropertyValue` structs stay defined in `commands/mod.rs`
  (shared types, re-exported via `panels/mod.rs`). Only the
  `Command::PropertyEdit` *variant* is paralleled by
  `DocumentCommand::PropertyEdit`.

## 2. Which `Command` variants go where

See the table above. `UndoLabel` (Step 13) takes the **24 undoable** variants:
all 12 snapshotting actor commands, all 4 keyframe commands, the 5 snapshotting
scene commands (`DuplicateScene`, `DeleteScene`, `SetTransition`,
`SetPlayTarget`, `SetSceneDuration`), and 3 from document (`PropertyEdit`,
`InsertionFromPalette`, `FindReplaceAll`). Non-undoable variants
(`SelectScene`, `ReorderScenes`, `ToggleActorVisibility`, `ToggleActorLock`,
all playback, all view, all file-lifecycle, `Undo`/`Redo`) are **not** in
`UndoLabel`.

## 3. Compatibility `From` conversion

Direction: **`From<DomainCommand> for Command`** (domain → union wrapper).
Each domain variant maps 1:1 to the existing `Command` variant. Example
(`commands/actor.rs`):

```rust
#[derive(Debug, Clone)]
pub enum ActorCommand {
    CreateActor { ty: String, label: String, position: [f32; 2], props: Vec<animatix_syntax::ast::Property> },
    RenameActor { old_label: String, new_label: String },
    // …
}

impl From<ActorCommand> for super::Command {
    fn from(c: ActorCommand) -> Self {
        match c {
            ActorCommand::CreateActor { ty, label, position, props } =>
                super::Command::CreateActor { ty, label, position, props },
            ActorCommand::RenameActor { old_label, new_label } =>
                super::Command::RenameActor { old_label, new_label },
            // …
        }
    }
}
```

Additionally, **`From<DomainCommand> for ShellAction`** in `commands/mod.rs`,
delegating to `Command`:

```rust
impl From<ActorCommand> for ShellAction {
    fn from(c: ActorCommand) -> Self { ShellAction::Command(c.into()) }
}
// … one per domain package
```

This lets migrated call sites write
`pending_actions.push_back(ActorCommand::CreateActor { … }.into())` and
`bus.emit(ActorCommand::CreateActor { … })` (since `CommandBus::emit` takes
`impl Into<ShellAction>`).

`Command` keeps **all 61 variants** through Phase 3; nothing is removed, so
`shell/mod.rs::handle_command`'s exhaustive `match` never breaks. Domain enums
are additive. For Step 13, `From<UndoLabel> for Command` is also provided so a
label can be logged/dispatched if needed (handlers will pass `UndoLabel`
directly to `snapshot()`).

## 4. Migration order (each step keeps `cargo check` + tests green)

### Step 12 — Split command modules (additive, no call-site churn)

**12a. Module move.** Convert `commands.rs` → `commands/mod.rs`:
- `mkdir crates/animatix-gui/src/app/commands/`
- Move the entire contents of `commands.rs` into `commands/mod.rs` unchanged.
- `app/mod.rs`'s `pub mod commands;` resolves to `commands/mod.rs`
  identically — **no edit needed**. Do this as a single `git mv` +
  recreate to avoid `commands.rs`/`commands/mod.rs` coexistence.
- Verify: `cargo check -p animatix-gui` (must be green, zero behavior change).

**12b. Add domain submodules.** Create `commands/{document,actor,keyframe,scene,playback,view}.rs`,
each with its `pub enum XCommand` (fields copied verbatim from the matching
`Command` variants) and `impl From<XCommand> for super::Command`. Wire
`pub mod document;` … `pub mod view;` + `pub use document::DocumentCommand;`
… re-exports in `commands/mod.rs`.
- All new types are `pub` and referenced by their `From` impls → **not dead
  code**, no `#[allow(dead_code)]` needed (satisfies AGENTS.md rule).
- Verify: `cargo check -p animatix-gui`. No call sites touched yet.

**12c. `From<XCommand> for ShellAction` impls** in `commands/mod.rs`
(delegating to `Command`). Verify: `cargo check -p animatix-gui`.

### Step 13 — Split undoable / non-undoable dispatch

**13a. Add `UndoLabel`** in `commands/mod.rs` (flat enum, 24 variants, fields
copied from the matching `Command` variants). Add `From<UndoLabel> for Command`.
`pub`, referenced by `From` → not dead. Verify: `cargo check`.

**13b. Tighten `snapshot()` typing** — one commit, signature + all 24 call
sites together:
- `stores/document_store.rs`: `PendingSnapshot.command: UndoLabel`;
  `fn snapshot(&mut self, label: UndoLabel)`.
- `stores/history_store.rs`: `UndoEntry.command: UndoLabel`.
- Replace `snapshot(Command::X{…})` → `snapshot(UndoLabel::X{…})` at all 24
  sites: `handlers/{actor,scene,keyframe,property}.rs`, `actions/mod.rs`,
  `shell/{find_replace,insertion_palette}.rs`. (Pure mechanical rename; the
  `UndoLabel::X` variant has the identical fields.)
- Verify: `cargo check -p animatix-gui` and
  `cargo test -p animatix-gui command_handlers` (undo/redo tests for property
  edit, actor create/delete, playback, panel toggles must still pass — they
  restore source text, not the label, so behavior is unchanged).
- Manual: undo/redo a property edit, actor create/delete, scene duplicate;
  confirm playback scrub and panel toggles do not push undo entries.

**13c. (Optional, cosmetic) Dispatch routing comments.** In `shell/mod.rs`,
group the `handle_command` match arms under section comments
`// ── Undoable document mutations ──` vs `// ── Transient (view/playback) ──`.
Do **not** restructure into separate fns unless desired — the bypass is already
enforced by `snapshot()`'s signature. Low priority; skip if it adds risk.

### Step 14 — Migrate panel emission to domain commands

Mechanical sweep replacing `ShellAction::Command(Command::X{…})` with
`ShellAction::Command(XCommand::X{…}.into())` (or just `XCommand::X{…}.into()`
where `From<XCommand> for ShellAction` applies). `shell/mod.rs` dispatch is
**not** changed. Commit per subgroup so each stays green:

**14a. Playback + Document emission** — `runtime.rs`, `shell/toolbar.rs`,
`shell/command_palette.rs`, `app/mod.rs` (keyboard shortcut block: `Undo`,
`Redo`, `Save`, `Reload`, `Rebuild`, `TogglePlayback`, `ScrubTo`,
`PrevKeyframe`, `NextKeyframe`, `DeleteSelectedActors`→`ActorCommand`,
`DuplicateActor`→`ActorCommand`, `ZoomToSelection`/`ZoomToAll`→`ViewCommand`,
`Group/UngroupSelectedActors`→`ActorCommand`, `PasteActors`→`ActorCommand`,
`ToggleEditorSync`→`PlaybackCommand`, `ScrollToLine`→`ViewCommand`,
`SelectScene`→`SceneCommand`). Also the `unsaved_changes.open(...)` re-entrancy
calls in `shell/mod.rs` (`OpenFile`, `SwitchWorkspace`, `Reload`) →
`DocumentCommand::*… .into()`.
- Verify: `cargo check -p animatix-gui`, `cargo test -p animatix-gui`.

**14b. Scene + Actor emission** — `panels/sidebar.rs`, `panels/inspector/mod.rs`,
`panels/preview_panel.rs`, `panels/timeline_panel.rs` (scene reorder/select +
actor create/reparent/visibility/lock/align/distribute/delete/duplicate).
- Verify: `cargo check`, `cargo test -p animatix-gui`.

**14c. Keyframe emission** — `panels/timeline_panel.rs` (keyframe
easing/delete/move/resize), `panels/inspector/keyframe_table.rs`,
`panels/inspector/property_groups.rs`, `preview/property_popup.rs`.
- Verify: `cargo check`, `cargo test -p animatix-gui`.

**14d. `PropertyEdit` / document emission** — `preview/drag_handler.rs`,
`preview/context.rs`, `preview/property_popup.rs`,
`panels/inspector/{property_groups,spreadsheet,mod}.rs`, `panels/editor.rs`.
Replace `Command::PropertyEdit(PropertyEdit{…})` →
`DocumentCommand::PropertyEdit(PropertyEdit{…}).into()`.
- Verify: `cargo check`, `cargo test -p animatix-gui`.

**14e. View emission** — `app/mod.rs` (`ScrollToLine`),
`panels/timeline_panel.rs` (timeline zoom/scroll/loop, transport may already
be PlaybackCommand from 14a), any remaining `Set*`/`Toggle*` panel-state
emissions. Replace remaining `Command::Set…`/`Command::Toggle…` (panel-state)
→ `ViewCommand::*… .into()`.
- Verify: `cargo check -p animatix-gui`, `cargo test -p animatix-gui`, and
  `rg "Command::" crates/animatix-gui/src/app crates/animatix-gui/src/cell_editor`.
  Expected: only `shell/mod.rs` dispatch arms (`Command::X =>`) and any
  not-yet-migrated `CommandBus`/`ShellAction` construction remain — **no** raw
  `Command::Variant{…}` constructors at emission sites.

Final: `cargo check`, `cargo test -p animatix-gui`, `cargo test -p animatix`,
then `cargo test --no-fail-fast` before commit. Manual smoke test
`cargo run -p animatix-gui -- examples/20_feature_reel.amx`: toolbar, sidebar
tabs, inspector edits, preview drag, timeline scrub, undo/redo, palettes.

## 5. How `ShellAction` interfaces with domain commands

`ShellAction::Command(Command)` remains the single dispatch envelope consumed
by `shell/mod.rs::handle_action`. Domain commands enter via `.into()`:

- Emission: `pending_actions.push_back(ActorCommand::CreateActor { … }.into())`
  — uses `From<ActorCommand> for ShellAction` (Step 12c), which delegates to
  `From<ActorCommand> for Command` then wraps in `ShellAction::Command`.
- `CommandBus::emit(actor_cmd)` works because `emit(impl Into<ShellAction>)`
  and `From<ActorCommand> for ShellAction` is implemented.
- Dispatch: `handle_command` still matches `Command::CreateActor { … }`. The
  domain command is normalized to `Command` at the `ShellAction` boundary, so
  the dispatcher is unaware of domain packages. This is what keeps Step 14
  non-breaking.

`CommandBus` itself stays `#[allow(dead_code)]` reserved infrastructure —
Phase 3 does **not** migrate panels onto `CommandBus` (panels still use
`ActionQueue` / `ui_store.pending_actions`). That wiring is a later phase.

## Files to touch

- `crates/animatix-gui/src/app/commands.rs` → **move** to `commands/mod.rs`.
- `crates/animatix-gui/src/app/commands/mod.rs` — moved content + `pub mod`
  declarations, `pub use` re-exports, `From<XCommand> for ShellAction` impls,
  `UndoLabel` enum + `From<UndoLabel> for Command`.
- `crates/animatix-gui/src/app/commands/{document,actor,keyframe,scene,playback,view}.rs`
  — new domain enums + `From<XCommand> for Command`.
- `crates/animatix-gui/src/app/stores/document_store.rs` —
  `PendingSnapshot.command: UndoLabel`, `snapshot(label: UndoLabel)`.
- `crates/animatix-gui/src/app/stores/history_store.rs` —
  `UndoEntry.command: UndoLabel`.
- `crates/animatix-gui/src/app/handlers/{actor,scene,keyframe,property}.rs` —
  `snapshot(Command::X)` → `snapshot(UndoLabel::X)` (Step 13b).
- `crates/animatix-gui/src/app/actions/mod.rs` —
  `snapshot(Command::PropertyEdit(...))` → `snapshot(UndoLabel::PropertyEdit(...))`.
- `crates/animatix-gui/src/app/shell/{find_replace,insertion_palette}.rs` —
  snapshot label swap (13b); insertion_palette emission unchanged (no
  `ShellAction` emission of `InsertionFromPalette`, it's snapshot-only).
- `crates/animatix-gui/src/app/shell/mod.rs` — `unsaved_changes.open(...)`
  re-entrancy `Command::OpenFile/SwitchWorkspace/Reload` →
  `DocumentCommand::*… .into()` (14a); optional dispatch comment grouping
  (13c). Dispatch `match` arms unchanged.
- `crates/animatix-gui/src/app/runtime.rs` — emission migration (14a).
- `crates/animatix-gui/src/app/mod.rs` — keyboard-shortcut emission migration
  (14a/14e).
- `crates/animatix-gui/src/app/shell/{toolbar,command_palette}.rs` — emission
  migration (14a).
- `crates/animatix-gui/src/app/panels/{sidebar,editor,preview_panel,timeline_panel}.rs`,
  `panels/inspector/{mod,property_groups,keyframe_table,spreadsheet}.rs` —
  emission migration (14b–14e).
- `crates/animatix-gui/src/app/preview/{drag_handler,context,property_popup}.rs`
  — emission migration (14c/14d).
- `crates/animatix-gui/src/app/command_bus.rs` — **no change** (stays reserved).

## Risks

- **Module rename collision.** `commands.rs` and `commands/mod.rs` cannot
  coexist. Do 12a as one move; `app/mod.rs`'s `pub mod commands;` needs no
  change. Verify with `cargo check` immediately after the move.
- **`PropertyEdit` / `PropertyValue` ownership.** These shared structs are
  re-exported via `panels/mod.rs`. Keep them in `commands/mod.rs`; only
  parallel the `Command::PropertyEdit` *variant* as
  `DocumentCommand::PropertyEdit`. Do not move the struct or its re-export.
- **`UndoLabel` duplication.** ~24 variants mirrored between `Command` and
  `UndoLabel`. Acceptable for the migration window; collapse after `Command`
  is removed in a later phase.
- **Pre-existing undo gaps (out of scope).** `ReorderScenes`,
  `ToggleActorVisibility`, `ToggleActorLock`, `SelectScene` mutate state but do
  not call `snapshot()`. Phase 3 preserves current behavior — they remain
  non-undoable. Flag as a separate follow-up; do not fix here (scope creep +
  behavior change risk).
- **`unsaved_changes` re-entrancy.** `UnsavedChangesDialog` stores a pending
  `ShellAction` and replays it. `OpenFile`/`SwitchWorkspace`/`Reload` must be
  in `DocumentCommand` so `.into()` produces the right `ShellAction`. Verified
  in the table above.
- **Exhaustive `match` on `Command`.** `shell/mod.rs::handle_command` and
  `command_palette.rs` build/match `Command`. We never add or remove `Command`
  variants in Phase 3, so these stay green. Domain enums and `UndoLabel` are
  separate types.
- **Test inspection of `UndoEntry.command`.** Changing its type `Command` →
  `UndoLabel` could break any test that matches on it. Current
  `command_handlers.rs` tests do not (verified by reading). Re-run
  `cargo test -p animatix-gui` after 13b.
- **`#[allow(dead_code)]` policy.** All new domain enums and `UndoLabel` are
  `pub` and referenced by `From` impls / `snapshot()` → not dead. Do not add
  bare `#[allow(dead_code)]` (AGENTS.md forbids uncommented allows). If any
  domain variant ends up temporarily unreferenced (it should not, since `From`
  covers all), add an inline justification comment or remove it.
- **Emission sweep size.** Step 14 touches ~15 files with ~90 `Command::`
  sites. Commit per subgroup (14a–14e) and run `cargo check` + `cargo test`
  after each. The `.into()` is type-driven, so a missed site is a hard compile
  error (fails loud, not silent).
- **`EditorChanged` placement.** Kept in `PlaybackCommand` (handler-aligned)
  though semantically a document-sync event. Documented; handler relocation is
  a follow-up. If reviewers prefer semantic placement, move to
  `DocumentCommand` — the `From` impl is the only edit.
- **`CommandBus` not wired.** Phase 3 does not move panels onto `CommandBus`.
  `From<XCommand> for ShellAction` is added so future `bus.emit(domain_cmd)`
  works, but panels continue using `pending_actions`. Do not partially wire
  `CommandBus` in this phase (mixed wiring risks lost actions).

## Verification summary

| Step | Command |
|---|---|
| 12a | `cargo check -p animatix-gui` |
| 12b/12c | `cargo check -p animatix-gui` |
| 13a | `cargo check -p animatix-gui` |
| 13b | `cargo check -p animatix-gui` && `cargo test -p animatix-gui command_handlers` |
| 14a–14e (each) | `cargo check -p animatix-gui` && `cargo test -p animatix-gui` |
| 14 done | `rg "Command::" crates/animatix-gui/src/app crates/animatix-gui/src/cell_editor` (only dispatch arms remain) |
| Final | `cargo check` && `cargo test --no-fail-fast` + manual smoke test of `examples/20_feature_reel.amx` |

## Commit sequence (suggested)

1. `refactor(gui): split commands.rs into commands/ module` (12a)
2. `refactor(gui): add domain command enums with From conversions` (12b/12c)
3. `refactor(gui): add UndoLabel and tighten snapshot typing` (13a/13b)
4. `refactor(gui): migrate playback/document command emission` (14a)
5. `refactor(gui): migrate scene/actor command emission` (14b)
6. `refactor(gui): migrate keyframe command emission` (14c)
7. `refactor(gui): migrate property/document-edit command emission` (14d)
8. `refactor(gui): migrate view command emission` (14e)

(Scopes per `cog.toml`: `gui`. Use `cog commit refactor "…" gui`.)
