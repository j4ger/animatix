# Post-Processing Effects Contract

> Status: **design contract** for the effect abstraction. Runtime evolution
> status lives in `roadmap.md` under "Post-Processing Effect Abstraction";
> rationale in `primitive_abstraction.md` §6. This document is the normative
> contract that host effects and plugin-authored effects must both satisfy.

## 1. Model

An **effect** is a stage in an ordered chain owned by a `Filter` compositing
scope. Effects are **not primitives or actors**: a `Filter` lowers its effect
children into the scope's own `EffectChainTrack` at build time, so they create
no scene-graph node, no layout entry, and no hit region. Declaring an effect
anywhere other than inside a `Filter` scope is a build diagnostic.

```
bg: Filter {
  soft: Blur, radius: 10                 // effects first, in application order
  warm: ColorGrade, contrast: 1.15
  photo: Image, url: "photo.jpg"         // content
}
#1.5s
bg.soft.radius = 24 [1.5s, ease: ease-out]  // effect params animate
```

Rules:

- **Chain membership is static.** A stage is fixed at build time; it cannot be
  added or removed over time. Its parameters animate like any other property.
- **Effects are declared before content.** Effect children do not render as
  content and do not participate in layout.
- **Parameters are `DynTrack`-backed.** Each stage stores its parameters as
  `BTreeMap<param_name, DynTrack>` plus an `enabled: DynTrack`, so animation,
  keyframe timing, snapshot/collapse, and persistence (`CarryBag`) reuse the
  dynamic-track machinery. Effect parameters do **not** enter the primitive
  property registry (`PROPERTY_REGISTRY` / `ActorField` / `PropertyPlan`).
- **Effect schema has one author-visible source per layer.** Built-ins implement
  the `Effect` trait (`timeline/effects/`), which owns the WGSL, passes, uniform
  layout, identity values, and spatial support; plugin effects implement the
  same trait over FFI-declared data (`PluginEffectData`). The analyzer's
  `animatix_syntax::schema::effect_specs()` mirrors the parameter names, kinds,
  and display names. `effect_specs_match_runtime_descriptors` pins the two tables
  together in both directions.
- **Every effect has an implicit `enabled: Bool`** (default `true`, animatable).
  There is no generic `mix` in v1.

Assignment syntax is scope-qualified: `scope.stage.param = value`. Parameters
are addressed by their authored names; the chain is sampled per frame into the
renderer-facing `EffectChain`.

## 2. On-demand evaluation

The host skips an effect with no contribution. An effect instance is skipped
when either:

