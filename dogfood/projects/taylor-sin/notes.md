# Taylor-Sin Dogfood Notes

Status: first pass render-verified 2026-09-06 (`animatix image` at 9 global
times, all five scenes). The content works, but getting there surfaced **two
silent engine bugs in the plot pipeline**, several spec↔runtime drifts, and a
cluster of expression-language gaps that make math explainers harder to write
than they should be.

## What worked

- Multi-scene `play` chain (5 scenes, `fade` transitions), scene-local `t`,
  keyframe-derived durations — no surprises.
- `Typst` labels/equations (`frac`, `sum_(k=0)^n`, `dots.h`), timed
  `opacity` assignments on Typst labels, single-line math with plain `/`.
- `stroke_progress` trace-on **when the curve itself is revealed** (see gap 1).
- Graph math-coordinate mapping for curves; `text.muted` + explicit
  `opacity: 0.35` as the dimmed-backdrop idiom inside a Graph.
- `always` + `format("n = {}", n)` live readout; `if` expression for the
  accent flip at n = 13.
- The `step()`-gated partial sum (gap 4 workaround) renders correctly and the
  degree sweep n = 1 → 13 visibly hugs `sin`.

## Gaps found (ordered by severity)

### 1. Container `fade-in` does not reveal Graph-hosted PlotCurve children (silent) — RESOLVED 2026-09-06

Fixed in the same pass (probe `010-graph-child-reveal`): container entrance
actions now cascade the reveal into children, and a child without any
entrance warns `never-revealed`. The Target scene no longer needs the
per-curve `fade-in sine` line. `examples/data/07_plots.amx` renders its
headline sine again. Historical text below.

Minimal repro (`dogfood` convention):

```animatix
g: Graph, x_domain: (-pi, pi), y_domain: (-1.8, 1.8), size: (400, 300), anchor: scene.center {
  c: PlotCurve, kind: "cartesian", func: (x) => sin(x), color: accent.primary, stroke_width: 4
}
#0.3s
fade-in g [300ms]
```

At any time after 0.6s the axes render, the curve never does, and **no
diagnostic fires** (no `never-revealed`, no warning). Pre-keyframe declarations
are hidden by default; `fade-in <container>` lifts the container's flag but
never walks into Graph children. Fading the curve itself (`fade-in c`) works.

**Shipped-example impact:** `examples/data/07_plots.amx` — its headline
"View 1" sine (inside `main: Graph`, revealed via `fade-in main`) is invisible
for the entire scene (pixel-verified at t = 3.0 and t = 6.0; the VectorField and
Heatmap, which are top-level actors, reveal fine).

**Workaround used here:** `fade-in sine_graph` + `fade-in sine` separately.
Also note the twin quirk: explicit `opacity: 0` on a Graph-hosted plot
declaration is honored (bypasses hidden-by-default), which is why the
PartialSums scene's assignment-driven reveal works while the Target scene's
did not — inconsistent semantics for the same "start invisible" intent.

### 2. Spec §14 "Runtime parameters" pattern never re-samples the curve — RESOLVED 2026-09-06

Fixed (probe `011`): the dynamic gate now accounts for captures written by
`always` blocks; the spec §14 example animates as written. Historical text below.

Spec §14 documents:

```animatix
#0s
let freq = 2
curve: PlotCurve, kind: "cartesian", func: (x) => sin(freq * x), ...
always {
  freq = 2 + 3 * sin(t * 0.5)  // sweep frequency over time
}
```

Pixel-verified static: t = 0.3 and t = 5.0 render **identical** curves. Root
cause direction: `ProceduralPlot::is_dynamic()` (plot.rs:1132) is
`func_body.references_ident("t") || !param_names.is_empty()` — a closure
capturing a `let` variable is classified static, the cached build-time
`vector_paths` are reused forever, and the frame-env shadowing machinery
(scene_eval.rs:522 onward) is never reached. Roadmap line "plot sampling lets
frame values shadow build-time closure captures" is therefore dead code on
this path.

**Verified working spellings / broken variants:**

| Spelling | Animates? |
|---|---|
| `t` referenced inside the closure body (e.g. `func: (x) => sin((2 + 3*sin(t*0.5)) * x)`) | ✅ |
| `let freq = 2` + `always { freq = ... }` (spec §14 as written) | ❌ static |
| Declared param `freq: 2` + `always { freq = ... }` | ❌ static (same shape as freq=2) |

**Workaround used here:** inline the `t`-expression into the closure body and
gate terms with `step()`. Readable, but the "reactive knob" vocabulary the spec
advertises is not the one that works.

### 3. Timed plot-param assignment (`curve.freq = 5 [1s]`) resamples but renders a wrong curve — RESOLVED 2026-09-06

Not a param-track bug: the adaptive samplers' 8-sample floor aliased the
16-period wave into a straight line (a plain `sin(5x)` reproduced it).
`resolution` is now honored as a minimum sample count (probe `012`).
Historical text below.

Probe: `curve: PlotCurve, func: (x) => sin(freq * x), freq: 2` +
`#0.5s curve.freq = 5 [1s]`. At t = 5.0 (well after the 1.5s completion) the
render is a nearly-flat gently-sloped line — neither `sin(2x)`, nor `sin(5x)`,
nor a plausible blend of the two. Something in the param-track keyframe path
(assignments/mod.rs:629–683) feeds the sampler a wrong `freq`. Analyzer also
flags the documented declaration form as `unknown-property: freq ... (may
still be valid)` — the runtime feature and the analyzer's common-property
table disagree.

