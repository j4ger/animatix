# Scene Persistence (`persist` / `remove`) — Design & Implementation Plan

Status: Design + plan (not yet implemented). Roadmap item #1.

## Goal

Carry actors across `play` scene transitions. Opt-in `persist actor` marks an
actor to survive into the next scene; explicit `remove actor` drops it.
Persist-until-removed: a persisted actor survives multiple transitions until
explicitly removed.

---

## Part 1 — Syntax Specification

`persist` and `remove` are **action verbs** (not new statements). They reuse
the existing `Stmt::Action { verb, targets, args, modifiers }` AST node, so
they parse automatically via the generic `action` rule in
`crates/animatix-syntax/src/parser/stmt.rs` and serialize automatically via
`ToSource for Action` in `to_source.rs`. No new AST variant is needed.

### `persist`

```animatix
persist target [, target2, ...]
```

- Sets the actor's **persistence flag** to `true`. Pure metadata — writes **no
  keyframes**, causes no visual change at the keyframe where it appears.
- No duration. A bare time literal (`persist title [500ms]`) emits a warning
  (`PersistIgnoresDuration`) and the duration is ignored.
- May be placed at any keyframe. The flag is **sticky**: once set, the actor
  carries into the next scene and remains persistent there until `remove`.
- Targeting a non-existent actor → `UnsupportedActionTarget` (reuses existing
  `ensure_target_exists`).
- Targeting a layout-managed leaf child directly → `PersistLayoutManagedChild`
  warning in Phase 1 (suggest persisting the container). Lifted in Phase 3.

```animatix
# Intro
title: Text, text: "Welcome", at: (640, 360)
#1s
persist title
play Diagram [fade, 400ms]

# Diagram
// `title` is already here at (640, 360), visible, no re-declaration needed.
#0.5s
title.text = "Diagram" [600ms]   // animate the carried actor
```

### `remove`

```animatix
remove target [, target2, ...] [duration, ease: <easing>]
```

- Fades the target's `opacity` to 0 over `duration` (default **0 = instant**),
  identical to `fade-out`, **and** sets the persistence flag to `false` so the
  actor does not carry into the next scene.
- Implemented by composing the `FadeOut` logic + a flag flip (see
  `crates/animatix/src/timeline/actions/exit.rs::FadeOut`).
- On a never-persisted actor, behaves as `fade-out` (idempotent).
- `remove` on an already-removed/invisible actor is a no-op (flag stays false).

```animatix
# Diagram
#2s
remove title [500ms, ease: ease-in]   // fade out + stop persisting
play Outro
```

### Why action verbs (not statements)

- Consistency: `remove title [500ms]` is visually and semantically parallel to
  `fade-out title [500ms]`.
- Zero parser work: the generic action rule already accepts `verb targets
  [args] [modifiers]`.
- Zero serializer work: `ToSource for Action` handles round-tripping.
- LSP/analyzer completion comes for free once registered in `get_builtin_actions()`.
- They fit `BuiltinAction` trait dispatch in `actions/mod.rs::process_action`.

---

## Part 2 — Semantics

### 2.1 The persistence flag

Each scene's `Timeline` gains a field:

```rust
// crates/animatix/src/timeline/mod.rs — Timeline struct
pub(crate) persistence_flags: BTreeMap<String, bool>,
```

- `persist X` → `persistence_flags[X] = true` (at build time, in time order).
- `remove X` → `persistence_flags[X] = false` + opacity fade-out keyframes.
- **Inherited** carried actors are seeded with `persistence_flags[X] = true`
  at the start of the receiving scene's build, so they carry onward
  automatically (chain persistence). A subsequent `remove X` in that scene
  flips it back to false.
- Because the build processes keyframes in ascending time order, the final map
  state after build == the scene-end flag state. (persist-at-2s, remove-at-3s
  → false.)

### 2.2 When is state captured?

