# Probe 010 — Effect chains, derived ROI, and plugin effects (dogfood)

Round: post-implementation dogfood of the effect-chain abstraction
(`docs/effects.md`, roadmap "Post-Processing Effect Abstraction").
Scenarios render via `animatix image` at multiple timestamps; findings below
were pixel-quantified with ImageMagick, not eyeballed.

## Scenarios

| File | Exercises | Result |
|---|---|---|
| `derived_roi.amx` | Blur+ChromaticAberration chain, derived ROI (no `bounds:`) | ✅ backdrop/card/badge all preserved (pixel-quantified); chroma fringe visible on card edge |
| `authored_bounds.amx` | Explicit `bounds: (x,y,w,h)` with content straddling the region | ✅ inside processed (blurred), outside sharp — composite-at-origin visible as designed |
| `animated_chain.amx` | Keyframed radius/offset/saturate over derived ROI | ✅ 0.5s≈4.0s (chain back at start values), 2.0s blurred+desaturated |
| `enabled_toggle.amx` | `enabled: false` decl + keyframed toggle | ⚠️ toggle works (sharp↔blurred confirmed) but text position is unstable — see Finding 2 |
| `plugin_pixelate` (via `examples/projects/plugin_pulse.amx` + `--plugin`) | Manifest-declared `Pixelate` effect on a Filter scope | ✅ checker stamp visibly pixelated by the plugin effect at size 12 |

## Findings

### Finding 1 (fixed during dogfood) — analyzer missed `enabled` on built-in effects
`soft.enabled = true` produced "Effect 'Blur' has no parameter 'enabled'" info
diagnostics: the built-in effect seeding in `SymbolTable::build_from_ast` did
not include the implicit `enabled` toggle. Fixed by seeding it (mirroring the
manifest `apply_to` path).

### Finding 2 (open, container-layout semantics) — child position flips between the processed and full-scene paths
`enabled_toggle.amx`: `target: Text, at: (0, -40)` inside
`fx: Filter, at: (320, 180)`. With the chain ACTIVE (blur sampling) the text
renders container-relative — center (320,140), matching `Filter center + at`.
With the chain EMPTY (all stages disabled/identity → the full-scene
`scene.append(sub_scene)` fast path) the SAME text renders at the scene's
bottom-right corner (~(542,316)-(639,359) sharp).

The two paths disagree on the child coordinate space when the container has an
explicit `at`: children are evaluated with the filter's world transform baked
into the sub-scene, but the append fast path and the processed path appear to
compose the container transform differently for anchor/at-less children.
Minimal repro: `dogfood/probes/010-effect-chains/enabled_toggle.amx` at
t=0.5 (empty chain) vs t=2.0 (active chain). Bisect artifacts: a stage with
`enabled: true` static renders centered (like `bisect_t5`); adding ONLY the
`soft.enabled` keyframes relocates the text (the stage's `has_any_keyframes`
flips the frame-cache/invalidation funnel, which changes which child-eval path
supplies the position).

Not effect-blocker: the effect output itself is correct in both paths; the
divergence is the container/child coordinate-space contract. Filed for the
container-layout workstream (together with the anchored-children-inside-
positioned-containers semantics).

### Finding 3 (open, language semantics) — `at` on containers is parent-center-relative
`fx: Filter, at: (320, 180)` in a 640x360 scene positions the scope at the
scene's bottom-right corner (at is an offset from the parent center for
containers, and the root scene's "center" base makes the doubled offset land
off-frame). Documented here so probe authors know: container `at` is not
absolute scene position. The offscreen integration tests pin absolute
positions via explicit root actor `at` instead.

## Verdict
Effect-chain features (chain order, derived ROI, authored bounds, enabled
toggle, plugin effects, keyframed parameters) render correctly across all
probes. Findings 2/3 are container-positioning semantics, tracked separately
from the effect work.
