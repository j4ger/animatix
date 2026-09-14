# Primitive Abstraction: Status and Remaining Gaps

Audience: contributors. This note records where the "every actor type is one
`Primitive`" abstraction stands, what is deliberately *not* unified, and the
concrete gaps that remain before a "fully ideal" abstraction — plus a
recommended order if those gaps are ever pursued.

It is a status/design note, not a work ticket. The deferred items are also
recorded in `roadmap.md`; completed work moved to `docs/history.md`.

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
    `vello::Scene`, then calls `dyn FilterBackend` (`timeline/effects/chain.rs`)
    whose GPU methods take `&vello::Scene` and return either a `SceneImage`
    (`vello::peniko::ImageData`) or a `PendingComposite` holding a
    `wgpu::Texture` / `TextureView`. Trait objects have no stable
    C vtable, and `vello`/`wgpu` ownership, threading, and version coupling
    cannot cross.
  - **Typst compilation (`Equation`).** `compile_typst_grouped_cached`
    (`crates/animatix-text/src/lib.rs`) takes a `&FontContext` and returns an
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

## 6. Post-processing effects: a chain, not a primitive category

`Filter` used to be a *container strategy* (`ChildProcessing::Filter`) whose
parameter set was baked into the track (`FilterTracks` — six fixed
`PropertyTrack<f32>`), registered per `"Filter"` actor type, and implemented by
two fixed WGSL shaders. It could only be applied by containing children, and
every new effect would have touched `FilterTracks`, the property table, and
`scene_eval`.

**Direction (implemented 2026-09-11): effects are a first-class chain owned by
the scope.** A `Filter` lowers its effect children into its own
`EffectChainTrack` at build time (`timeline/effects/track.rs`,
`build/effect.rs`); the
stages are **not** primitives, actors, or scene nodes, and their parameters are
`DynTrack`-backed rather than registry properties. This avoids both the
`ActorField`/`PROPERTY_REGISTRY` entanglement and the discovery that
extension-registered parameters never apply on the primary build paths (those
run with `extensions: None`). Background and remaining work: `docs/effects.md`
(contract) and `docs/history.md` ("Post-Processing Effect Abstraction").

**Authoring surface (shipped, not backward compatible).** The flat
`Filter, blur: …, brightness: …` properties are removed. Effects are declared as
labeled child declarations of a compositing scope, applied in declaration order:

```animatix
bg: Filter {
  soft: Blur, radius: 10
  warm: ColorGrade, contrast: 1.15, saturate: 0.9
  photo: Image, url: "photo.jpg", size: fill
}
#1.5s
bg.soft.radius = 24 [1.5s, ease: ease-out]
```

The chain is **static in membership** (an effect child cannot appear or disappear
over time); effect *parameters* animate like any other property. Children
partition by category into effects vs. content, so no new block kind is
introduced. `.amx` files, examples, and `docs/spec.md` are rewritten — no
compatibility shim.

`Filter` is the only effect scope in v1; an effect primitive declared outside one
is a diagnostic. Effect parameters are stored in the **dynamic property slots**
(`PropertyPlan` / `PropertyId`), not as new `ActorField` variants, so the
`ActorField::Filter*` enum surface is deleted rather than extended. Every effect
gets an implicit animatable `enabled: Bool` (default `true`) and there is no
generic `mix` in v1. The first two effects are `Blur` and `ColorGrade`.

> **Constraint (discovered 2026-09-11).** The slot *storage* is generic, but the
> *registration* cannot go through `ExtensionRegistry` for built-in effects: the
> primary build paths (`Timeline::build(&ast)` in the GUI document and in
> image/video export) build with `extensions: None`
> (`timeline/mod.rs:765`), and extension properties are only written by
> `write_extension_properties_for_decl` when a context is attached. Built-in
> effect parameters must be intrinsic built-in properties (`PROPERTY_REGISTRY` +
> `PROPERTY_DESCRIPTORS`) with a generic per-track binding (`Tagged`/plan-slot),
> not per-parameter `ActorField` variants. Plugin effects (Stage 4) imply an
> extension context and can use the extension-property path.

**Texture / pass contract (decided).**

- Input/output `Rgba8Unorm` with `TEXTURE_BINDING | STORAGE_BINDING`.
- Bind group 0: binding 0 `texture_2d<f32>`, binding 1
  `texture_storage_2d<rgba8unorm, write>`, binding 2 = the effect's own
  parameters, binding 3 = a host-owned `EffectContext` (`tex_size`, `inv_size`,
  `pass_index`, `pass_count`, `time_ms`). Host context is a separate binding so
  context values never masquerade as author parameters (blur needs `direction`
  and `tex_size`).
- A linear sampler binding, so sub-pixel effects (chromatic aberration, motion
  blur) are expressible.
- `@workgroup_size(16, 16)`, entry `main`, dispatch `div_ceil(16)`.
- An N-pass model; each effect declares an ordered pass list and reads
  `pass_index` from `EffectContext` (blur is H+V).
- **One input texture in v1.** A second input (original / mask) is deliberately
  *not* reserved: when a real effect needs it (bloom add-back), that is a
  deliberate ABI bump, not a speculative field.
- Host owns ping-pong textures; a plugin never assumes source/destination.

**On-demand and ROI (decided).** Because parameters are registered properties
with declared identity defaults and each effect declares a spatial support, the
pipeline skips any effect whose `enabled` is false or whose parameters are all
identity, and sizes the offscreen target to `content bounds ∪ max support`
instead of the full scene. ROI is derived, not authored: `soft.radius = 24`
widens the support to 72px and the target follows. An optional
`Filter, bounds: auto | (x, y, w, h)` override is the only explicit knob. Two
accepted costs: (a) an animating support would resize the target per frame, so
the target uses the track's worst-case support (full-scene fallback), preserving
the PF-7 allocation budget; (b) a rect-scoped blit is applied after the full
scene render, so the `can_post_composite_filter` precondition generalizes from
"last rendered element" to "no later sibling intersects the ROI".

Costs and limits: arbitrary WGSL has no CPU fallback (decided, expected — no
backend means "skip + warn" with a diagnostic, uniformly for built-ins and
plugins); wgpu validates syntax but not termination, so this is trusted-authoring
rather than a sandbox; pipelines cache on `(source, entry, layout)`; effects
needing a second texture or geometry info are outside the v1 contract.

Migration blast radius: the true `FilterTracks` / `filter_*` /
`ActorField::Filter*` surface is ~141 identifier references across ~13 files
(`dispatch`, property registry, animation track, persistence, GUI timeline
diff/panel/inspector, tests) — **not** the "73" raw grep count, which also
matches iterator `.filter(|…|)` calls. Moving effects to a primitive category
retires that surface rather than porting it.

Stages live in `docs/history.md` under "Post-Processing Effect Abstraction".

## 7. Guardrails to preserve

Any work above must keep these green (and extend them where behaviour changes):

- `cargo test -p animatix --lib -- --test-threads=1`
- `cargo test --workspace --no-fail-fast -- --test-threads=1`
- `scripts/check_examples.sh`, `scripts/render_smoke.sh`,
  `scripts/dogfood-verify.sh`, `scripts/check-docs.sh`
- ABI drift: `abi_version_stays_in_sync_with_authoring_docs` in
  `crates/animatix-plugin-api` (pins the docs snapshot string).