**At scene end.** The persistence flag flips at the keyframe where
`persist`/`remove` appear, but the *snapshot* of carried state is taken at the
scene's end time (`CompositionScene::duration_s`), sampling every property
track at that time. Rationale:

1. Transitions are scene-boundary events; you want the actor's final pose, not
   its pose at the `persist` keyframe.
2. Sampling beyond the last keyframe is well-defined (`PropertyTrack` clamps to
   the last value), so an explicit `duration` config extending past keyframes
   still yields a deterministic snapshot.

### 2.3 Build-time transplant (the core mechanism)

Scenes are built with isolated `Timeline`s and transitions are GPU texture
composites (`renderer/transition.rs::TransitionCompositor`) — there is no actor
state merging at render time. Persistence is therefore a **build-time track
transplant**:

1. Build scene A's `Timeline` (with carry bag from its predecessor injected, if
   any).
2. After A is built, compute A's **carry bag**: for every actor with
   `persistence_flags[label] == true`, snapshot its full `AnimationTrack` at
   `duration_s` into a single-keyframe (t=0) "seed" track. Snapshots are
   recursive (containers carry their subtree).
3. Inject the carry bag into scene B's build: pre-seed B's `Timeline.tracks`
   (and `root_nodes`, `container_metadata`) with the seed tracks **before**
   processing B's body.
4. Because the seed track already exists, B's `is_first_decl` check
   (`!self.tracks.contains_key(label)` in `build/actor.rs::process_actor_decl`)
   is **false** for a re-declared carried actor → it follows the **re-declaration
   morph** path. Carried actors not re-declared simply render at their carried
   state.

### 2.4 Re-declaration → morph (answers design Q3)

A persisted actor re-declared in the next scene **morphs** from its carried
state, exactly like intra-scene re-declaration. This falls out naturally from
the transplant: the seed track's `vector_paths`/`text_paths` are the morph
source. No special-casing.

```animatix
# Intro
btn: Rect, size: (100, 60), at: (640, 360)
persist btn
play Form [fade, 400ms]

# Form
#0s
btn: Rect, size: (140, 80), at: (640, 360) [600ms]   // morphs 100x60 → 140x80
```

### 2.5 Transition rendering (answers Q on fade double-images)

No change to the GPU compositor. Because the carried actor's t=0 state in B ==
A's scene-end state, during a `fade` transition the actor occupies the **same**
screen pixels in both the `from` (A near end) and `to` (B near start) textures.
The shader's `mix(from, to, alpha)` leaves that actor's pixels unchanged while
the rest of the scene cross-fades. This is the desired "hold steady while the
world cross-fades" behavior — for free. Verified reasoning against
`renderer/transition.rs` (cut = step at 0.5, fade = lerp by progress).

Caveat: if the user re-declares (morphs) the carried actor at B's `#0s`, the
morph is already in progress during the transition overlap, producing a mild
double-image for that actor. Acceptable and arguably correct; document it.

### 2.6 Colorscheme (answers Q4)

Carried color is **verbatim** — the seed track bakes the concrete RGBA sampled
at A's scene end. If scene B uses a different colorscheme, the carried actor
keeps A's color unless the user reassigns (`title.color = text.primary [400ms]`)
or re-declares with an explicit/scheme color. Auto-color slot index is carried
so `color: auto` stays consistent. No magic recoloring in MVP.

### 2.7 Layout (answers Q5)

- **Scene-anchored / absolute / scene-percent** actors (binding variants in
  `PositionBinding::SceneAnchor | ScenePercent | Absolute`): carried as-is —
  they resolve against scene dimensions, which are global, so they stay put.
- **Layout-managed** children (`PlacementMode::LayoutManaged`,
  `PositionBinding::ContainerDefault | ContainerPercent`): Phase 1 emits
  `PersistLayoutManagedChild` and refuses to carry the leaf (suggest persisting
  the container). Phase 3 re-roots them to `Absolute` at their resolved
  world-space position at scene end.
