# Callout Cleanup Implementation Plan

## Goal

Make targeted `Callout` robust and maintainable by centralizing geometry, typing `place`, improving target resolution/diagnostics, finishing GUI affordances/detach support, reducing warning/test friction, and deferring only high-risk bounds precision work.

## Priority Order

1. Shared Callout geometry + narrow resolver API
2. Typed `CalloutPlace` + diagnostics
3. GUI affordances and source detach support
4. Warning/test-environment cleanup
5. Transform-aware complex bounds as a scoped follow-up

## Plan

### Commit 1 — Centralize Callout geometry derivation

1. Add shared geometry module.
   - Files: `crates/animatix/src/primitives/callout.rs:164`, new `crates/animatix/src/timeline/callout_geometry.rs`
   - Move target-mode math out of `CalloutPrimitive::evaluate` into a public core helper such as `derive_callout_geometry(input, resolver) -> CalloutGeometry`.
   - Include `from`, `to`, `label_point`, `attach`, `place`, `target_name`, and fallback reason in the return type.
   - Expected outcome: core renderer and GUI use one formula for `to`, `from`, `label_at`, `standoff`, and `to_offset`.
   - Verify: `cargo test -p animatix --lib callout`

2. Replace GUI duplicate math.
   - Files: `crates/animatix-gui/src/app/preview/mod.rs:1068`, `crates/animatix-gui/src/app/preview/gestures/callout.rs:58`
   - Replace `callout_effective_to` math with the shared core helper; keep screen conversion/drawing in GUI.
   - Expected outcome: GUI handles match rendered Callout geometry exactly for supported bounds.
   - Verify: `cargo test -p animatix-gui --no-default-features`

3. Narrow `EvaluateCtx`.
   - Files: `crates/animatix/src/primitives/mod.rs:315`, `crates/animatix/src/timeline/scene_eval.rs:498`
   - Replace `EvaluateCtx::timeline: Option<&Timeline>` with a narrow resolver, e.g. `target_resolver: Option<&dyn TargetResolver>` or `scene_query: Option<&dyn SceneGeometryResolver>`.
   - Resolver should expose only lookup methods needed by primitives: actor exists, actor kind, target bounds/transform at time.
   - Expected outcome: primitives no longer receive unrestricted timeline access.
   - Verify: `cargo check -p animatix`

Dependencies: do this before GUI enhancements so new gestures bind to one geometry API.

Risks:
- Trait-object lifetimes may be awkward because `EvaluateCtx` borrows `track`; fallback is a small `SceneGeometryResolver<'a>` struct wrapping `&'a Timeline`.
- Keep resolver read-only; do not let primitives mutate timeline state.

### Commit 2 — Type `place` internally and validate Callout props

1. Add typed `CalloutPlace`.
   - Files: `crates/animatix/src/timeline/animation_track.rs:360`, `crates/animatix/src/timeline/property_track.rs:118`, `crates/animatix/src/timeline/property_engine.rs:57`
   - Define `CalloutPlace { Top, Bottom, Left, Right, Auto }` with parse/display helpers and `Interpolate` as step interpolation.
   - Store `GeometryTracks::callout_place` as `Option<PropertyTrack<CalloutPlace>>` instead of `String`.
   - Expected outcome: engine never branches on raw place strings.
   - Verify: `cargo check -p animatix`

2. Wire typed property plumbing.
   - Files: `crates/animatix/src/timeline/property_registry.rs:44`, `crates/animatix/src/timeline/property_registry.rs:294`, `crates/animatix/src/timeline/dispatch.rs:354`, `crates/animatix/src/timeline/value_parser.rs:81`
   - Add `ValueType::CalloutPlace`, `PropertyValue::CalloutPlace`, `TrackFieldRef/Mut::CalloutPlace`, and use it for registry property `place`.
   - Keep source syntax backward compatible: accept bare identifiers and strings (`right`, `"right"`).
   - Expected outcome: source and GUI still show/edit `place`, but core stores a validated enum.
   - Verify: `cargo test -p animatix --lib registry_is_sorted`

3. Normalize defaults.
   - Files: `crates/animatix/src/primitives/callout.rs:93`, `crates/animatix/src/timeline/property_registry.rs:431`, `crates/animatix/src/timeline/property_registry.rs:680`
   - Make `place` default `right` everywhere and `standoff` default consistent (currently 20/40 split).
   - Suggested decision: keep user-visible default `40.0` if examples/docs already imply that, otherwise migrate docs to `20.0`.
   - Expected outcome: declaration seeding, registry defaults, and evaluation fallback match.
   - Verify: `cargo test -p animatix --lib callout`

