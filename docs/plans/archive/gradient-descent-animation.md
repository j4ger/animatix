# Educational .amx Animation: Topic Selection & Design

## Goal
Pick a pedagogically valuable topic that showcases Animatix DSL strengths (plots, reactive `always`, multi-scene, morphing) and is distinct from `fft_explain.amx` (signal processing). Design it scene-by-scene.

---

## Part 1 — Five Candidate Topics

### 1. Gradient Descent on a Loss Surface
Visualize a ball rolling down a 2D loss surface's contour map to the minimum, tracing its path.
- **DSL features:** `ContourSet` (loss contours), `VectorField` (gradient arrows), `PlotCurve` parametric (descent trail), `always` (analytical spiral ball position + live loss label), `Graph` math→screen mapping, multi-scene, `stroke_progress` draw-in.
- **Pedagogy:** foundational ML/optimization — "follow −∇f to minimize loss," plus learning-rate tradeoffs.

### 2. Binary Search
Animate binary search over a sorted array: highlight the middle element, discard the eliminated half, narrow the window.
- **DSL features:** `Row` of `Rect` bars with `Text` labels, `highlight`/color assignments, `opacity` fade-out of eliminated halves, multi-scene per step, `stagger` entrance.
- **Pedagogy:** classic O(log n) algorithm; clear divide-and-conquer visualization.

### 3. Conic Sections
Show a plane slicing a cone at varying angles, morphing the cross-section into circle → ellipse → parabola → hyperbola, each paired with its polar equation curve.
- **DSL features:** `Polygon`/`Path` morphing via re-declaration (`strategy: auto/fade`), polar `PlotCurve` for each conic, multi-scene per conic, `Typst` equations.
- **Pedagogy:** unified geometric theory of conics; the morphing makes the family relationship visceral.

### 4. Neural Network Forward Pass
Build a small MLP (input → hidden → output) node-and-edge diagram, then animate activation values propagating layer by layer.
- **DSL features:** `Ellipse` nodes, `Arrow`/`Line` connections, `stagger` for layered reveal, `opacity`/`color` flow for activations, `Group`/`Col` layout, `scale` pulse on active nodes.
- **Pedagogy:** how signals flow through layers; weighted sums → activation.