### 4. Expression language can't build a series: no `sum`, no closure locals, no user-fn calls in closures

Writing Sₙ(x) = x − x³/3! + … requires:

- every term spelled out (no `sum(list, f)`, no `factorial`);
- the degree knob **inlined 6×** into the closure body (no `let` inside a
  closure — parse error `expected expression, found 'let'`; block bodies are
  rejected);
- `step()` gates instead of an indexed loop.

For a 7-term series this is tedious-but-possible; for anything longer
(general orthogonal polynomials, numerical experiments) it is a wall. A
`sum(expr, k, k0, k1)`-style folder, closure-local `let`, or callable pure
`fn` inside plot closures would remove the entire workaround.

### 5. `format()` has no precision control — RESOLVED 2026-09-06

`format("y = {:.2}", v)` now works on both eval paths (unrecognized specs
stay literal, surplus `{}` keep braces). Historical text below.

`eval_format` (eval_shared.rs:394) replaces `{}` with Rust `Display`: an
animated float readout prints `0.7071067811865476`. Math explainers need
`{:.2}`-style specifiers (or a `round_to(x, n)` builtin; current workaround
`floor(x * 100) / 100` is lossy at trailing zeros and negative values).

### 6. Unlabeled actors inside a Graph hit the reserved-label check

Declaring the Sweep curve without a label generated `__anon_sw_graph_1` and
the build rejected it: `error[build:reserved-label-prefix]`. The engine's own
generated name trips the engine's reserved-prefix rule, so the anonymous-child
syntax (shown for `Row` in spec §8) is unusable inside a Graph. Labeled +
`// lint-disable: unused-label` is the working spelling.

### 7. Col auto-width propagation wraps Typst math labels badly

Four Typst labels inside a `Col` wrapped mid-formula (`x - x³⏎6`): the
container-to-child `text_max_width` propagation measured a narrower box than
the inline fractions need. Workaround: absolutely positioned labels.

### 8. `highlight … [intensity: …]` is silently ignored — RESOLVED 2026-09-06

Action-modifier validation now warns `UnsupportedModifierKey` for keys the
action does not declare; the `intensity` call sites were removed from
gradient_descent.amx. Historical text below.

`examples/projects/gradient_descent.amx` uses `highlight ball [600ms,
intensity: 1.3]`. `Highlight`'s ActionSignature declares only
`color/blend/padding/radius`; timing.rs:563 whitelists `intensity` for *any*
action without checking the action declares it. The example, the spec's
highlight signature table, and the runtime disagree three ways, and the
stray key vanishes silently (the "never silently drop values" rule has a
modifier-shaped hole).

### 9. Spec §14 stroke_progress example contradicts §3 hiding rules

Spec §14 shows `signal: PlotCurve, …, stroke_progress: 0` then
`signal.stroke_progress = 1 [1.5s]` with no entrance action. As a pre-keyframe
declaration the actor is hidden by default and renders **black forever** (the
build even warns `never-revealed`). The example needs a `fade-in signal` (as
`gradient_descent` actually does) or an in-keyframe declaration.

## Minor

- `always`-writes-`color` on an actor with any keyframed property emits
  `always-overrides-keyframes` even when that property has no keyframes — the
  canonical reactive-status-label idiom (06_reactive) ships with this warning
  noise.
- Sweep scene's Graph shows numeric tick labels while the visually identical
  PartialSums Graph shows none (didn't chase; cosmetic inconsistency).
- `engine.map_inverse` extrapolates outside the padded plot area (documented),
  worth remembering for marker-follows-cursor effects.

## Follow-up outcome (2026-09-06 fix pass)

Landed on `feat/dogfood-fix-pass` (all merged gates green, 26 suites):

1. Gap 1 → `26b4ca46` (probe 010 resolved; 07_plots renders its headline
   sine again; 23_plot_kinds fixed with a container fade-in).
2. Gap 2 → `291ae9ff` (probe 011 resolved; spec §14 animates as written).
3. Gap 3 → `aab8ced9` (probe 012's real cause: sampler resolution floor).
4. Gap 5 → `606f8fd7` (`{:.N}` on both eval paths).
5. Gap 8 → `e7bdbecb` (modifier validation; example call sites cleaned).
6. Spec §14/§8 corrections → `b597a48e`.

Still open: gap 4 (`sum`/`factorial`/closure-local `let` — roadmap LG-1,
design-gated), the `__anon_` reserved-prefix self-collision (roadmap Known
Issues, documented in spec §8 for now), analyzer's `unknown-property` info
for declared plot params, and the `always-overrides-keyframes` warning noise
on the reactive readout idiom.

## Verification

```bash
cargo run --bin animatix -- check dogfood/projects/taylor-sin/entry.amx
# key frames: Title 1.5 · Target mid-draw 5.0 · PartialSums 9.0/10.5/12.5 ·
# Sweep n=1/5/13 → 14.0/15.5/17.5 · Outro 20.0
cargo run --bin animatix -- image dogfood/projects/taylor-sin/entry.amx --time 15.5 --output /tmp/taylor-n5.png
```

Remaining known warning: `always-overrides-keyframes` on `readout.color`
(intentional reactive styling, see Minor).