4. Add build diagnostics for bad Callout targets/place.
   - Files: `crates/animatix-syntax/src/diagnostics.rs:46`, `crates/animatix/src/timeline/build/actor.rs:673`, `crates/animatix/src/primitives/callout.rs:66`, `crates/animatix-analyzer/src/diagnostics.rs:310`
   - Add codes such as `callout-target-not-found` and `invalid-callout-place`, or reuse `UnknownTargetPath`/`InvalidPropertyValue` with Callout-specific messages.
   - Validate declaration-time `target` after tracks are built, not during primitive seeding if forward declarations are possible.
   - Analyzer should flag `target: missing_actor` and invalid `place` in editor diagnostics.
   - Expected outcome: missing target is a build/analyzer diagnostic, not only a render-time warning.
   - Verify: `cargo test -p animatix --lib callout && cargo test -p animatix-analyzer`

Dependencies: typed `place` should land before GUI place affordance; diagnostics should land before removing render-time warning noise.

Risks:
- `target` as a property name is not the same as action assignment targets; avoid reusing assignment target diagnostics blindly.
- Forward-declared actors require post-build validation over all tracks.

### Commit 3 — Improve target bounds resolution, but keep complex bounds staged

1. Introduce resolver-level bounds API.
   - Files: `crates/animatix/src/timeline/scene_eval.rs:56`, `crates/animatix/src/timeline/scene_eval.rs:533`, new/updated `crates/animatix/src/timeline/callout_geometry.rs`
   - Add `TargetBounds { world_aabb, world_affine, local_half_size, precision }` returned by the resolver.
   - First implementation may use transformed size AABB via existing `actor_world_affine` instead of local `position + size`.
   - Expected outcome: nested transforms and scale are handled better without changing Callout rendering semantics.
   - Verify: `cargo test -p animatix --lib callout`

2. Add regression tests for transformed targets.
   - Files: `crates/animatix/src/timeline/tests/callout.rs:447`
   - Add cases for target inside translated/scaled/rotated parent; assert derived attach point uses world bounds from resolver.
   - Expected outcome: no regression to local-only target bounds.
   - Verify: `cargo test -p animatix --lib callout_target`

Deferral:
- Precise complex/vector/text bounds are deferred. Full precision likely needs evaluated `RenderCommand::local_bounds`, text path bounds, SVG/path bounds, and transform-cache integration. Track as a separate “precise actor bounds service” after the resolver shape is stable.

Risks:
- Circular dependency: Callout evaluation wants target render bounds while target render may be evaluated later. Prefer geometry-track/world-transform bounds for this commit.
- Rotation AABB is visually safe but may attach to bounding-box edge rather than exact rotated shape edge.

### Commit 4 — Add `SourceEdit::RemoveProperty` for Callout detach

1. Add source edit variant.
   - Files: `crates/animatix-gui/src/source_edit/apply.rs:36`, `crates/animatix-gui/src/source_edit/actor_edits.rs:15`
   - Add `SourceEdit::RemoveProperty { actor, property }`.
   - Implement removal from actor declaration props; optionally remove matching assignment at current top-level only if needed later.
   - Expected outcome: source writer can clear `target` without setting `target: ""`.
   - Verify: `cargo test -p animatix-gui --no-default-features source_edit`

2. Wire property command support if needed.
   - Files: `crates/animatix-gui/src/app/actions/mod.rs:224`, `crates/animatix-gui/src/app/commands/document.rs:3`
   - Prefer using `SourceEdit::RemoveProperty` directly from detach command; do not overload `PropertyEdit` with null values unless broader property deletion UI is planned.
   - Expected outcome: Shift-detach can remove `target` cleanly.
   - Verify: `cargo test -p animatix-gui --no-default-features`

Dependencies: needed before Shift-detach GUI behavior.

Risks:
- Existing `SetProperty` falls back to assignment mutation; `RemoveProperty` should be explicit about not deleting keyframed assignments unless specified.

### Commit 5 — Finish Callout GUI affordances

1. Extend Callout drag states.
   - Files: `crates/animatix-gui/src/app/preview/mod.rs:233`, `crates/animatix-gui/src/app/preview/gestures/callout.rs:23`
   - Add states for `CalloutPlace` and `CalloutStandoff`, or a single `CalloutHandle { kind }`.
   - Use shared core geometry to get attach/tip/label/standoff handle positions.
   - Expected outcome: one gesture handler owns all Callout-specific drag interactions.
   - Verify: `cargo test -p animatix-gui --no-default-features`

