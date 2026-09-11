# Primitive Abstraction: Status and Remaining Gaps

Audience: contributors. This note records where the "every actor type is one
`Primitive`" abstraction stands, what is deliberately *not* unified, and the
concrete gaps that remain before a "fully ideal" abstraction — plus a
recommended order if those gaps are ever pursued.

It is a status/design note, not a work ticket. The deferred items are also
recorded in `roadmap.md` ("Primitive abstraction — deferred decisions"); open
work stays in `roadmap.md`.

## 1. The model today

`crates/animatix/src/primitives/mod.rs` defines `Primitive`; the primitive
registry (`crates/animatix/src/primitives/registry.rs`) stores built-ins and
extensions uniformly as `Builtin(&'static dyn Primitive)` /
`Extension(Arc<dyn Primitive>)`. The following are trait-driven with no
per-type branching in the pipeline:

| Concern | Entry point | Notes |
|---|---|---|
| Per-actor drawing | `Primitive::evaluate()` | `scene_eval::evaluate_node` is the only draw path; the legacy manual `ActorKindId` match is gone. |
| Child rendering | `Primitive::render_children(&mut RenderChildrenCtx, &[&str])` | `scene_eval::render_node_children` contains no `ChildProcessing` branch; `Filter`/`Mask`/`Equation` override it. |
| Mask clip geometry | `Primitive::clip_path(&EvaluateCtx)` | Plot geometry is excluded (`plot_geometry`); the Mask warns and falls back to a rect. |
| Equation fragments | `Primitive::equation_fragment(&EvaluateCtx)` | No `ActorKindId::Fragment` check. |
| Identity | `AnimationTrack::actor_type: String` | Required and registry-resolved; `kind` is derived by `set_identity` (extensions may override via `kind_id()`), with a build-time drift warning. |
| Native extensions | `crates/animatix-plugin-api` + `extension_native_plugin.rs` | `repr(C)` callbacks: `build`, `evaluate`, `handle_assignment`, `finalize_container_build`, `default_props`, `default_color_key`, `clip_path`, `equation_fragment` (ABI snapshot 8). |

Invariants are pinned by tests: `every_built_track_identity_is_consistent`
(`timeline/tests/build.rs`), `registry_matches_primitives` /
`child_processing_capabilities_cover_special_containers` (`primitives/mod.rs`),
`custom_primitive_builds_through_timeline` (`primitives/registry.rs`).

## 2. Deliberately not unified (accepted boundaries)

These are engineering trade-offs, not oversights. Document them; do not "fix"
them without a concrete driver.

- **The FFI boundary.** Native `cdylib` plugins cannot share Rust pipeline
  internals — a `repr(C)` ABI carries only POD structs, raw pointers, and
  `extern "C"` fn pointers, never trait objects, types with destructors, or GPU
  handles. Concretely:
  - **Offscreen GPU composite (`Filter`).** The strategy renders children into a
    `vello::Scene`, then calls `dyn FilterBackend` (`timeline/filter.rs:22`)
    whose GPU methods take `&vello::Scene` and return either a `SceneImage`
    (`vello::peniko::ImageData`) or a `PendingComposite` holding a
    `wgpu::Texture` / `TextureView` (`filter.rs:6`). Trait objects have no stable
    C vtable, and `vello`/`wgpu` ownership, threading, and version coupling
    cannot cross.
  - **Typst compilation (`Equation`).** `compile_typst_grouped_cached`
    (`renderer/text.rs:2260`) takes a `&FontContext` and returns an
    `Arc<CachedGroupedText>` from a process-wide memo — all Rust types, and the
    Typst engine has no C surface.
  - **Sub-scene readback.** The only image the ABI ingests is
    `NativeImageCommand` — a URL resolved from the asset cache
    (`crates/animatix-plugin-api/src/lib.rs:485`); there is deliberately no
    "here are pixels" or "here is a texture" command, because `SceneImage` /
    `RenderedFrame` carry `vello` types and a buffer park/reuse protocol.

  The host adapter is a hand-written mirror, and every new trait method needs a
  callback plus an ABI bump. Plugins reuse the host's `Filter`/`Mask`/`Equation`
  by declaring `child_processing`. The one route that *does* cross this boundary
  is **source text, not handles**: a plugin can ship WGSL plus a parameter
  schema and let the host compile and run it on its own device (see §6).
