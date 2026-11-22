# Post-Processing Effects Contract

> Status: **design contract** for the effect abstraction. Runtime evolution
> status lives in `docs/history/2026-08.md` under "Post-Processing Effect Abstraction";
> rationale in `primitive_abstraction.md` §6. This document is the normative
> contract that host effects and plugin-authored effects must both satisfy.

## 1. Model

An **effect** is a stage in an ordered chain owned by a `Filter` or `Glass`
compositing scope. Effects are **not primitives or actors**: the scope lowers
its effect children into its own `EffectChainTrack` at build time, so they
create no scene-graph node, no layout entry, and no hit region. Declaring an
effect anywhere other than inside an effect scope is a build diagnostic.

```
bg: Filter {
  soft: Blur, radius: 10                 // effects first, in application order
  warm: ColorGrade, contrast: 1.15
  photo: Image, url: "photo.jpg"         // content
}
#1.5s
bg.soft.radius = 24 [1.5s, ease: ease_out]  // effect params animate
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
- **Effect schema has one author-visible source, period.** The effect's own
  file under `animatix-core::builtins` declares everything: parameters (name, kind,
  identity), display name, WGSL passes, `pack`, and `support`. Uniform offsets
  follow the host rule (scalars 4-byte aligned, vec2 at 8, vec4 at 16,
  sequential). The parser crate derives its contract table
  (`effect_specs()`) from the same catalog, so nothing is declared twice.
  Plugin effects implement the same trait over FFI-declared data
  (`PluginEffectData`) and use the identical identity convention
  (`[x, y, z, w]` interpreted per kind).
- **Every effect has an implicit `enabled: Bool`** (default `true`, animatable).
  There is no generic `mix` parameter: a pass that balances the original frame
  against the running result exposes an effect-specific control instead —
  `Bloom.keep` is that control for the one built-in that needs it.

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

Built-in identity values:

| Effect | Parameter | Identity value |
|---|---|---|
| `Blur` | `radius` | `0` |
| `ColorGrade` | `brightness` | `1.0` |
| `ColorGrade` | `contrast` | `1.0` |
| `ColorGrade` | `saturate` | `1.0` |
| `ColorGrade` | `hue_rotate` | `0` |
| `ColorGrade` | `sepia` | `0` |
| `ChromaticAberration` | `offset` | `0` |
| `Sharpen` | `amount` | `0` |
| `Sharpen` | `radius` | `0` |
| `Bloom` | `intensity` | `0` |
| `Bloom` | `keep` | `1.0` |
| `Bloom` | `threshold` | `0` |
| `Vignette` | `amount` | `0` |
| `Vignette` | `radius` | `0.9` |
| `Vignette` | `softness` | `0.6` |
| `Vignette` | `color` | `(0, 0, 0, 1)` |
| `Vignette` | `center` | `(0.5, 0.5)` |
| `MotionBlur` | `length` | `0` |
| `MotionBlur` | `angle` | `0` |
| `Grain` | `amount` | `0` |
| `Grain` | `seed` | `0` |
| `Grain` | `monochrome` | `false` |
| `Grain` | `animated` | `true` |
| `Levels` | `in_black` | `0` |
| `Levels` | `in_white` | `1` |
| `Levels` | `gamma` | `1` |
| `Levels` | `out_black` | `0` |
| `Levels` | `out_white` | `1` |
| `Duotone` | `amount` | `0` |
| `Duotone` | `shadow` | `(0, 0, 0, 1)` |
| `Duotone` | `highlight` | `(1, 1, 1, 1)` |
| `Posterize` | `levels` | `0` |
| `Edge` | `amount` | `0` |
| `Edge` | `threshold` | `0` |
| `LensDistortion` | `amount` | `0` |
| `DropShadow` | `offset` | `(0, 0)` |
| `DropShadow` | `softness` | `0` |
| `DropShadow` | `color` | `(0, 0, 0, 1)` |

Set `Grain.animated` to `false` to keep its pattern fixed for a given seed
and texture size. To add a subtle paper texture, use a small monochrome amount
such as `0.012` and put only the backdrop inside the `Filter`. Declare text,
fine rules and diagrams after the filter so their pixels bypass the effect.

**Parameter units.** Most parameters are unitless, normalized fractions, or
angles. Exactly seven are lengths in **scene pixels**:
`Blur.radius`, `ChromaticAberration.offset`, `Sharpen.radius`,
`MotionBlur.length`, `DropShadow.offset`, `DropShadow.softness`, and
`LensDistortion.amount`. All seven scale with the render resolution the same
way actor geometry does; the remaining length-like parameters are fractions of
a local reference (for example `Vignette.radius` and `Vignette.softness` are
fractions of the shorter texture side).

`Vignette.center` accepts an actor point such as `card.center` or a normalized
effect-texture coordinate pair. Named points follow the actor and use the
actual crop and render scale for their coordinate conversion. For pairs, `(0, 0)` is the
top left and `(1, 1)` is the bottom right. Within a smaller scope the coordinates
include dispatch padding and quantization (§3). For scene-relative framing,
use a `Filter` whose bounds cover the entire canvas.

## 3. Spatial support and ROI

Every effect declares a **spatial support**: how far outside a pixel it reads,
in scene pixels, as a function of its parameters (blur reads `radius` scene
pixels; a colour matrix reads nothing). Support is the `Effect::support` method;
the chain's `worst_case_support()` evaluates it over every parametric keyframe
time and sums the per-stage maxima, so the result is constant per track (PF-7:
no per-frame reallocation).

**Implemented (explicit knob).** `Filter, bounds: (x, y, w, h)` restricts the
effect to that region. `bounds ∪ worst-case support` (clamped to the scene)
determines the harvested area, which is composited back at the region origin.
A valid authored region that covers the whole canvas after camera mapping,
support padding and clipping selects the full-scene path; it does **not**
fall back to a content-derived ROI. This distinction matters for region-relative
effects such as `Vignette`. Absent or nonpositive-size bounds retain automatic
content-region derivation.

**Region-scoped dispatch (implemented 2026-09-16).** The chain dispatches over
the **region**, not the full canvas: the sub-scene is rendered into a
translated scratch target. The seed and all effect passes use the same dispatch
extent, rounded up to 64 scene-pixel steps and capped at the canvas size. The
padding participates in the shader's coordinates and only the original region
is composited back. The seed, ping-pong textures and shader extent match for each chain. Cost of a scope
now scales with its region instead of the canvas. Scratch targets are
quantized to 64-px steps and cached per size, so animated regions never
reallocate in steady state (PF-7 intent preserved; effects whose semantics
assume scene-centred coordinates — none today — would need an origin-aware
`EffectContext` first). Without `bounds`, the region is derived from content
bounds (below) and the same region-scoped path applies; only a region that
covers the whole scene falls back to the historical full-canvas textures.

**Derived ROI (implemented).** When no `bounds:` is authored, the region is
derived from the content bounds the sub-scene evaluation itself records
(`precise_bounds`): union the content subtree's world bounds, pad by the chain's
worst-case support, and clamp to the scene. The bounds are exact for the frame
being rendered — no staleness, no heuristics — and the 64-px quantization means
a region that grows or shrinks between frames rarely reallocates.
`Blur.radius` animating simply widens the region automatically.

Two constraints:

- **Allocation stability.** An animating support would resize the target every
  frame. The region uses the *worst-case* support over the effect track's
  parameter range, and region scratch targets are quantized to 64-px steps and
  cached per size, so animated regions stay in steady state. Only a region that
  covers the whole scene allocates canvas-sized textures; the PF-7/PF-9 budget
  is preserved.
- **Composite ordering.** Inline Filter scopes enter the scene as GPU textures
  at their declaration position. Tail scopes composite after the base scene.
  Both paths keep pixel data on the device.

> **Effect-scope child labels are scene-global.** Give every child of every
> Filter or Glass scope a unique label (`img_blur`, `img_grade`, …). Reusing a
> child label such as `img` across two scopes re-parents the single shared actor
> into the later scope; the earlier scope silently renders empty. `animatix check`
> reports `duplicate-label`.

## 4. GPU pass contract

Built-in effects run as ordered full-screen fragment passes on every renderer
target. The WGSL compute bindings and dispatch dimensions below describe the
native plugin ABI. Both paths apply effects to straight-alpha RGBA and keep
premultiplied textures at the frame-compositing boundary.

### 4.1 Textures

- Format: `Rgba8Unorm` for both input and output.
- Alpha representation: **straight (unassociated) RGBA**, matching Vello's
  separated-alpha output and the image/composite input paths. A shader that
  composites layers must multiply RGB by alpha for the composition, then divide
  by the resulting alpha before storing; store transparent black when that
  alpha is zero. `DropShadow` and `Bloom` follow this rule for translucent
  content and coloured penumbras. Bloom keeps its existing coverage rule:
  `alpha = clamp(original.a + running.a * intensity * gate, 0, 1)`; `keep`
  scales the original's colour contribution, while threshold tests the running
  result's straight RGB luma.
- Neighbourhood filtering also premultiplies **each source texel before
  interpolation**, then averages premultiplied RGB and alpha with the same
  weights. `Blur` stores the normalized Gaussian coverage and recovers straight
  RGB by dividing the weighted colour by coverage. `MotionBlur` recovers colour
  from the sampled mean coverage, then retains its existing output coverage
  `max(center.a, mean.a)`. A zero-alpha neighbourhood stores transparent black;
  identity/sub-pixel passthrough paths remain exact copies. See the
  [spatial alpha follow-up](history/2026-11-16-spatial-alpha.md).
- Input usage: `TEXTURE_BINDING`; output usage: `STORAGE_BINDING`.
- The host owns the ping-pong textures. A shader must never assume whether it
  read or wrote a given physical texture; `binding 0` is always the input view
  and `binding 1` always the output view for that pass.
- **Two input textures as of ABI v2.** Binding 5 is the *pre-chain* original of
  the scope, copied by the host before pass 0 runs, so a pass can add light back
  onto what the scene looked like rather than onto whatever the previous pass
  left. `Bloom` is its first consumer (`Blur` then `Bloom` in one scope is a
  glow; `Bloom` alone is an exposure lift). v1 had one input and no second
  reserved — the bump was made deliberately, with its cost measured: one extra
  full-region copy per scope per frame, and one extra texture per scratch set.

### 4.2 Bind group 0

| Binding | Type | Owner |
|---|---|---|
| 0 | `texture_2d<f32>` | host (input) |
| 1 | `texture_storage_2d<rgba8unorm, write>` | host (output) |
| 2 | `var<uniform>` — author parameters | effect schema |
| 3 | `var<uniform> EffectContext` | host |
| 4 | `sampler` (linear, clamp-to-edge) | host |

| 5 | `texture_2d<f32>` | host (pre-chain original, ABI v2) |

Bindings 4 and 5 are always present in the layout — 4 so sub-pixel effects
(chromatic aberration, motion blur) are expressible, 5 so a pass can read the
untouched frame. A shader that does not need one simply does not declare it: a
bind group layout may bind more than a pipeline reads, which is what made the
ABI bump additive rather than breaking for the thirteen effects already written
against v1.

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
- The uniform buffer is padded to a multiple of 16 bytes. Built-in parameter
  offsets/sizes derive from the contract table's kinds (§1); an effect whose
  WGSL struct is hand-padded beyond its parameters overrides the derived size
  (e.g. `ColorGrade` packs its five scalars into four `vec4` rows = 64 bytes).
  Plugin effects use the generic layout declared in their parameter schema.
- Repeated instances of an effect share its compute pipelines, but each instance
  has its own aligned slot in the author-uniform buffer. Binding 2 uses a dynamic
  offset, followed by binding 3's context offset. This is host-side storage;
  shader declarations and the plugin parameter ABI are unchanged.

### 4.5 Dispatch

- `@workgroup_size(16, 16)`, entry point `main`, dispatch
  `(width.div_ceil(16), height.div_ceil(16), 1)`.
- Out-of-range invocations are the shader's responsibility to guard.

### 4.6 Synchronisation

A chain encodes its alpha conversion, original-input copy when required,
ordered effect passes and final premultiplication in one command encoder.
Each effect instance and pass has its own uniform slot, so later parameters
cannot overwrite values used by earlier passes.

The [GPU interaction report](history/2026-11-15-gpu-interactions.md) records the
separate-versus-combined submission experiments for the retired Compute renderer.

A chain may dispatch at most 64 effect passes (the per-pass context-buffer slot
count, `MAX_CHAIN_PASSES`). A larger plan is rejected with a render error before
any pass is submitted; it is never truncated.

### 4.7 The backdrop seed (`Glass`)

Glass starts from the rendered backdrop. Once the base scene is on the GPU
target, `FrameGpu` copies the scope's padded bounds into a cleared scratch
texture and applies the same effect chain as Filter. It samples the panel's
window within the padded result and clips the composite to the rounded panel.
The glass children composite immediately above the panel. Each additional
Glass scope samples the result of the earlier backdrop composites.

## 5. Failure policy

- **No GPU backend:** effects are skipped, the content children render
  unfiltered, and a `RenderFailure` diagnostic is pushed. Uniform for built-ins
  and plugins — there is no CPU fallback.
- **Pipeline creation/validation failure:** surface a render error from the GPU
  backend.
- **Unsupported parameter value:** log a warning and treat the parameter as its
  identity value; never drop silently.
- Arbitrary WGSL has no CPU path, so an effect is defined by its GPU
  implementation only.

## 6. Determinism and caching

- Chain order equals declaration order; pass order equals the descriptor's pass
  list. Neither is ever reordered for optimisation without a documented,
  deterministic rule.
- Effect pipelines are cached by effect identity on each renderer device. Dimension changes recreate
  targets but must not recreate pipelines whose key is unchanged.
- Bind groups are cached by the input/output/original texture views and author
  uniform buffer/size. Dynamic offsets select each instance's parameters and
  each pass's context without creating a new bind group. The cache is bounded
  to 256 entries and cleared when author buffers grow or region scratch resets.

**Adjacent caches outside the effect pipeline.** Two other per-frame caches share
the same staleness hazard, and both are deliberately bypassed rather than keyed:

- The shape-command memo is keyed on `(epoch, style, state)`. A `dash_offset` or
  a gradient ramp that animates would be served stale from it, so dash and
  gradient paints are *stamped onto freshly built commands* instead of being
  folded into the key. Scenes that author neither take the memo fast path
  untouched.
- The static-subtree cache holds a whole encoded scene per root. An actor that
  carries dash, a gradient or a `blend:` always has keyframes, so it is excluded
  from that cache by the existing rule rather than needing its own carve-out.

**Per-actor `blend:` is not the effect pipeline.** It is a Vello node-level
`push_layer` in `evaluate_node`, so it composites the actor's whole subtree
against whatever is behind it in the scene, with no offscreen texture and no GPU
pass. The layer rect is the subtree's world-space bounds union when that is
**provably safe** (a leaf/identity-transform subtree, no camera/mask/effect scope
inside, no child overflowing its parent) — which covers the house-style
`StageFloor` glow ellipses on 40+ scenes — and falls back to the historical
full-surface rect otherwise; a wrong clip would be visible, so the fallback is
deliberately conservative (`docs/history/2026-11.md`). Remaining limits: `blend`
on `Filter`/`Mask` scopes is untested, and there is no per-actor `Compose` control
(only `Mix` names are parsed). A backdrop blur is not part of the effect pipeline
either: it is the `Glass` scope, whose chain runs on a copy of the live render
target rather than on the scope's own sub-scene (§4.7), so no chain input can
ever express it — see `docs/spec.md` ("Glass").

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

1. Create `crates/animatix-core/src/builtins/effects/<name>.rs` implementing the `Effect`
   trait — parameters (name, kind, identity), WGSL passes, `pack`, and
   `support`. This file is the single declaration of the contract.
2. Add the module and its `pub static <NAME>` to
   `crates/animatix-core/src/builtins/effects/mod.rs`, then reference it from
   `EFFECTS` in `crates/animatix-core/src/builtins/mod.rs` — that is the
   registration. Effect dispatch and the plugin registry are in
   `crates/animatix-core/src/builtins/registry.rs`. There is no enum
   variant to add (`EffectId` is the authored name), and no separate contract
   row: the parser crate derives `effect_specs()` from `EFFECTS`.
3. Document it here (the identity table in §2) and in `docs/spec.md`.
4. If the effect reads a neighbourhood or displaces samples, calibrate
   `support` — the derived ROI pads by it, and an under-reported support clips
   pixels silently. If the WGSL uniform struct is hand-padded beyond the
   parameters, override `author_uniform_size` with the struct's real size
   (`ColorGrade` does this).

## 9. Editor support

Author effect stages and parameter animation in `.amx`. The workbench previews
the result and offers read-only inspection; its insertion palette and inspector
write-back are disabled. The analyzer validates effect parameter names and types
in declarations and `scope.stage.param` assignments. See the
[GUI workbench](gui-workbench.md) for the editing boundary.
