# effects-wave1 — notes

Dogfood pass for the wave-1 built-in effects (`Sharpen`, `Vignette`,
`MotionBlur`, `Grain`, `Levels`) over a real brief (dark-mode keynote intro
reel). Render-verified at t = 0.3, 1.6, 3.5, 5.0, 8.5.

## Engine bugs found and fixed (this pass)

1. **`offset` timed assignment was a silent no-op with a misleading warning.**
   `card.offset = (0, 0) [1.4s]` fell through to the generic property engine,
   which maps `offset` to `PositionBindingGroup` and emits "Cannot set
   'position' directly" without moving anything. The special-case position
   handler only matched `position | at`. Fixed by handling `offset` there: it
   rewrites the offset component of the *current* position binding (anchor and
   percent bases keep their base; absolute/container bindings warn
   `IgnoredOffset`, matching the declaration path). Verified: centroid moves
   by exactly the authored offset, no warning.

2. **`Image` ignored its position anchor when drawing.** The image command drew
   its box from the actor origin (top-left at `at`/`anchor`), so
   `Image, anchor: scene.center` put the image's *top-left* at the scene
   center — visible in the shipped `examples/animation/08_effects.amx` too.
   Fixed by centering the draw box on the local origin
   (`primitives/image.rs`) and aligning the two local-bounds recorders with it.

3. **Region-scoped effect compositing leaked stale GPU pixels.** With a derived
   or authored ROI, the compute shaders got `tex_size = region` but sampled the
   full-canvas ping-pong textures: every sample outside the cropped seed read
   stale texels from the *previous* scope's render, and `MotionBlur`'s
   `max(alpha)` dragged that garbage into the frame (opaque checker/black boxes
   around a moving card). Fixed by always dispatching the chain over the full
   canvas and scoping only the readback/composite to the region. The dispatch
   saving described in `docs/effects.md` §3 is deferred until the
   `EffectContext` gains an origin-aware ABI (roadmap follow-up).

4. **Leftover `[roi-dbg]` eprintln** in the derived-ROI path (also a
   `println!`-rule violation) — removed.

## Authoring findings (not bugs, but traps)

- **`anchor: scene.*` on a child pins it to the scene, escaping every
  container.** Inside a `Filter`/`Stack` with an offset, `anchor: scene.center`
  silently ignores the scope's placement. The idiomatic placement for content
  that rides a scope is no binding at all (defaults to the scope origin) or
  `at: (x, y)` (scope-local). Cost ~30 min of this pass; a lint
  ("scene anchor inside a layout container/scope") would have caught it.
- **`offset` on children of a layout container is ignored** — the
  `AbsolutePositionOnLayoutManagedChild` diagnostic covers `at`/`position` but
  not `offset`, and `transform` is the documented escape hatch. The diagnostic
  should mention `offset` too.
- **`Stack` overlaps children around a shared origin**; `Col`/`Row` flow, and
  container-managed children need `transform` (not `offset`) for visual
  nudges. All three cost a render-review cycle each.

## Effect-specific observations

- `MotionBlur` `max(alpha)` keeps silhouettes readable over moving backdrops —
  correct for card fly-ins.
- `Grain` at 0.12–0.35 with `monochrome: true` reads as film grain without
  mangling text; the timeline-driven frame index animates it for free.
- `Vignette` radius/softness are fractions of the half minimum dimension, so
  the same values read identically at 720p and 1080p.
- `Levels` `in_black: 0.06 + gamma: 0.85` is a convincing day→night shift
  combined with `Vignette` amount 0.55 → 0.8.

## Open: video export drops the full-canvas backdrop scope

`animatix image` renders every beat of this project correctly, but `animatix
video` loses the whole backdrop scope (checker + grain + vignette + levels) in
**all** frames, while the card scope's MotionBlur composite renders fine.
Repro: `animatix video dogfood/projects/effects-wave1/entry.amx --fps 30 -o
out.mp4` (needs the `video` feature). Probe evidence: the backdrop's filter
readback returns an all-transparent texture on most frames while the card's
region readback has content. Single-GPU-scope scenes export fine, so the
trigger is ≥2 GPU filter scopes per frame in the pipelined export path.
Tracked in `roadmap.md` (Known Issues).