- **Container strategy bodies live in `Timeline`, not the primitive.** The
  built-in `render_children` overrides (`primitives/filter.rs`, `mask.rs`,
  `equation.rs`) are one-line delegations to `Timeline::render_*_children_ctx`
  (`timeline/scene_eval.rs`). Moving the bodies into `primitives/*` would pull
  `filter_backend` (a `&mut Option<&mut dyn FilterBackend>` with a nested
  lifetime) and the Typst compiler into the primitive layer. The split is
  intentional.

## 3. Remaining gaps (ranked by leverage)

| # | Gap | Evidence | Ideal |
|---|---|---|---|
| G1 | **Build side is not unified** — biggest asymmetry | `find_actor_kind` returns `None` for `Shape`/`Container`, so those are built inline by `process_actor_decl` (`Rect::build` is a no-op); `Text::build` delegates to `timeline::declarations_text`, plot/media similarly (`timeline/actor_kind.rs:28`, `primitives/text.rs:40`) | Each primitive's `build()` owns its logic; no central kind dispatch; shapes/containers also go through the trait. |
| G2 | **Property system is kind-centric** | `PropertyPlan::for_actor_kind(kind)`, `Applicable::includes(kind)`, `allowed_property_indices(kind)`, `(schema.default_value)(track.kind)` (`timeline/plan.rs:108`, `property_registry.rs:1872`, `property_engine.rs:305`) | Built-in property applicability/defaults declared on the primitive; `kind` reduced to an internal cache key. |
| G3 | **Identity is dual and not pre-resolved** | `actor_type: String` + `kind: ActorKindId` (the latter independently writable by extensions); the frame path hashes the registry per node per frame (`scene_eval.rs:740`, `primitive_registry.find(&track.actor_type)`) | One identity; build resolves the primitive handle (or an interned id) onto the track so the frame path does no lookup. |
| G4 | **Overlapping taxonomies** | `ActorKindId`, `ShapeKind`, `ActorCategory`, `PrimitiveCategory` (schema), `ChildProcessing`, `PrimitiveFamily` mutually mapped and drift-pinned | One capability description per primitive; the rest derived views. |
| G5 | **Shape semantics partly outside the trait** | `vector_shape_uses_custom_path` / `exposes_tip_size` are `ShapeType` free functions for the GUI edit-vertices layer (`timeline/shapes/mod.rs`) | Shape behaviour on the primitive (or `ShapeType` as a primitive view). |
| G6 | **Child strategy is a closed enum** (declaration, not dispatch) | `ChildProcessing::{Generic,Filter,Mask,Equation}` is a shared-schema/ABI enum; a native plugin cannot author a fifth strategy (ABI 9 deferred) | Open strategy surface. Accepted as-is pending a real plugin need. |
| G7 | **`RenderCommand` is a second closed ABI** | `#[non_exhaustive]` enum with manual `execute` / `local_bounds` dispatch (`primitives/mod.rs`) | Extensible draw primitives. |
| G8 | **FFI mirror surface** (inherent) | See §2 | N/A — bounded by what `repr(C)` can carry. |

## 4. Deferred decisions (decided, not scheduled)

Recorded so they are not re-litigated without new evidence:

- **Native `render_children` callback (ABI 9).** Plugins cannot be given the
  host's child pipelines (see §2). If a plugin ever needs a genuinely new
  strategy, implement the minimal surface — child enumeration + `render_child` +
  a host `push_clip_layer`/`pop_layer` + the existing `append_*` callbacks —
  keep it orthogonal to `child_processing` (so analyzer/manifests are
  untouched), and document that offscreen GPU and Typst are out of scope.
- **Privatizing `AnimationTrack.kind`.** `kind` is an intentional independent
  override for extension primitives (`kind_id()`), so privacy would relocate one
  write rather than make `actor_type` the sole identity. The invariant is already
  enforced by `set_identity` + the build-time drift warning +
  `every_built_track_identity_is_consistent`.

## 5. If pursued: recommended order

Do these only when driven by evidence (a profile, an extension-author need, or a
feature that hits the boundary) — not for purity.

1. **G2 — property primitive-ization.** Self-contained relative to the others
   (touches `property_registry`/`plan`/`property_engine` and the primitive
   declarations), and it removes one taxonomy. Start here.