- **Persisting a container** (Row/Col/Grid/Stack/Group): carries the container
  **and its entire subtree** recursively. The container keeps its own binding
  (typically scene-anchored). Its children keep their layout-managed placement
  relative to the carried container (which exists in B), so internal layout
  continues to work.

### 2.8 Chain persistence (answers Q6)

Sticky flag travels with the carried actor. The carry bag records, per carried
actor, `persistent: true`. On injection into B, B's `persistence_flags[label]`
is seeded `true`. B → C therefore re-carries automatically with no re-persist.
`remove` in any scene breaks the chain from that scene onward.

### 2.9 Remove semantics (answers Q7)

`remove` is mid-scene-capable. It (a) flips the flag false and (b) writes
opacity fade-out keyframes (reuse `FadeOut::execute` logic). With duration: fade
over N ms then invisible. Without duration: instant opacity→0 at the keyframe.
The actor track remains in the scene graph at opacity 0 (truly pruning scene
graph nodes is unnecessary and risky for MVP).

---

## Part 3 — Implementation Plan

Phased. Each phase is independently testable and committable.

### Phase 0 — Plumbing (no behavior)

1. **`crates/animatix-syntax/src/diagnostics.rs`** — add `DiagnosticCode`
   variants + `Display` arms:
   - `PersistIgnoresDuration` (warning)
   - `PersistLayoutManagedChild` (warning)
   - `PersistTargetNotCarried` (warning — e.g. persist in a single-scene file)
   - `CarryAmbiguousPredecessor` (warning — multi-predecessor scene)
   - `RemoveNonExistent` is covered by existing `UnsupportedActionTarget`.
   Verification: `cargo test -p animatix-syntax`.

2. **Tree-sitter grammar** (`tree-sitter-animatix/`) — verify the action rule
   already matches `persist`/`remove verb target [mods]` (it should, since
   these are generic verb+target actions). Add fixtures
   `test/corpus/persist.amx` if a gap appears. Likely no grammar change.

### Phase 1 — Actions + flag + snapshot (single-scene-safe)

3. **New module `crates/animatix/src/timeline/persistence.rs`** — defines:
   ```rust
   pub struct CarryBag { pub entries: BTreeMap<String, CarryEntry> }
   pub struct CarryEntry {
       pub track: AnimationTrack,        // single-keyframe snapshot at t=0
       pub children: BTreeMap<String, CarryEntry>, // recursive subtree
       pub persistent: bool,             // sticky flag
   }
   pub fn snapshot_track_at(track: &AnimationTrack, time_ms: u64) -> AnimationTrack;
   ```
   `snapshot_track_at`: clone the track; for every property field reachable via
   the property registry (`PROPERTY_REGISTRY` + `allowed_property_indices(kind)`)
   collapse each `PropertyTrack<T>` to a single keyframe at `t=0` holding the
   value sampled at `time_ms` (use `TrackAccessor::get`/`last` per field). Reset
   `first_seen_ms = 0`. Carry `kind`, `procedural_plot`, `image`, `svg_paths`,
   `text.text_paths`, `shape.vector_paths` (sampled). Drop `child_orders`
   animations (collapse to current order). Position binding left as-is here;
   re-rooting happens at injection (Phase 3) for layout-managed.

4. **`crates/animatix/src/timeline/mod.rs`** — add `persistence_flags:
   BTreeMap<String, bool>` to `Timeline`; initialize empty in
   `new_with_font_context`.

5. **`crates/animatix/src/timeline/actions/persistence.rs`** (new) —
   `Persist` and `Remove` structs implementing `BuiltinAction` (template:
   `exit.rs::FadeOut`).
   - `Persist::execute`: validate targets via `ensure_target_exists`; for each
     target set `timeline.persistence_flags[target] = true`; if any timing
     modifier present → `PersistIgnoresDuration` warning.
   - `Remove::execute`: parse timing modifiers (reuse
     `parse_timing_modifiers` with `ModifierHost::Action`); for each target,
     run the `FadeOut` opacity logic (extract a shared helper from `exit.rs`),
     then set `timeline.persistence_flags[target] = false`.