### 5. Derivative as Slope (Tangent Line)
A point slides along a curve y=f(x); the tangent line at that point is drawn, its slope = f'(x) shown live; sweep to reveal the derivative function.
- **DSL features:** cartesian `PlotCurve` (f and f'), `Line` (tangent) driven by `always` analytical position, `Arrow` for slope vector, `Graph` mapping, `stroke_progress` to "draw" the derivative curve as the point sweeps.
- **Pedagogy:** the derivative as the limit of slopes; geometric meaning of f'.

---

## Part 2 — Best Pick: Gradient Descent

**Why it wins on all five criteria:**

| Criterion | Gradient Descent | Runner-up (Binary Search) |
|---|---|---|
| Visually clear | Iconic "ball rolling down a bowl" — instantly readable | Clear, but bar-array is plainer |
| Pedagogically valuable | Foundational to all of ML/optimization | Strong, but narrower scope |
| Self-contained (3-6 scenes, 20-40s) | 6 scenes, ~35s | 5 scenes, ~30s |
| **DSL strengths** | **Maximal** — uniquely stacks `ContourSet` + `VectorField` + parametric `PlotCurve` + `always` + `Graph` mapping + multi-scene + `stroke_progress`. No other candidate hits this many first-class primitives. | Moderate — containers + highlight, but no plotting/fields/reactive |
| Not FFT-similar | Optimization domain; visually nothing like waveforms | Fully distinct |

It is the single topic that exercises the **plotting/field primitives** (the DSL's most distinctive capability beyond what fft_explain already used) **plus** the **reactive `always`** system **plus** multi-scene — while remaining honest and accurate.

### Design constraint handled
`always` is stateless, so iterative gradient descent cannot be simulated frame-to-frame. **Solution:** drive the ball with an **analytical logarithmic spiral** `r(s)=r₀·e^(−k·s)`, `θ(s)=θ₀+ω·s` — which is exactly the trajectory of gradient descent with momentum on a quadratic bowl, and is a clean closed-form function of time `t`. The same formula feeds both the ball position (`always`) and the trail `PlotCurve` (parametric, drawn via `stroke_progress`), keeping them perfectly synced. Loss `= r²` is computed from the same `s` for a live readout — no state needed.

---

## Part 3 — Detailed Design: "Gradient Descent: Rolling Down the Loss"

**Global config:** `colorscheme: "editorial-dark"`, `resolution: (1280, 720)`.
**Shared math frame:** one `Graph`, `x_domain: (-4, 4)`, `y_domain: (-4, 4)`, `size: (640, 640)`, centered-left. (Per example 23 caveat, keep all plot curves in this single Graph.)
**Loss function:** `f(x, y) = x² + y²` → concentric circular contours, gradient `∇f = (2x, 2y)` pointing radially outward (uphill).
**Descent spiral constants:** start `(x₀, y₀) = (3, 2.4)` → `r₀ ≈ 3.84`, `θ₀ = atan2(2.4, 3) ≈ 0.676`; `k = 0.55` (decay), `ω = 2.6` (rotation), `T = 4.5s` (descent duration).

### Scene 1 — TitleCard  (~3.0s)
**Actors:** `title` (Text, "Gradient Descent"), `subtitle` (Text, "Rolling down the loss surface").
- `#0.5s` fade-in title [800ms, ease-out]; `#1.4s` fade-in subtitle [600ms].
- `play LossSurface [fade, 400ms]`.

### Scene 2 — LossSurface  (~6.5s)  — "1. The Loss Surface"
**Actors (inside `Graph`):**
- `contours: ContourSet`, `func: (x,y) => x^2 + y^2`, `levels: {1, 4, 9, 16, 25}`, `color: accent.primary`.
- `field: VectorField`, `func: (x,y) => (x, y)` (gradient, outward), `density: 10`, `color: accent.muted`.
- `min_dot: Ellipse` at `(0,0)` (the minimum), small, `color: accent.success`.
**Side text:** `step_title` ("1. The Loss Surface"), `caption` ("contours = equal loss · arrows = uphill gradient").
- `#0.5s` fade-in step_title.
- `#1.0s` stagger [150ms] { fade-in contours; fade-in field; fade-in min_dot }.
- `#3.0s` highlight min_dot [pulse] to flag the target.
- `play GradientDirection [wipe-left, 400ms]`.

### Scene 3 — GradientDirection  (~6.0s)  — "2. Go Against the Gradient"
**Actors:** re-establish a static ball `ball: Ellipse` at start `(3, 2.4)`; `up_arrow: Arrow` from ball along `+∇f` (outward, `color: accent.danger`, label "∇f — uphill"); `down_arrow: Arrow` from ball along `−∇f` (inward, `color: accent.success`, label "−∇f — descend"). `Typst` equation `$delta f = (partial_x f, partial_y f)$`.
- `#0.5s` fade-in ball, step_title.
- `#1.5s` draw-in up_arrow [800ms]; `#2.0s` fade-in ∇f equation.
- `#3.2s` draw-in down_arrow [800ms]; pulse ball.
- `play Descent [fade, 400ms]`.

### Scene 4 — Descent  (~9.0s)  — "3. Roll to the Minimum"  ★ centerpiece
**Actors (inside `Graph`):**
- `contours` (same as Scene 2, dimmed `opacity: 0.4`).
- `trail: PlotCurve`, `kind: "parametric"`, `func: (s) => (3.84 * exp(-0.55 * s) * cos(0.676 + 2.6 * s), 3.84 * exp(-0.55 * s) * sin(0.676 + 2.6 * s))`, `stroke_progress: 0`, `color: accent.warning`, `stroke_width: 3`.
- `ball: Ellipse`, size `(16,16)`, `color: accent.warning`.
- `live_loss: Text` (top-right), `live_pos: Text`.
**Reactive `always` block** (drives ball + readouts from the same spiral formula, synced to `trail`'s `stroke_progress`):
```
always {
  let s = clamp(t - 1.0, 0.0, 4.5)        // descent starts at t=1.0
  let r = 3.84 * exp(-0.55 * s)
  let th = 0.676 + 2.6 * s
  ball.at = (r * cos(th), r * sin(th))    // math coords → auto-mapped by Graph
  let loss = r * r
  live_loss.text = format("loss = {loss:.3f}", loss)
  live_pos.text  = format("({x:.2f}, {y:.2f})", x = r * cos(th), y = r * sin(th))
}
```
- `#0.5s` fade-in contours (dim), step_title.
- `#1.0s` fade-in ball; `trail.stroke_progress = 1.0 [4.5s, ease: ease-out]` (trail draws in exactly as ball travels — same `s` window).
- `#1.0s` fade-in live_loss / live_pos.
- `#5.7s` (ball reaches center) pulse ball [intensity: 1.4]; `min_dot` (re-shown) pulse.
- `#6.5s` fade-in takeaway text "Follow −∇f → reach the minimum."
- `play LearningRate [fade, 400ms]`.

### Scene 5 — LearningRate  (~7.0s)  — "4. Step Size Matters"
**Actors (inside `Graph`, contours dimmed):** three overlaid parametric `PlotCurve`s from the same start, different `k`/`ω`:
- `too_small`: `k=0.12` — short, slow spiral barely moving (`color: text.muted`, label "too small → slow").
- `just_right`: `k=0.55` — clean spiral into center (`color: accent.success`, label "just right").
- `too_big`: `k=-0.2` (negative decay → `r` grows) — diverges outward (`color: accent.danger`, label "too big → diverges").
- `#0.5s` fade-in step_title.
- `#1.0s` stagger [400ms] { draw-in too_small; draw-in just_right; draw-in too_big }.
- `#4.5s` fade-in lesson text "Pick a learning rate that converges — not too small, not too large."
- `play Outro [fade, 400ms]`.

### Scene 6 — Outro  (~3.0s)
**Actors:** `thanks` (Text, "θ* = argmin L(θ)"), `tag` (Text, "gradient descent · animatix").
- `#0.5s` fade-in thanks [700ms]; `#1.0s` fade-in tag.
- `#2.2s` fade-out both [300ms].

### Narrative flow
Title → **what** is a loss surface (contours + gradient field) → **which way** to step (against ∇f) → **watch it converge** (live spiral + loss readout) → **the catch**: step size governs success (three regimes) → summary. Each scene adds one idea; the contour backdrop persists across scenes 2/4/5 as a visual anchor.

### Transitions
fade → wipe-left → fade → fade → fade. The single `wipe-left` (into GradientDirection) signals a shift from "object" to "direction"; the rest are calm `fade`s to keep focus on the math.

### `always` usage summary
- **Scene 4:** ball position + loss/position readouts, all from the closed-form spiral of `t`. Stateless, deterministic, synced to `trail.stroke_progress`.
- (Scenes 2, 3, 5, 6 use only keyframes — no reactive blocks needed.)

---

## Risks / open questions for implementation
- **ContourSet/VectorField as Graph children vs standalone:** example 18 uses them standalone with explicit `at`/`size`/domains; fft example puts `PlotCurve` inside `Graph`. If child placement misrenders, fall back to standalone primitives with matching `x_domain`/`y_domain`/`size`/`at` and convert ball coords to pixels manually in `always` (screen_x = cx + (x / 4.0) * (size/2)).
- **Single-Graph plot-curve caveat (example 23):** keep all `PlotCurve`s of a scene in one Graph. Scene 5's three curves are fine together; do not split across Graphs.
- **`format()` precision syntax:** spec shows `format("y = {x}", x)`; the `{loss:.3f}` precision form should be verified against the runtime before relying on it (fallback: round manually with `floor(loss*1000)/1000`).
- **`clamp`/`exp` availability:** both listed in built-in math — OK.
- **Spiral honesty:** a pure quadratic `f=x²+y²` has radial gradient → straight-line descent. The spiral reads as "descent with momentum / on a rotated quadratic." If strict accuracy for the vanilla case is required, either (a) switch the loss to a rotated quadratic `f = x² + x·y + y²` (genuinely spiral-inducing) and re-derive contours, or (b) relabel the motion as "gradient descent with momentum." Recommend (a) for honesty — contours become rotated ellipses, equally teachable.