2. **G1 — build-side unification.** Largest and dirtiest: migrate each
   `timeline/build/*` kind-specific builder into its primitive's `build()`, and
   route shapes/containers through the trait. Land one primitive family per
   change.
3. **G3 — single identity + pre-resolved handle.** Fold into (2): when build
   ends it already holds the primitive, so store the resolved handle on the
   track. Yields a design cleanup plus a per-node lookup removal.
4. **G4/G5/G7 — opportunistic.** Taxonomy convergence, shape semantics, and
   `RenderCommand` openness as the surrounding modules are next edited.
5. **G6/G8 — keep as-is.** Documented boundaries; do not expand the ABI without a
   concrete plugin need.

## 6. Post-processing effects: a second abstraction?

`Filter` is currently a *container strategy* (`ChildProcessing::Filter`) whose
parameter set is compiled into the track (`FilterTracks` — six fixed
`PropertyTrack<f32>`, `timeline/animation_track.rs:348`), registered per
`"Filter"` actor type (`animatix-syntax/src/schema.rs:521`), and implemented by
two fixed WGSL shaders plus a CPU fallback (`renderer/filter_backend.rs:23/80`,
`timeline/filter.rs:83`). It can only be applied by containing children.

Pulling effects out into their own axis — an effect descriptor + registry, and a
generic offscreen→effect→composite pipeline — is a better separation of concerns:
`ChildProcessing` returns to structural aggregation (`Mask` clip geometry,
`Equation` aggregation) while visual post-processing becomes its own axis. Three
tiers:

1. **Seam (small).** An effect description (name + parameter schema) and a
   registry; the `Filter` container becomes a generic "apply this effect chain to
   my children" shell. Adding a host effect stops touching `FilterTracks`, the
   property table, and `scene_eval`.
2. **Pass / render-graph (medium).** Generalize offscreen→effect→composite into a
   pass graph with ping-pong, chaining, and non-container targets (an actor or the
   whole scene). Real renderer work that must preserve the PF-7/PF-9 buffer-reuse
   and pending-composite invariants.
3. **Plugin-authored effects (large, but viable).** Register effects through the
   extension system; the key is that a plugin supplies **WGSL source + a parameter
   schema**, never a GPU handle — the host owns the device, textures,
   synchronization, and lifetime, so this *does* cross the FFI boundary (§2). The
   contract to pin: `Rgba8Unorm` input/output with
   `TEXTURE_BINDING | STORAGE_BINDING`; bind group 0 =
   `texture_2d<f32>` / `texture_storage_2d<rgba8unorm, write>` / `var<uniform>`;
   `@workgroup_size(16, 16)` with entry `main`; an N-pass model (blur is H+V); a
   minimal typed parameter model the host marshals into the plugin's uniform
   struct; host-owned ping-pong textures.

Costs and limits: arbitrary WGSL has **no CPU fallback**, so with no GPU backend a
plugin effect is "skip + warn" (unlike the built-in six, which keep a CPU
implementation); wgpu validates syntax but not termination, so this is
trusted-authoring rather than a sandbox; pipelines must be cached on
`(source, entry, layout)`; and effects needing a second texture or geometry info
are outside the contract.

Migration blast radius: `FilterTracks` / `filter_*` has 73 references across 10+
files (GUI timeline diff, timeline panel, inspector, persistence `CarryBag`,
`property_engine`, `dispatch`, `scene_eval`, tests), so moving to a generic
parameter model is a renderer **plus** property/GUI/persistence change. With only
one effect today, the recommended path is staged: effect contract (design doc) →
in-process seam → validate with a second effect → extend the ABI → migrate the
property tracks last. Tracked in `roadmap.md` under "Post-Processing Effect
Abstraction".

## 7. Guardrails to preserve

Any work above must keep these green (and extend them where behaviour changes):

- `cargo test -p animatix --lib -- --test-threads=1`
- `cargo test --workspace --no-fail-fast -- --test-threads=1`
- `scripts/check_examples.sh`, `scripts/render_smoke.sh`,
  `scripts/dogfood-verify.sh`, `scripts/check-docs.sh`
- ABI drift: `abi_version_stays_in_sync_with_authoring_docs` in
  `crates/animatix-plugin-api` (pins the docs snapshot string).