2. Add place affordance.
   - Files: `crates/animatix-gui/src/app/preview/context.rs:1073`, `crates/animatix-gui/src/app/preview/mod.rs:1099`
   - Draw four side handles around target bounds for targeted Callouts; dragging/clicking one writes `place`.
   - Use typed enum values serialized as source identifiers/strings as appropriate.
   - Expected outcome: user can change `place` visually without inspector text editing.
   - Verify: manual GUI: select targeted Callout, drag/click side handle, `place` updates and arrow repositions.

3. Add standoff drag.
   - Files: `crates/animatix-gui/src/app/preview/gestures/callout.rs:127`, `crates/animatix-gui/src/app/preview/drag_utils.rs:272`
   - Add a handle on the tail/label-side axis that writes scalar `standoff`; clamp to `>= 0`.
   - Optionally route handle position through snap/grid only if it feels consistent.
   - Expected outcome: user can adjust label/tail distance without editing numeric inspector field.
   - Verify: manual GUI: drag standoff handle, source/inspector `standoff` updates.

4. Add Shift-detach.
   - Files: `crates/animatix-gui/src/app/preview/gestures/callout.rs:23`, `crates/animatix-gui/src/source_edit/apply.rs:36`
   - On Shift+drag of targeted Callout, bake current derived `from`/`to` into manual properties and remove `target`; keep `label_at` so label remains visually stable.
   - Use `SourceEdit::RemoveProperty` for `target`; optionally remove `place`, `standoff`, `to_offset` only if product decision says detach should clean all target-mode props.
   - Expected outcome: Shift-detach converts target-mode Callout to manual mode without visual jump.
   - Verify: manual GUI: Shift+drag targeted Callout, source contains `from`/`to` and no `target`, rendered arrow stays in place.

Dependencies: Commit 1 geometry helper and Commit 4 `RemoveProperty`.

Risks:
- Drag event coalescing currently defers source edits; detach needs an atomic multi-edit or stable ordering.
- If keyframe mode is active, decide whether detach bakes at current keyframe only or declaration-level; default should be declaration-level unless user is explicitly keyframing.

### Commit 6 — Reduce existing warning noise

1. Clean Callout warning path.
   - Files: `crates/animatix/src/primitives/callout.rs:185`, `crates/animatix/src/timeline/tests/callout.rs:447`
   - Once build/analyzer diagnostics exist, downgrade render-time missing-target logs to `debug!` or suppress when a diagnostic was already emitted.
   - Expected outcome: repeated per-frame missing-target warnings stop.
   - Verify: run a missing-target Callout test/example and confirm one diagnostic, no frame-spam.

2. Fix known default/value mismatch warnings.
   - Files: `crates/animatix/src/timeline/property_registry.rs:431`, `crates/animatix/src/primitives/callout.rs:93`
   - Align `place` and `standoff` defaults from Commit 2 and update tests/docs.
   - Expected outcome: inspector/default reads do not produce surprising deltas.
   - Verify: `cargo test -p animatix --lib callout`

3. Audit general warnings separately.
   - Files: `crates/animatix/src/lib.rs:1`, workspace-wide
   - Run `cargo check --workspace --no-default-features` first, then decide whether remaining warnings are true issues or policy noise.
   - Expected outcome: create a small follow-up list; do not mix unrelated warning fixes into Callout commits.
   - Verify: `cargo check --workspace --no-default-features`

Deferral:
- Do not chase every workspace warning in the Callout PR unless it blocks relevant checks.

### Commit 7 — Make GUI tests independent of FFmpeg by default

1. Change GUI default features.
   - Files: `crates/animatix-gui/Cargo.toml:8`, `crates/animatix/Cargo.toml:8`
   - Remove `video` from `animatix-gui` default features; keep `video = ["animatix/video"]`.
   - Expected outcome: `cargo test -p animatix-gui` no longer requires system FFmpeg.
   - Verify: `cargo test -p animatix-gui`

2. Preserve video-enabled validation path.
   - Files: `docs/contributing.md:1`, `AGENTS.md:1`, `crates/animatix-gui/src/app/shell/export_dialog.rs:1152`
   - Document explicit commands:
     - `cargo test -p animatix-gui`
     - `cargo check -p animatix-gui --features video`
   - Expected outcome: contributors without FFmpeg can run full GUI tests; release/video checks stay explicit.
   - Verify: `cargo check -p animatix-gui --features video` on an FFmpeg-capable machine.