6. **`crates/animatix/src/timeline/actions/mod.rs`** — register
   `Box::new(Persist)` and `Box::new(Remove)` in `get_builtin_actions()`. Add
   `pub mod persistence;`. Update `categorize_action` (add a `Persistence`
   `ActionCategory` or reuse `Effect`).

7. **Timeline carry-bag computation** — in `persistence.rs`:
   ```rust
   impl Timeline {
     pub fn compute_carry_bag(&self, scene_end_ms: u64) -> CarryBag;
   }
   ```
   Iterate `persistence_flags` where value is true; snapshot the actor + walk
   its `children` recursively (only persisting the subtree if the *root* is
   persisted — children inherit persistence from a persisted container). Skip
   layout-managed leaf roots with a `PersistLayoutManagedChild` warning
   (Phase 1 limitation). Set `persistent: true` on each entry.

   Verification: `cargo test -p animatix --lib persistence` with unit tests
   building a single-scene timeline, issuing `persist`, and asserting the carry
   bag contains a single-keyframe track.

### Phase 2 — Composition walk-order build + carry injection (the restructure)

8. **`crates/animatix/src/composition.rs::build_with_font_context`** —
   restructure into a walk-order build:
   - Pass 1: iterate statements, extract `Stmt::Scene` declarations and
     `play` targets (as today) **without building timelines**; also collect
     cross-file scenes from namespaces (existing logic). Compute `walk_order`
     via `compute_walk_order` (already exists; move its call earlier — it only
     needs `declaration_order` + `edges` + `scenes` map keys, not built
     timelines).
   - Pass 2: build timelines **in walk order**, threading a `CarryBag` from
     each scene to its successor. For the first scene, the bag is empty.
     Predecessor lookup: the scene earlier in `walk_order` whose `play` edge
     targets the current scene (or the immediately preceding scene in walk
     order). If a scene has ≥2 distinct predecessors → `CarryAmbiguousPredecessor`
     warning and carry only from the walk-order predecessor.
   - Keep duration/global-time computation (steps 3–4 of current code) after
     builds.

9. **Carry injection entry point** — add to
   `crates/animatix/src/timeline/build/entry.rs`:
   ```rust
   pub fn build_with_carry(
     ast, namespaces, font_context, build_quality, carry: &CarryBag
   ) -> BuildReport<Timeline>
   ```
   This is `build_with_diagnostics_and_font_context` plus, right after
   `Timeline::new_with_font_context`, a call to
   `timeline.inject_carry_bag(carry, &mut diagnostics)`.

10. **`Timeline::inject_carry_bag`** (in `persistence.rs`) — for each
    `CarryEntry`: insert its snapshot `track` into `self.tracks`, push the
    label to `self.root_nodes` (re-rooted), set
    `self.persistence_flags[label] = entry.persistent`, and recursively inject
    children (keeping parent/child links; carried containers also seed
    `self.container_metadata`). Convert any layout-managed/Container* binding
    to `Absolute` only in Phase 3; in Phase 2 assume carried roots are
    scene-anchored/absolute (enforced by Phase 1's layout-managed refusal).
    `Composition::build_with_font_context` passes the predecessor's carry bag
    here for each walk-order scene.

    Verification: `cargo test -p animatix composition` — extend
    `composition.rs` tests with a two-scene `persist` case asserting the
    carried actor's track exists in scene B's timeline at t=0 with the sampled
    position/color, and that B's duration/global time is correct.

### Phase 3 — Layout-managed child re-rooting

11. **`crates/animatix/src/timeline/scene_eval.rs`** — refactor
    `evaluate_node_transform` to expose a reusable
    `Timeline::resolve_actor_world_transform(label, time_ms, dims) ->
    kurbo::Affine` (extract from the DFS in `evaluate_node`; or compute by
    walking root→label applying layout positions via
    `compute_animated_layout` + `evaluate_node_transform`). Add
    `resolve_actor_world_position` returning `[f32;2]` (translation of the
    affine).

12. **`persistence.rs::snapshot_track_at` / `inject_carry_bag`** — when a
    carried actor's binding is `LayoutManaged`/`ContainerDefault`/
    `ContainerPercent`, call `resolve_actor_world_position` on the **source**
    scene's timeline at `scene_end_ms` (the carry-bag computation in step 7
    needs the source timeline + dims, so `compute_carry_bag` gains a `dims`
    param and a `&Timeline` self already available) and rewrite the snapshot
    track's binding to `Absolute` with that position. Remove the Phase 1
    `PersistLayoutManagedChild` refusal for direct persists; keep it only when
    re-rooting is impossible (e.g. deeply nested + dynamic_layout edge cases).

    Verification: test persisting a `Row` child → in scene B it appears at the
    same screen coords as scene-end of A, as a root-level absolute actor.

