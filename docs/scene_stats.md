# Scene Statistics (STAT-1) — Design Note

Status: **proposal, not started**. Not yet tracked in `roadmap.md`; a row is the
first landing step. Related: `series_construction.md` (LG-1, the registration-route
precedent), `effects.md` (why this is not an effect), `history.md` ("M1 — light/color
vocabulary", the recipe-not-mechanism decision).

## Motivation

Scenes cannot currently see themselves. Anything that should *react to the
composition* — an ambient backdrop that brightens where the action is, a camera
that settles when the scene gets busy, a type size that shrinks as the cast grows —
has to be hand-authored as static values, because there is no derived quantity to
read.

The obvious implementation is a frame-time accumulator, and it is not available:
`Timeline::evaluate` is `&self` on a cloned timeline per export thread
(`scene_eval::evaluate_program_inner`, `crates/animatix/src/timeline/scene_eval.rs:2013`),
and a frame can be served by `restore_frame_cache` (`scene_eval.rs:1913`) **without
recomputing anything**. Anything that persists between frames is therefore
order-dependent and thread-dependent.

This note proposes the smallest thing that still removes the whole workaround
class, built on one observation:

> **A curve is a memory.** If a derived quantity is materialized as a function of
> time at build, then every "smoothing", "delay" and "lookback" an author wants
> becomes a *pure function of `t`* — and the stateless-frame constraint costs
> nothing.

The second observation is why this is cheap rather than merely possible: the baked
signal is **piecewise-linear** and the smoothing kernel is **exponential**, and that
pair has a closed-form convolution per segment. No taps, no history, no accumulator
— an exact one-pole response evaluated analytically. The engine's own
`Easing::Spring { damping, frequency }` is literally `1 - e^{-dt}cos(ft)`
(`crates/animatix-syntax/src/easing.rs:159-166`), i.e. a damped oscillator's step
response. Easing curves in this codebase already *are* filter kernels; STAT-1
exploits the identity instead of adding a filter.

## Scope: three moving parts, and nothing else

| # | Part | Where | Surface added |
|---|---|---|---|
| 1 | **Bake pass** — sample scalar stats over time, simplify, inject as values | new `timeline/scene_stats.rs`, called from `build/entry.rs` beside the Legend scan | none (data only) |
| 2 | **Two builtins** — `curve_at(c, t)`, `curve_smooth(c, t, tau)` | `eval_shared::eval_builtin_fn` fast path | 2 function names |
| 3 | **Ambience recipe** — a library component that reads the stats | `examples/lib/ambience.amx` | none |

Explicitly **no** new primitive, property, config key, keyword, `Stmt` variant,
`Value` variant, or GPU pass. The feature is invisible to the grammar and to the
renderer.

## Mechanism selection

### Why build time, and why not a frame-time sampler

The pass shape already exists twice over. `timeline/legend.rs`'s
`scan_legend_entries(&timeline.tracks, &timeline.root_nodes)` is invoked at
`build/entry.rs:904-922` with the comment *"Populate Legend entries after every
actor is built so generated and forward-declared actors participate in the same
scan"* — a post-build walk that derives scene facts and stores them. The seamless-loop
check at `build/entry.rs:713-749` proves the second half: build can already sample
any track at any time (`property_has_keyframes`, `read_property_value(track, field, t)`).
STAT-1 is those two capabilities pointed at each other.

A frame-time sampler over all actors was rejected on measurement grounds: it costs
O(actors) per query, it would double-count what `inject_runtime_lookup_values`
(`timeline/frame_env.rs:228`) already pays per frame, and — decisively — the exact
same quantity sampled at build is *free* at frame time. The one thing frame-time
sampling would buy is *rendered* geometry (real ink bounds rather than declared
boxes), and that is unavailable for the whole scene anyway: `precise_bounds` is
cleared at `scene_eval.rs:2063` and filled by the root loop, i.e. **after** `always`
runs. See "Non-goals" for the bound this imposes.

### Registration route: fast path

`series_construction.md` (LG-1) settled that a builtin needing the caller's
`Environment` registers as a stdlib `NativeFn`, and one that does not takes the fast
path (`eval_shared::eval_builtin_fn`, called from both the tree-walker and the IR's
`CallBuiltin` arm, with the IR triple). `curve_at`/`curve_smooth` take the curve
**as an argument**, so they need no environment, no name registry, and no string
lookup — hence the fast path, like `list_swap`/`list_set`/`sum`. This is what keeps
the design general: any list of numbers is a curve, including one the author built
themselves.

### The value: a curve is a flat `List` of numbers

`Value::List(Arc<[Value]>)` (`timeline/env.rs:158`) is already shared-by-refcount for
exactly this shape — its doc comment names an FFT sample array captured by a plot
closure, deep-cloned per sample point until the `Arc` landed. Curves reuse it as a
flat interleaved pair list:

```
scene.stats.motion = [t0, v0, t1, v1, ...]   // t ascending, seconds; t0 == 0, tN == duration
```

`Num` only, so `sum`/`list_set` keep working and the sampler is a binary search over
`f64` returning `Value::Num` — no allocation beyond the refcount bump. Vec2-valued
stats are two scalar curves (`focus_x`, `focus_y`), not a list of pairs, so one
encoding covers everything.

Keys are injected as **literal dotted strings**, which is how `scene.center` already
works: `Expr::Path` tries the joined dotted key in the environment first
(`timeline/utils.rs:387`) and only then falls back to walking `Value::Object` fields
(`:391-410`). So `scene.stats.motion` needs no `Value::Object`, no type-layer field
resolution, and no nesting machinery.

### Injection point and gating

The bake writes into `timeline.env` **before** `env_base` is frozen
(`build/entry.rs:856`), so a curve reaches every frame through `Environment::with_base`
as an immutable base-layer key — readable in `always` with zero per-frame work, the
same route the `scene.{anchor}` keys take.

Gating follows `has_effect_scopes` / `refresh_blend_used` / `refresh_camera_used`
(`build/entry.rs:933-936`): a build-time flag `stats_used`, set when any expression
textually references `scene.stats`, guards the whole pass. A scene that never asks
about itself pays nothing anywhere — not at build, not at frame time. (Companion
requirement: `build::referenced_roots`, which filters per-frame property injection by
textual reference, `timeline/mod.rs:633-638`, must not mistake `scene.stats` for an
actor label. Verify against how `scene.center` is currently treated in the analyzer's
`symbol_table.rs`.)

## Semantics

### Statistics (v1 set)

Every entry is a pure function of authored tracks sampled at `t`. `a_i` = actor `i`,
`p_i(t)` = `geometry.position`, `s_i(t)` = `geometry.size`, `o_i(t)` = effective
opacity, `canvas` = `config { resolution }`.

| Key | Definition | Reads as |
|---|---|---|
| `scene.stats.motion` | `Σ ‖p_i'(t)‖ · s_i.w · s_i.h · o_i / (canvas · duration)` | how much is happening |
| `scene.stats.ink` | `Σ s_i.w · s_i.h · o_i / canvas_area` | how much is on screen |
| `scene.stats.focus_x` / `.focus_y` | `(s·o)`-weighted centroid of `p_i` | where it's happening |
| `scene.stats.spread_x` / `.spread_y` | weighted RMS distance from `focus` | how concentrated |
| `scene.stats.cast` | count of `a_i` visible at `t` | how many |

Norm/derivative conventions: `p_i'` is the exact slope of the position segment
containing `t` (tracks are piecewise, so the derivative is available without finite
differencing), and `s_i · o_i` weighting makes an off-screen or unrevealed actor
contribute zero, matching intent.

Two exclusions inherited from existing code, both load-bearing:

- **Full-viewport backgrounds do not count.** Reuse `legend.rs`'s
  `is_full_viewport_background` predicate, the same rule that stops a `size: fill`
  plate from polluting a legend (`spec.md` "Legend": *"Full-viewport background shapes
  using `fill` or `100%` sizing are excluded automatically"*). Without it, one
  full-screen rect makes `spread` and `ink` constant and the whole feature inert.
- **`solo` is not honoured at build.** The bake sees the authored scene; `solo` is
  resolved per frame (`resolve_solo_state`). Documented limit, not a bug.

### Builtins

```
curve_at(c: List<Num>, t: Num) -> Num
```
Binary search for the bracketing pair, linear interpolate. `t <= t0` → `v0`,
`t >= tN` → `vN` (clamped, never extrapolated — an extrapolated `focus` can leave the
canvas and put a wash off-screen with no visible cause). Empty or 1-element curve →
that value; a malformed odd-length list is a `TypeMismatch`, never a silent drop
(`AGENTS.md`, "Never silently drop values").

```
curve_smooth(c: List<Num>, t: Num, tau: Num) -> Num
```
Exact one-pole exponential response at `t`, evaluated analytically over the segments
inside `[t - W, t]` where `W = 5·tau` (99.3% of the step response):

    y(t) = (1/tau) ∫_{t-W}^{t} e^{-(t-u)/tau} · c(u) du

With `c(u) = A + B·u` linear on a segment, each term is
`A(1-e^{-Δ/tau}) - A·tau·(...)  + B·[...e^{-Δ/tau}(...)]` — precompute the closed form
once, loop over ≤ `W`-spanning segments. `tau <= 0` → `curve_at(c, t)` exactly (identity,
so `tau` can be animated through zero without a branch in author code). `tau` larger
than the scene → the whole-window mean.

This is the answer to "how does easing work under stateless `always`": it does not
*approximate* an IIR filter with taps, it **is** the filter's response, computed from
the signal rather than accumulated into a register. Nothing persists between frames.

Compositions authors get for free, all pure:

| Want | Write |
|---|---|
| low-pass follow | `curve_smooth(c, t, 0.6)` |
| 2-pole / springier settle | `curve_smooth(curve_smooth(c, t, 0.4), t, 0.4)` |
| delayed copy, for staggered ambience layers | `curve_at(c, t - 0.4)` |
| "is it increasing?" | `curve_at(c, t) - curve_at(c, t - 0.25)` |
| two response speeds, blended | `lerp(curve_at(c, t), curve_smooth(c, t, 0.8), 0.6)` |

### Loop closure

When `config { seamless_loop: true }`, the smoothing window wraps circularly
(`t - W < 0` reads past the scene end) and the bake **pins `v0 == vN`**. Without this,
the window truncation at `t < W` is a startup transient that jumps at the seam —
the same family of defect as the pinned-spring-endpoint note at
`easing.rs:156-158` ("a keyframe that lands 0.2% off its target moves the resting
composition"). For non-loop scenes the window clamps at `t0` and the transient is the
correct behaviour.

The baked curves are then lintable, which closes the one hole the existing check has:
the seamless-loop pass samples *keyframed properties* and its comment at
`build/entry.rs:708-712` is explicit that `always`-computed values are invisible to it.
Baked curves are data, so add them to that walk: compare `curve_at(c, 0)` against
`curve_at(c, end_ms)`. Absent this, a scene whose ambience jumps at the seam fails
invisibly — which is precisely how the next `history.md` entry gets written.

### Bake policy

- **Adaptive stride**: `P = clamp(round(duration_s · 8), 32, 256)` sample points.
  Stats are low-frequency by construction; the goal is a build cost that does not grow
  with scene length.
- **Simplify**: Ramer–Douglas–Peucker at `epsilon` proportional to the signal range
  (start at 0.5%). A smoothed scalar typically collapses from 256 samples to tens of
  points, so stored curves are a few KB for the whole set, and linear interpolation
  between surviving points is *more* accurate than the raw stride.
- **Order of operations**: sample raw → smooth → simplify → pin loop endpoints. The
  RDP pass after smoothing is what makes the point count small; simplifying first
  would distort the response.
- **Determinism**: iteration is over `BTreeMap` tracks (`timeline/mod.rs:516` — the
  key order is the determinism guarantee), so bake output is byte-identical across
  runs and threads.

## Cost

| Phase | Cost | Notes |
|---|---|---|
| build | `O(P × N_actors)` samples + `O(P)` RDP | `P ≤ 256`. 100 actors → ~25k position/size samples, a few ms at worst. This is on the **web player's pre-first-frame path**, so it must be measured, not assumed |
| frame env | 6 extra base-layer keys | No per-frame injection work: base layer, frozen `env_base` |
| frame time | `O(log P)` per query, no allocation | Only paid if authored |
| GPU | **0** | Deliberate: the visuals stay authored primitives, so they ride the main vello render rather than adding a scope |
| scenes not using it | 0 everywhere | `stats_used` gate |

The zero-GPU column is the argument against the alternative framing of "background
shading" as an effect or a container scope. A full-canvas `Filter` scope would be one
more serialized render/harvest round-trip — and the measured floor says that cost is
not fill-rate: halving resolution bought +15% (`roadmap.md:150`), the 13-scope
catalog is 63–71 ms with the chain itself only ~12 ms. Worse, a background is by
definition *first* in z-order, and `can_post_composite_filter` (`scene_eval.rs:380`,
call site `:2290`) permits the zero-readback path only for the last rendered element,
so a background scope takes the inline readback path every frame.

## The consumer: a recipe, not a mechanism

Ambience ships as `examples/lib/ambience.amx`, in the lineage of `examples/lib/slide.amx`
— the deliberate choice recorded in `history.md`: the three background recipes were
catalogued and then landed as *primitives + builtins + recipes*, "product forms of the
light vocabulary"; the particle item closed as *"no new primitive, no state model"*
(`history.md:1925`) and count-up as *"Deliverable is a recipe + example"*
(`history.md:1901`). STAT-1 supplies the one thing genuinely missing from that
decision — a way to read the scene — and stops there.

Illustrative shape, using only what exists today plus the two builtins:

```
# ambience.amx — authored-explicit, reads the scene, invents no actors
config { resolution: (1280, 720), duration: 12 }

plate: Rect, size: fill, color: scene.background, anchor: scene.center, opacity: 1

wash: Ellipse, size: (900, 900), opacity: 0.18, blend: "screen",
  fill_gradient: radial((0.5, 0.5), 0.5, {(0%, "#f5b94233"), (100%, "#f5b94200")})

always
  let e   = curve_smooth(scene.stats.motion, t, 0.7)
  let fx  = curve_smooth(scene.stats.focus_x, t, 1.2)
  let sx  = curve_smooth(scene.stats.spread_x, t, 1.0)
  wash.at       = (fx + 180 * sin(τ * t / 6), scene_height * 0.5)
  wash.size     = (600 + 900 * sx, 600 + 900 * sx)
  wash.opacity  = clamp(0.06 + 0.5 * e, 0.02, 0.30)
  plate.color   = lerp_color("#0d0f14", "#141821", clamp(1.5 * e, 0, 1))
end
```

`opacity` on `wash` is **not** decorative. An actor declared before the first keyframe
with no explicit `opacity` is seeded `opacity: 0` and stays invisible until an entrance
lifts it (`AGENTS.md`, "Hidden by default"), so a component whose entire job is to be
behind everything must state that it is visible. A recipe that omits it renders
nothing and looks correct in source.

## Non-goals

| Rejected | Why | Reopen when |
|---|---|---|
| Density **grid** / `density_at(x, y)` | Needs `List<List<Num>>`, a spatial sampler, and a size-2 encoding decision, for marginal gain over `focus` + `spread` (which describe the same content as two Gaussians) | A real layout needs more than one hotspot |
| `Ambience` **primitive** | Every visual it would emit is already expressible as `Rect`/`Ellipse` + `fill_gradient` + `blend`; a primitive adds a catalog row, an identity card, an applicability row, and a spec table | The recipe set grows a shared parameter vocabulary worth pinning |
| Ambience as **parent container** of the cast | A parent evaluates before its children, so `TargetResolver::target_bounds` (`timeline/callout_geometry.rs:34-77`) can only reach its 2nd/3rd fallback — declared size × world affine — never the `precise_bounds` that reflect text/path/image ink. And full-viewport-under-content compositing is documented as absent: *"there is no way to put content underneath it"* (`roadmap.md:167-170`) | Compositing gains a below-scope pass |
| Frame-time **stat accumulation** (`prev = lerp(prev, now, k)`) | Impossible, not merely discouraged: cache hits skip recomputation (`scene_eval.rs:1913`) and export threads start mid-timeline. `curve_smooth` provides the identical response for free | Never — this is the constraint the design is built on |
| Rendered-ink statistics (pixel/luminance) | Readback hard-errors on wasm and is the recorded perf landmine (`history.md:776-789`) | A GPU-side reduction with no CPU round-trip exists |
| `stat("name")` string lookup | Would need the env route and a name registry; passing the curve as an argument is strictly more general (author-built curves work too) | — |

## Landing order

1. `roadmap.md`: add STAT-1 under Planned Features (it is remaining work, so it belongs
   there and nowhere else).
2. `timeline/scene_stats.rs`: the bake (`stat_fn` table, stride, smoothing, RDP, loop
   pin) + unit tests asserting **purity**: `bake(t)` is identical across two `Timeline`
   clones and independent of query order.
3. Wire into `build/entry.rs` beside the Legend scan; freeze-gate on `stats_used`;
   extend the `seamless_loop` walk with the pin check.
4. `curve_at` + `curve_smooth` on the fast path, with the full LG-1 triple
   (`eval_builtin_fn`, `BuiltinFn` IR variant, `builtin_for_name`, the enum→name arm in
   `ir/eval.rs` which has no wildcard so the compiler enumerates it), plus
   `animatix-syntax`'s builtin declaration list for the parser/analyzer.
5. `examples/lib/ambience.amx` + `spec.md`/`properties.md`/`primitives.md` as applicable,
   and a tour scene that makes reactivity visible (the feature is invisible in a static
   frame — which is also why it needs a rendered check, not just a build check).

### Gates

`cargo test -p animatix-syntax` · `cargo test -p animatix --lib -- --test-threads=1` ·
`ir_tests.rs` AST↔IR parity for both builtins · `cargo clippy -p animatix
--no-default-features --all-targets -- -D warnings` (slim is what the web build uses) ·
`cargo test -p animatix-web --test site_scenes` **both profiles** if a site scene
adopts the recipe · `scripts/perf-bench.sh compare` in the commit message
(`AGENTS.md`: hot-path rule — this touches `always` evaluation) · a build-time
measurement for the web first-frame path, which `perf-bench.sh` does not cover.

### Risks

- **Build cost lands in the browser.** The bake runs before the web player's first
  frame. If `P × N` is wrong for a large scene, the symptom is a slow-appearing
  embed, invisible to every desktop gate. Mitigate with the `P` clamp and a
  build-side number in the commit message.
- **Approximation vs authored geometry.** `motion`/`ink`/`focus` describe *declared*
  boxes. A camera zoom, a layout container's reflow, or a plot's sampled path will all
  move real ink without moving the stats. This is a documented limit of the whole
  feature, not a per-scene bug — say so wherever the stats are documented, or the first
  person to use them with `camera.zoom` will file it as one.
- **Ordering coupling.** If a stat ever influences `background_color` while `Legend`
  samples `background_color` for contrast (`primitives/legend.rs`'s luminance rule),
  there is a feedback loop through two post-build passes. Pin the invariant now:
  the bake reads authored tracks only, and `Legend` continues to sample *declared*
  colours, never derived state.
- **Naming.** `auto` already carries three unrelated meanings here (`color: auto`,
  `strategy: auto`, `gap: auto`). `scene.stats.*` is a fresh namespace; keep it that
  way and do not overload `auto`.