Risks:
- Packaging/release scripts may assume default video; check CI/release config before merging this change.

### Commit 8 — Docs and examples update

1. Update user docs.
   - Files: `docs/spec.md:540`, `docs/architecture.md:681`
   - Document typed `place` values, target diagnostics, GUI handles, and current bounds precision.
   - Expected outcome: docs match implementation and known limitations.
   - Verify: docs review.

2. Update roadmap/known limitations.
   - Files: `docs/roadmap.md:1`
   - Remove completed Callout cleanup items; keep precise complex bounds as a remaining item if deferred.
   - Expected outcome: roadmap tracks only remaining work.
   - Verify: docs review.

3. Keep parser sync check scoped.
   - Files: `tree-sitter-animatix/grammar.js:1` only if syntax changes
   - No grammar change expected if `place` remains identifier/string and no new tokens are introduced.
   - Verify: `bash scripts/check-parser-sync.sh` if examples or grammar are touched.

## Files To Touch

- `crates/animatix/src/primitives/callout.rs:164` — remove duplicated geometry logic, call shared helper, reduce render-time warning spam.
- `crates/animatix/src/primitives/mod.rs:315` — replace broad `Option<&Timeline>` with a narrow resolver.
- `crates/animatix/src/timeline/scene_eval.rs:498` — provide resolver to primitive evaluation and expose bounds helpers.
- `crates/animatix/src/timeline/animation_track.rs:360` — type `callout_place` and store Callout-specific tracks consistently.
- `crates/animatix/src/timeline/property_registry.rs:44` — add typed value kind/default/schema for `place`.
- `crates/animatix/src/timeline/dispatch.rs:354` — add typed field ref/mut/evaluation plumbing.
- `crates/animatix/src/timeline/value_parser.rs:81` — parse `CalloutPlace` from identifiers/strings with diagnostics.
- `crates/animatix/src/timeline/property_engine.rs:57` — add `PropertyValue::CalloutPlace`.
- `crates/animatix/src/timeline/property_track.rs:118` — add step interpolation for `CalloutPlace`.
- `crates/animatix/src/timeline/tests/callout.rs:447` — add typed/default/diagnostic/bounds regression tests.
- `crates/animatix-syntax/src/diagnostics.rs:46` — add or reuse diagnostic codes for Callout target/place.
- `crates/animatix-analyzer/src/diagnostics.rs:310` — add editor diagnostics for target/place.
- `crates/animatix-gui/src/app/preview/mod.rs:233` — extend Callout drag state/handle helpers.
- `crates/animatix-gui/src/app/preview/context.rs:1073` — draw Callout place/standoff handles.
- `crates/animatix-gui/src/app/preview/gestures/callout.rs:23` — implement place, standoff, and Shift-detach gestures.
- `crates/animatix-gui/src/app/preview/drag_utils.rs:272` — reuse snap/finalization helpers if needed.
- `crates/animatix-gui/src/source_edit/apply.rs:36` — add `SourceEdit::RemoveProperty`.
- `crates/animatix-gui/src/source_edit/actor_edits.rs:15` — implement property removal.
- `crates/animatix-gui/Cargo.toml:8` — remove FFmpeg/video from default GUI feature set.
- `docs/spec.md:540` — update Callout target-mode docs.
- `docs/architecture.md:681` — document shared geometry/resolver architecture.
- `docs/roadmap.md:1` — keep only remaining/deferred work.

## Verification Matrix

- Core Callout geometry/typing: `cargo test -p animatix --lib callout`
- Registry/property plumbing: `cargo test -p animatix --lib registry_is_sorted`
- Analyzer diagnostics: `cargo test -p animatix-analyzer`
- GUI source edits/gestures compile: `cargo test -p animatix-gui --no-default-features`
- GUI no-FFmpeg default after feature change: `cargo test -p animatix-gui`
- Workspace no-FFmpeg compile: `cargo check --workspace --no-default-features`
- Full workspace with local deps available: `cargo check --workspace`
- Parser/docs/examples touched: `bash scripts/check-parser-sync.sh`
- Video-capable machine only: `cargo check -p animatix-gui --features video`

## Deferrals

- Precise shape/text/SVG/vector-path target bounds beyond transformed AABB.
- Animated `target` support beyond stepwise string/target switching, unless product needs it.
- Broad cleanup of all existing workspace warnings unrelated to Callout.
- General enum support for every string-valued registry property; only `CalloutPlace` is in scope.