### Phase 4 — Edge cases, colorscheme, polish

13. **Colorscheme auto-color carry** — when snapshotting, copy
    `auto_color_assignments` slot for the label into the carry entry and
    re-seed on injection so `color: auto` stays consistent. (Step 3 already
    bakes the concrete color, so this is only needed if the actor is
    re-declared with `color: auto` in B — ensure the slot doesn't collide.)
    Add a test with two scenes using different colorschemes and a persisted
    `color: auto` actor.

14. **`persist` in a single-scene file** — `BuildTarget::SingleScene` path:
    `persist`/`remove` still record flags (harmless), but with no successor
    scene there is no carry. Emit `PersistTargetNotCarried` info/warning so
    users know persist is a no-op without a `play` successor.

15. **`remove` then `persist` in same scene** — flags apply in time order;
    final-state wins. If `persist` follows `remove`, the actor carries at
    whatever opacity it has at scene-end (likely 0). Emit a warning
    (`PersistAfterRemove`) recommending `fade-in`. Document.

16. **Persisting plot/graph/equation actors** — `procedural_plot` closures and
    `image`/`svg` handles are carried verbatim by `snapshot_track_at` (they
    live on the track). Add tests for persisting `PlotCurve` (carries closure
    + params), `Image`, `Svg`. Flag any kind whose snapshot is lossy as
    unsupported with a diagnostic if discovered.

17. **Cross-file scenes** (`play alias.Scene`) — ensure carry flows across
    cross-file edges in walk order. The cross-file scene's prelude is already
    merged at build; carry injection wraps that build. Test
    `examples/19_cross_file_scenes.amx` extended with `persist`.

### Phase 5 — Tooling & docs

18. **`crates/animatix-analyzer`** — add `persist`/`remove` to action
    completion (sourced from `get_action_signatures()`, which auto-includes
    them after step 6). Add hover docs. Verify target-existence diagnostics
    already cover them via `ensure_target_exists`.

19. **`tree-sitter-animatix`** — finalize any grammar fixtures (step 2).

20. **Docs** — `docs/spec.md` §18 (add `persist`/`remove` subsection),
    `docs/architecture.md` §16 (carry-bag mechanism), `docs/roadmap.md`
    (remove item #1), `examples/` (new `25_persistence.amx`). Update the LLM
    generation checklist in spec.md.

21. **GUI** (`crates/animatix-gui`) — `persist`/`remove` surface in the
    insertion palette automatically (action registry-driven). Composition
    timeline GUI is itself pending (roadmap Phase 4–6); persistence visualization
    can ride that later. Add `SourceEdit` support only if the palette needs a
    dedicated variant (likely not — actions insert via existing
    `InsertAction`).

### Files to touch (summary)

- `crates/animatix-syntax/src/diagnostics.rs` — new codes + Display.
- `crates/animatix/src/timeline/mod.rs` — `persistence_flags` field, module
  decl, re-exports.
- `crates/animatix/src/timeline/persistence.rs` — **new**: `CarryBag`,
  `CarryEntry`, `snapshot_track_at`, `compute_carry_bag`, `inject_carry_bag`.
- `crates/animatix/src/timeline/actions/persistence.rs` — **new**: `Persist`,
  `Remove` (`BuiltinAction` impls).
- `crates/animatix/src/timeline/actions/mod.rs` — register both; export module;
  category.
- `crates/animatix/src/timeline/actions/exit.rs` — extract shared fade-out
  helper used by `Remove`.
- `crates/animatix/src/timeline/build/entry.rs` — `build_with_carry`.
- `crates/animatix/src/timeline/scene_eval.rs` — refactor
  `evaluate_node_transform` → reusable `resolve_actor_world_transform`
  (Phase 3).
- `crates/animatix/src/composition.rs` — walk-order build + carry threading;
  extend tests.
- `crates/animatix/src/timeline/tests/` — new `persistence.rs` test module.
- `tree-sitter-animatix/` — fixtures/grammar (verify).
- `crates/animatix-analyzer/`, `docs/`, `examples/` — Phase 5.

---

## Part 4 — Edge Case Handling

| Case | Handling |
|------|----------|
| `persist` with no successor scene (single-scene, or last scene) | `PersistTargetNotCarried` warning; flag recorded but unused. |
| `persist` then `remove` same scene | Final flag false → not carried. `remove`'s fade-out still plays. |
| `remove` then `persist` same scene | Carries at scene-end opacity (likely 0). `PersistAfterRemove` warning. |
| `persist` on layout-managed leaf (Phase 1) | `PersistLayoutManagedChild` warning; not carried. Lifted Phase 3. |
| Persisted actor re-declared in next scene | Morphs (natural, via `is_first_decl=false`). |
| Persisted actor assigned in next scene | Animates from carried value (normal keyframe/assignment). |
| Colorscheme differs between scenes | Carried color verbatim; adapt via explicit reassign/re-declare. |
| Persisting a container | Carries container + full subtree; internal layout preserved. |
| Cross-file scene successor | Carry flows along walk-order edge; prelude merge unaffected. |
| Multi-predecessor scene (diamond play graph) | `CarryAmbiguousPredecessor`; carry from walk-order predecessor only. |
| `play` cycle | Already an error (`PlayCycleDetected`); carry not attempted. |
| `persist`/`remove` inside `sequence`/`stagger` | Supported (processed in time order like other actions). |
| `persist`/`remove` on a group/container target | Group expansion via existing `expand_group_targets` applies; flag set per leaf. |
| PlotCurve/Image/Svg persistence | Carried verbatim via track snapshot (closure/asset handles on track). |
| `remove` instant (no duration) | opacity→0 at keyframe; flag false. |
| Carried actor collides label with a new declaration in B | Re-declaration morphs (same label) — intended. Distinct labels are independent. |
| `dynamic_layout` in B with carried container | Container re-admits its carried children; layout recomputes per frame. |

---

## Part 5 — Testing Strategy

**Unit (per-phase):**
- Phase 1: `timeline/tests/persistence.rs` — build single-scene timeline,
  `persist X`, assert `persistence_flags[X]==true` and
  `compute_carry_bag` yields a single-keyframe snapshot matching the sampled
  position/color/size. `remove X [500ms]` asserts opacity keyframes + flag
  false. `persist X [500ms]` asserts `PersistIgnoresDuration`.
- Phase 2: `composition.rs` tests — two scenes, `persist` in A, assert B's
  timeline contains the carried track at t=0; assert B's `duration_s` and
  `global_duration_s` correct; assert re-declaration in B morphs (track has
  morph keyframes). Chain: three scenes, persist in first only, assert
  carried through second into third; `remove` in second stops carry to third.
- Phase 3: persist a `Row` child, assert B's carried actor is root-level
  `Absolute` at A's scene-end world position.
- Phase 4: colorscheme-change carry; cross-file carry; plot/image/svg carry.

**Integration / render:**
- Extend `run_render_smoke` (`main.rs`) path is already composition-aware;
  add an example `examples/25_persistence.amx` and export a video/GIF in CI
  smoke. Assert no double-image for a persisted actor during a `fade`
  transition by comparing the persisted actor's pixel block across the
  transition midpoint (approximate via a small image-diff test, or assert
  opacity stays ~1 in that region).

**Regression:**
- `cargo test -p animatix-syntax`, `cargo test -p animatix --lib`,
  `cargo test --no-fail-fast`, `cargo check --workspace`.
- Existing composition tests in `composition.rs` must stay green (the
  walk-order restructure must not change behavior for non-persist files).

**Diagnostics tests:**
- Each new `DiagnosticCode` has a test asserting it fires and is suppressed
  in the happy path (mirror existing `test_orphan_scene_warning` style).

---

## Part 6 — Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Walk-order restructure of `Composition::build`** breaks existing composition tests. | High — central path. | Keep the non-persist path behavior-identical; carry bag is empty for files without `persist`/`remove`, so injection is a no-op. Add a fast-path: if no scene contains persist/remove directives, skip carry entirely and use the original build loop. |
| `snapshot_track_at` misses a property field → carried actor renders wrong. | Med — visual glitch. | Enumerate fields via `PROPERTY_REGISTRY` + `allowed_property_indices(kind)` rather than hand-listing; add a "field coverage" test that snapshots every primitive in `PRIMITIVES` and re-renders a frame, diffing against the source scene-end frame. |
| Position re-rooting (Phase 3) computes wrong world transform. | Med — actor jumps. | Reuse `evaluate_node_transform` DFS rather than re-implementing; test against known layouts (Row/Col/Grid) with assertable coordinates. |
| Fade transition double-image when carried actor is morphed at B's `#0s`. | Low — cosmetic. | Document; recommend morphing at `#+0.2s` or later if a clean hold is wanted. |
| Carried `procedural_plot` closure captures stale env. | Med — plot wrong in B. | Closures capture `Arc` env base at build; verify `frame_env` in B includes carried plot params. Test persisting a `PlotCurve`. If broken, scope plots to a later phase with a diagnostic. |
| Multi-predecessor / branched play graphs carry ambiguously. | Low — rare. | `CarryAmbiguousPredecessor` warning; carry from walk-order predecessor. Document as unsupported topology. |
| GUI composition timeline is itself pending (roadmap P4–6). | Low — feature still usable via CLI. | Persistence works end-to-end in CLI export/preview now; GUI visualization deferred. |
| `remove` leaving opacity-0 tracks in the scene graph inflates render cost. | Low. | Negligible for typical scene counts; can prune zero-opacity-no-keyframe-beyond tracks later if profiled. |
| Tree-sitter grammar doesn't highlight `persist`/`remove`. | Low. | Verify in Phase 0; grammar's action rule should already cover them. |
| Performance: rebuilding carried scenes in walk order vs. parallel. | Low. | Composition build is already serial per scene; carry adds one snapshot pass per scene. Parallel export (`render_video_composition`) is unaffected (it operates on already-built timelines). |

---

## Open Questions (to confirm before implementation)

1. **Should `persist` accept a list of targets** (`persist a, b, c`)? — *Proposed
   yes*, consistent with `swap a, b`. The action parser already supports
   comma-separated targets.
2. **Is re-rooting layout-managed children (Phase 3) in-scope for the first
   merge, or deferred?** — *Proposed: ship Phase 1+2 first (scene-anchored +
   containers), Phase 3 as a follow-up PR.* This delivers the core use case
   (titles, badges, persistent UI chrome) immediately.
3. **Carry bag for branched graphs** — confirm single-chain walk order is the
   supported topology (matches current `compute_walk_order`). Multi-predecessor
   is warned, not supported.