1. `enabled` is `false`; or
2. every declared parameter equals its **identity value** (declared in the
   effect's parameter schema).

If every instance in the chain is skipped, the offscreen round-trip is skipped
entirely and the content children are appended directly to the parent scene.
The pipeline must not allocate targets or dispatch passes for a skipped effect.

Identity values are declarative, never callbacks — they must be expressible
across the FFI boundary. `enabled` is *not* part of the identity rule; an
unauthored `enabled` resolves to `true`.

Built-in examples:

| Effect | Parameter | Identity |
|---|---|---|
| `Blur` | `radius` | `0` |
| `ColorGrade` | `brightness` | `1.0` |
| `ColorGrade` | `contrast` | `1.0` |
| `ColorGrade` | `saturate` | `1.0` |
| `ColorGrade` | `hue_rotate` | `0` |
| `ColorGrade` | `sepia` | `0` |
| `ChromaticAberration` | `offset` | `0` |

## 3. Spatial support and ROI

Every effect declares a **spatial support**: how far outside a pixel it reads,
in scene pixels, as a function of its parameters (blur reads `radius` texels; a
colour matrix reads nothing). Support is the `Effect::support` method; the
chain's `worst_case_support()` evaluates it over every parametric keyframe time
and sums the per-stage maxima, so the result is constant per track (PF-7: no
per-frame reallocation).

**Implemented (explicit knob).** `Filter, bounds: (x, y, w, h)` restricts effect
processing to that region: the rendered sub-scene is cropped to
`bounds ∪ worst-case support` (clamped to the scene), the chain dispatches at
the region size, and the result is composited back at the region origin —
through the readback path (drawn at the origin) or the zero-readback path
(a viewport-scoped blit). Textures stay at full scene capacity; only the seed
copy, dispatch, and readback shrink. Without `bounds`, the whole scene is
processed exactly as before.

**Derived ROI (implemented).** When no `bounds:` is authored, the region is
derived from the content bounds the sub-scene evaluation itself records
(`precise_bounds`): union the content subtree's world bounds, pad by the chain's
worst-case support, and clamp to the scene. The bounds are exact for the frame
being rendered — no staleness, no heuristics — and since the GPU textures stay
at full scene capacity, a region that grows or shrinks between frames never
reallocates. `Blur.radius` animating simply widens the region automatically.

Composite ordering: the zero-readback path keeps the existing
`can_post_composite_filter` precondition ("this scope is the last rendered
element"), under which a rect-scoped composite can never overwrite later
geometry. Generalising that precondition to per-region intersection remains an
optional optimization.

Two constraints:

- **Allocation stability.** An animating support would resize the target every
  frame. The region uses the *worst-case* support over the effect track's
  parameter range, and the GPU textures stay at full scene capacity (only the
  seed copy, dispatch, and readback shrink), preserving the PF-7/PF-9 budget.
- **Composite ordering.** A rect-scoped composite is applied after the full
  scene render, so it overwrites anything drawn later inside its rect. The
  zero-readback path therefore keeps the existing `can_post_composite_filter`
  precondition ("this scope is the last rendered element"); generalising it to
  "no later sibling intersects the ROI" is part of the derived-ROI work.

## 4. GPU pass contract

An effect declares an ordered list of compute passes. Each pass shares one
binding layout.

### 4.1 Textures

- Format: `Rgba8Unorm` for both input and output.
- Input usage: `TEXTURE_BINDING`; output usage: `STORAGE_BINDING`.
- The host owns the ping-pong textures. A shader must never assume whether it
  read or wrote a given physical texture; `binding 0` is always the input view
  and `binding 1` always the output view for that pass.
- **One input texture in v1.** A second input (original for bloom add-back, or a
  mask) is deliberately *not* reserved. Adding it is a deliberate ABI bump, not
  a speculative field.

### 4.2 Bind group 0

| Binding | Type | Owner |
|---|---|---|
| 0 | `texture_2d<f32>` | host (input) |
| 1 | `texture_storage_2d<rgba8unorm, write>` | host (output) |
| 2 | `var<uniform>` — author parameters | effect schema |
| 3 | `var<uniform> EffectContext` | host |
| 4 | `sampler` (linear, clamp-to-edge) | host |

Binding 4 is always present in the layout so sub-pixel effects (chromatic
aberration, motion blur) are expressible; a shader that uses only `textureLoad`
simply does not declare it.

### 4.3 Host context (binding 3)

`EffectContext` is fixed and host-owned, so context values never masquerade as
author parameters:

```wgsl
struct EffectContext {
    tex_size: vec2<u32>,   // target size in texels
    _pad0: vec2<u32>,
    inv_size: vec2<f32>,   // 1.0 / tex_size
    _pad1: vec2<f32>,
    pass_index: u32,       // 0-based pass within this effect
    pass_count: u32,       // total passes declared by this effect
    time_ms: f32,          // timeline time
    _pad2: f32,
}
```

`pass_index` is how an N-pass effect selects behaviour (blur uses 0 =
horizontal, 1 = vertical).

### 4.4 Author parameters (binding 2)

- Types: `f32`, `u32`, `vec2`, `vec4`/colour, `bool` (marshalled as `u32`).
- v1 forbids `vec3`, arrays, and nested structs — WGSL uniform alignment makes
  them a silent-misalignment hazard.
- The uniform buffer is padded to a multiple of 16 bytes. Built-in effects may
  supply a host-side packer (e.g. `ColorGrade` composes its five scalars into
  four `vec4` rows); plugin effects use the generic layout declared in their
  parameter schema.

### 4.5 Dispatch

- `@workgroup_size(16, 16)`, entry point `main`, dispatch
  `(width.div_ceil(16), height.div_ceil(16), 1)`.
- Out-of-range invocations are the shader's responsibility to guard.

### 4.6 Synchronisation

Each pass is submitted in its own command encoder. Back-to-back compute passes
sharing ping-pong textures in one encoder do not make a storage write visible to
the next pass on every driver (probe 009); a submit boundary is a portable
synchronisation point. Do not merge passes into one encoder without driver
evidence.

## 5. Failure policy

- **No GPU backend:** effects are skipped, the content children render
  unfiltered, and a `RenderFailure` diagnostic is pushed. Uniform for built-ins
  and plugins — there is no CPU fallback.
- **Pipeline creation/validation failure:** report a diagnostic identifying the
  effect and the wgpu error; skip that effect (the rest of the chain still runs
  where independently valid).
- **Unsupported parameter value:** log a warning and treat the parameter as its
  identity value; never drop silently.
- Arbitrary WGSL has no CPU path, so an effect is defined by its GPU
  implementation only.

## 6. Determinism and caching

- Chain order equals declaration order; pass order equals the descriptor's pass
  list. Neither is ever reordered for optimisation without a documented,
  deterministic rule.
- Compute pipelines are cached on `(shader source, entry point, layout)`. The
  cache is per-device and lives with the backend. Dimension changes recreate
  targets but must not recreate pipelines whose key is unchanged.

## 7. Plugin authoring (implemented, ABI snapshot 9)

A plugin supplies **WGSL source text and a parameter schema, never a GPU
handle**. The host compiles the shader with its own device, owns all textures
and synchronisation, and marshals parameters into the declared uniform layout
(`pack_generic` driven by the declared offsets). Registration goes through
`NativePluginApi.register_effect(host, NativeEffectDescriptor)`; see
`docs/extension_authoring.md` ("Plugin-authored effects") for the full surface.

At registration the host:

- validates the declared uniform layout — known parameter kinds, declared
  offsets aligned to their kinds, buffer size a multiple of 16 — rejecting the
  effect with `NATIVE_STATUS_TYPE_ERROR` on mismatch;
- rejects name collisions with built-in effects or previously registered
  plugin effects (re-registering the same plugin name is idempotent);
- registers the effect under its authored type name (`EffectId` is the name),
  tracked for install rollback;
- compiles the WGSL lazily at first render, where wgpu validation errors
  surface as runtime diagnostics and the stage is skipped.

wgpu validates syntax but not termination: this is **trusted authoring**, not a
sandbox. Effects are GPU-only — no backend means the stage is skipped with a
warning, exactly like built-ins.

## 8. Adding a built-in effect

1. Create `crates/animatix/src/timeline/effects/<name>.rs` implementing the
   `Effect` trait — parameter schema, WGSL passes, `pack`, and `support` in one
   file.
2. Add `&<name>::CONST` to the `EFFECTS` bootstrap array.
3. Add an `EffectId` variant.
4. Add the author-visible parameters to
   `animatix-syntax/src/schema.rs::effect_specs()`.
5. Document it here (the identity table in §2) and in `docs/spec.md`.
6. If the effect reads a neighbourhood or displaces samples, calibrate
   `support` — the derived ROI pads by it, and an under-reported support clips
   pixels silently.

Steps 3-4 are hand-maintained (a persisted enum tag and a cross-crate schema
table); everything else derives from the one file, mirroring how
`primitives/mod.rs` documents adding a primitive.

## 9. Editor support

The GUI insertion palette lists the built-in catalog (the "Effects" tab, built
from `EFFECTS`) and inserts `label: Blur` at the head of a selected `Filter`
scope's body, ahead of its content. A newly inserted stage sits at its identity
values, so set or animate its parameters from the Inspector's "Effects" group;
effect parameters also get timeline lanes (`stage.param`, `stage.enabled`). The
analyzer validates effect parameter names and types in both declarations and
`scope.stage.param` assignments.

