# Feasibility Assessment & Implementation Plan — Roadmap #1: Plot Function Transitions

## 0. Executive Summary

**Verdict: Practical (Partial).** Animating `PlotCurve.func` between two functions is feasible with **Strategy A (output blending)** integrated into the existing per-frame `ProceduralPlot` resampling path. The approach generalizes cleanly to all four `kind`s (cartesian, polar, parametric, implicit) because each blends the function *output* rather than sampled geometry. Kind *changes* (cartesian→polar) and implicit plots are deferred. Complexity is Medium; ~3–5 days for the core, plus testing/docs.

---

## 1. Feasibility Verdict

| Dimension | Assessment | Rationale |
|---|---|---|
| Practical? | **Yes (partial scope)** | Output blending fits the existing `ProceduralPlot` per-frame model; no new runtime architecture needed. |
| Complexity | **Medium** | Sampling refactor is mechanical but touches ~250 lines across 4 sampling functions + implicit; assignment/build wiring is non-trivial because `func` is a build-time-only AST node, not a registry-animated value. |
| Time | **3–5 days** core, +1–2 days tests/docs | See task breakdown (§3). |
| Risk | **Medium** | Main risks: (a) static→procedural promotion losing parent-graph domain context, (b) per-frame double-evaluation cost, (c) interaction with the existing morph/path-cache machinery. All mitigable (§5). |

### Assumptions (must hold)
- A `func` transition occurs on a **single `PlotCurve` actor** whose `kind` does not change — both the "from" and "to" closures share the same `kind` and arity. *(Verified: `kind` and domains are properties of the PlotCurve, not the func; both funcs evaluate over the same domain.)*
- The existing `ProceduralPlot` per-frame resampling path (`scene_eval.rs:394–426`) is the correct integration point. *(Verified: it already overrides `vector_paths` each frame for dynamic plots.)*
- `evaluate_expr` over the AST is the evaluation mechanism (no compiled-closure fast path exists yet). *(Verified: `sample_recursive_*` call `evaluate_expr(body, env)` directly.)*

### Unverified claims (flag for maintainer)
- That always creating a `ProceduralPlot` for static `PlotCurve`s has negligible memory cost. (Likely true — it's an AST clone + a few f64s — but unmeasured.)
- That double function evaluation per sample point is acceptable at 30/60fps for typical plots. (Bounded by adaptive sampling; see §5 performance.)

---

## 2. Recommended Approach

### Strategy: **A — output blending** (with Strategy B explicitly rejected for MVP)

**Why A over B (path morphing):**

| Criterion | Strategy A (output blend) | Strategy B (path morph) |
|---|---|---|
| Math semantics | Blends actual `f(x)`→`g(x)`: a sine smoothly becoming a cosine. **Correct & intuitive.** | Blends sampled *points* in screen space: sin→cos slides points sideways (looks like translation, not transformation). Misleading. |
| Adaptive sampling | Re-subdivides the *blended* curve — detail appears where the blend needs it. | Two independently-sampled paths have mismatched point counts/density; `align_segments` splits/merges → artifacts. |
| Discontinuities (NaN/asymptotes) | `lerp(NaN, y, p)` = NaN → stroke breaks naturally. Per-function. | NaN in either path breaks subpath alignment; morph draws spurious connectors. |
| Generalizes to all `kind`s | **Yes, uniformly:** cartesian/polar blend scalar `r`/`y`; parametric blends `Vec2`; implicit blends the scalar *field* then runs marching squares. Elegant. | Path morph works on final BezPaths so it *could* cross kinds — but that's out of scope and visually dubious. |
| Reuses existing code | Reuses `ProceduralPlot` + sampling; refactors sampling to accept a blended-func abstraction. | Reuses `morph.rs` fully — but procedural plots **bypass** `evaluate_vector_paths` morph (`scene_eval.rs:426` overwrites `vector_paths`), so B would need a new per-frame morph call anyway. Less reuse than it appears. |
| Perf (per frame) | ~2× `evaluate_expr` calls during transition window. | 2 full samples + `morph_paths` alignment (O(n) with splitting). Comparable; A is simpler. |

**Decision: Strategy A.** It is mathematically correct, generalizes to all kinds via one uniform refactor, and integrates with the existing per-frame resampling rather than fighting it.

### Scope

**In scope (MVP):**
- `func` assignment on `PlotCurve`: `curve.func = (x) => cos(x) [1s, ease: ease-in-out]`
- All four `kind`s, **provided `kind` is identical** for from/to (same arity & return type).
- Output blending with the assignment's `easing`.
- Multiple sequential func transitions on one plot.

**Out of scope (explicitly):**
- **Kind changes** (cartesian→polar, etc.). Forbidden with a build diagnostic. (Strategy B would be the only route; deferred.)
- **Implicit plots** in the first cut. The scalar-field blend is conceptually clean, but marching-squares interaction with a moving zero-contour needs visual validation. Recommend a follow-up task after cartesian/polar/parametric ship. *(If the maintainer prefers, implicit can be included — the code path is isolated — but I recommend excluding it from the first PR to bound risk.)*
- `func` transitions on `VectorField` / `Heatmap` / `ContourSet` (they also have `func`). Different rendering; separate effort.
- Cross-fade (opacity) as an alternative blend mode. (The existing `MorphStrategy::Fade` is path-based and doesn't apply here; output blending subsumes the common case.)

---

## 3. Implementation Plan

Tasks are fixer-sized, ordered by dependency. Each names files/functions.

### Task 1 — Add transition data model
**Files:** `crates/animatix/src/timeline/plot.rs`, `crates/animatix/src/timeline/dispatch.rs`
- In `plot.rs`, add:
  ```rust
  /// One keyframe-driven transition between two function bodies.
  #[derive(Clone, Debug)]
  pub struct FuncTransition {
      pub start_ms: u64,
      pub end_ms: u64,
      pub easing: Easing,
      pub from_args: Vec<String>,
      pub from_body: Expr,
      pub to_args: Vec<String>,
      pub to_body: Expr,
  }
  ```
  and extend `ProceduralPlot` with `pub is_dynamic: bool` (precomputed `func_body.references_ident("t") || !params.is_empty()`).
- In `dispatch.rs` `AnimationTrack`, add:
  ```rust
  pub func_transitions: Vec<FuncTransition>,
  ```
  init to `Vec::new()` in `AnimationTrack::new`. Include it in `max_keyframe_time()` and `has_any_keyframes()` scans (mirror the `plot_param_tracks` loop).
- Add a helper `FuncTransition::active_at(time_ms) -> Option<(progress, &from, &to, easing)>` and `current_func_for(time_ms) -> (&args, &Expr)` (last completed transition's `to`, else the declaration func).
- **Verify:** `cargo check -p animatix`.
- **Effort:** ~1h.

### Task 2 — Refactor sampling to accept a blended-func abstraction
**Files:** `crates/animatix/src/timeline/plot.rs`
- Introduce:
  ```rust
  pub(crate) enum PlotFuncRef<'a> {
      Single(&'a Expr),
      Blended { from: &'a Expr, to: &'a Expr, t: f64 },
  }
  ```
- Add per-kind evaluation helpers that close over `PlotFuncRef` and the cache, e.g.:
  ```rust
  fn eval_scalar(func: &PlotFuncRef, env, arg, x) -> f64  // cartesian: y ; polar: r
  fn eval_vec2(func: &PlotFuncRef, env, arg, t) -> [f64;2] // parametric
  ```
  For `Blended`, evaluate both bodies and `lerp`. **Cache key must distinguish from/to** (e.g. cache as two `HashMap<u64,Value>`, or fold a `0/1` tag into the key) so a blended sample doesn't return the wrong body's cached value.
- Change signatures of `sample_recursive_cartesian`, `sample_recursive_polar`, `sample_recursive_parametric` from `body: &Expr` → `func: &PlotFuncRef<'_>`. Bodies are mechanical: replace `evaluate_expr(body, env)` with the helper. Keep visibility/jump/NaN logic unchanged (it operates on the blended result).
- Update `build_plot_curve_paths` (`build/plot.rs`) to wrap its single `body` in `PlotFuncRef::Single` — keeps the static build path working unchanged.
- **Verify:** `cargo test -p animatix` (existing plot tests must pass — they exercise the `Single` path).
- **Effort:** ~2–3h (mechanical, but 3 functions × ~60 lines).

### Task 3 — Blend in `sample_procedural_plot`
**Files:** `crates/animatix/src/timeline/plot.rs`
- Change signature:
  ```rust
  pub fn sample_procedural_plot(plot: &ProceduralPlot, env: &mut Environment, time_ms: u64, transitions: &[FuncTransition]) -> Vec<VelloPath>
  ```
- Resolve the active func: if `transitions.active_at(time_ms)` → `PlotFuncRef::Blended { from, to, t: eased_progress }`; else `PlotFuncRef::Single(current_func_for(time_ms))`.
- Pass the resolved `PlotFuncRef` into the (refactored) sampling functions. The implicit branch: for MVP, **error/skip** if a transition targets an implicit plot — emit a `tracing::warn!` and fall back to `Single` (or, if implicit is in scope per maintainer, blend the scalar field in `evaluate_implicit_value`).
- **Verify:** unit test — build a `ProceduralPlot` with a transition sin→cos, assert `sample_procedural_plot` at progress 0.5 yields points ≈ `(sin(x)+cos(x))/2`.
- **Effort:** ~1.5h.

### Task 4 — Always create `ProceduralPlot` for `PlotCurve`; guard per-frame resample
**Files:** `crates/animatix/src/timeline/build/plot.rs`, `crates/animatix/src/timeline/scene_eval.rs`
- In `process_plot_actor` (build/plot.rs ~line 1071): drop the `if body.references_ident("t") || !plot_params.is_empty()` gate for `PlotCurve` — always construct `procedural_plot` when `func` exists. Set `is_dynamic` from the existing check. **Keep the static `plot_path_cache` + `vector_paths` track population unchanged** (still used when not transitioning).
- In `scene_eval.rs` (~line 394–426), restructure:
  ```rust
  let mut vector_paths = track.evaluate_vector_paths(time_ms); // cached/static + morph
  if let Some(pp) = track.procedural_plot.as_ref() {
      let transitioning = !track.func_transitions.is_empty()
          && track.func_transitions.iter().any(|t| time_ms >= t.start_ms && time_ms <= t.end_ms);
      if pp.is_dynamic || transitioning {
          // inject params (existing code) …
          vector_paths = sample_procedural_plot(pp, &mut local_env, time_ms, &track.func_transitions);
      }
  }
  ```
  Static, non-transitioning plots keep using the cached `vector_paths` (no perf regression).
- **Verify:** `cargo test -p animatix`; confirm static plot examples (`07_plots.amx`) still render identically.
- **Effort:** ~1.5h.

### Task 5 — Handle `func` assignment (build-time transition setup)
**Files:** `crates/animatix/src/timeline/assignments.rs`
- In the assignment handler, **before** the "is plot param" branch (~line 402), add a special case:
  ```rust
  if property == "func" && track.kind == ActorKindId::PlotCurve {
      // 1. Evaluate the RHS to Value::Closure(args, body).
      // 2. Determine "from": the to_body of the last transition with start_ms < t_start,
      //    else the declaration func (track.procedural_plot.func_body).
      // 3. Validate: same kind, same arity, same return type (reuse the existing
      //    InvalidPlotFunc diagnostic pattern from build/plot.rs).
      // 4. Push FuncTransition { start_ms: t_start, end_ms: t_end, easing,
      //    from_args, from_body, to_args, to_body }.
      // 5. If the plot had no procedural_plot (legacy static), this is now impossible
      //    because Task 4 always creates one — but assert it exists.
      return; // func is not a registry property; do not fall through.
  }
  ```
- **Kind-change guard:** if `kind` of from ≠ to (inferred from arity/return-type check), push an `InvalidPlotFunc` error: *"func transition must keep the same kind and arity"*.
- **Mid-transition reassignment edge case (MVP):** if a new `func` assignment starts before the previous one ends, snap: set the previous transition's `end_ms = t_start` (so blend completes at the snap point) and chain. Document this behavior.
- **Verify:** integration test — `curve.func = (x) => cos(x) [1s]` produces a `FuncTransition` on the track with correct from/to.
- **Effort:** ~2h.

### Task 6 — Tests
**Files:** `crates/animatix/src/timeline/tests/` (new `plot_transitions.rs`), `examples/`
- Unit: blending correctness at p=0/0.5/1 for cartesian (scalar), parametric (Vec2); NaN propagation (one func `1/x`, blend across x=0 stays broken).
- Build: diagnostic for kind-mismatch / arity-mismatch func assignment.
- Integration `.amx` example (`examples/24_plot_transitions.amx`): sin→cos→x² sequence; also a parametric morph (circle→Lissajous).
- Regression: existing `07_plots.amx`, `23_plot_kinds.amx`, `fft_explain.amx` render unchanged (static plots unaffected).
- **Effort:** ~2h.

### Task 7 — Docs
**Files:** `docs/spec.md` (~line 1231 PlotCurve section), `docs/primitives.md`, `docs/properties.md` (line 106 `func` row), `docs/roadmap.md` (remove item #1)
- Document syntax, the same-kind constraint, that `func` uses output-blending (not path morph), and that `morph:`/`MorphOptions` do not apply to func transitions.
- **Effort:** ~1h.

### Dependency order
1 → 2 → 3 → (4 ‖ 5) → 6 → 7. Tasks 4 and 5 are independent once 1–3 land.

---

## 4. Design Decisions Needed

### 4.1 Syntax
**Recommend:** standard assignment, consistent with every other animatable property:
```amx
#0s
curve: PlotCurve, kind: "cartesian", func: (x) => sin(x),
  stroke: accent.primary, stroke_width: 3

#2s
curve.func = (x) => cos(x) [1s, ease: ease-in-out]

#4s
curve.func = (x) => x^2 / 10 [800ms, ease: ease-in-out]
```
Rationale: matches the `label.prop = value [duration, ease: …]` pattern users already know (e.g. `main.size = …` in `07_plots.amx`). No new keyword. The `func` registry entry stays `BuildTimeOnly`/`F::empty()` — assignment is handled by a special-case branch (exactly as `plot_param_tracks` already is in `assignments.rs:402`), NOT by promoting `func` to a generic `ASSIGNABLE` property (closures can't implement `Interpolate` and have no `PropertyValue` variant).

**Rejected:** a dedicated `transition` action/block (`curve.transition func to: …`) — inconsistent, more grammar work, no benefit.

### 4.2 `func` as animatable property? **No.**
`func` is an AST node (`Expr::Closure`). `Interpolate` requires blending two `Self` values into one `Self`; there is no meaningful "interpolated closure." The transition is a *sampling-time* concern, not a value-track concern. Keep `func` build-time-only in the registry; model transitions as a side-channel (`func_transitions`) parallel to `plot_param_tracks`. This is the established pattern.

### 4.3 Domain mismatches
**Not a problem** for same-actor transitions: both funcs evaluate over the `PlotCurve`'s own `x_domain`/`t_domain` (a property of the actor, not the func). No intersection/union logic needed. If one func is undefined (NaN) on part of the domain, the blend is NaN there → stroke breaks, which is correct (the discontinuity "fades in"). Document this.

### 4.4 Kind changes
**Forbid** (MVP). Emit `InvalidPlotFunc` if from/to arity or return type differ. Rationale: blending a scalar `y` with a `Vec2` `(x,y)` is type-erroneous; kind change is a Strategy-B (path morph) problem with poor visual guarantees. Revisit only if a real use case appears.

---

## 5. Risks & Edge Cases

| Risk | Impact | Mitigation |
|---|---|---|
| **Static→procedural promotion loses parent-graph domain context** (parent label not stored on track). | Build error / wrong domain. | **Avoid** by always creating `ProceduralPlot` at declaration (Task 4), where `parent_label` is in scope and `p_x_domain`/`p_size` are read from `self.env` (`{parent}_x_domain`). Assignment handler (Task 5) only *adds* a transition to the existing plot — never reconstructs. |
| **Per-frame double evaluation cost** during transition. | Frame drops on dense/complex plots. | Bounded by adaptive sampling + visibility culling (already present). Outside the transition window, static plots still use the cached path (zero added cost). Can lower `max_depth`/raise `tolerance` during transitions if needed (future). Icebox item "pre-compiled plot closures" would help. |
| **Cache contamination** — blended sampling must not return the wrong body's cached value for a given `x`. | Wrong curve. | Tag cache entries by body (from=0/to=1) or use two caches. (Task 2.) |
| **Procedural plots bypass morph** — `scene_eval.rs:426` overwrites `vector_paths`, so `MorphOptions`/`morph:` modifier has no effect on a func transition. | User confusion if they set `morph: fade`. | Document: func transitions use output blending; `morph:` is ignored for them. (Could emit a build hint.) |
| **`stroke_progress` interaction** — `PlotCurvePrimitive::evaluate` trims the *blended* path by `stroke_progress`. Probably fine (trim happens post-sample). | Low. | Add a test: func transition + `stroke_progress` animate together. |
| **Mid-transition reassignment** (`#2s func=A[1s]`, `#2.5s func=B[1s]`). | Ugly jump if naive. | Snap-and-chain (Task 5): truncate prior transition's end to the new start. Document. |
| **Backward compat** — always creating `ProceduralPlot` changes `has_procedural_plots()`/`needs_frame_env()` (`mod.rs:934`), possibly enabling frame-env for formerly-static plot scenes. | Minor perf: frame_env built where it wasn't. | Guard `needs_frame_env` on `is_dynamic || has_func_transitions`, not merely `procedural_plot.is_some()`. (Task 4 — check `frame_env.rs:96`.) |
| **`func` referencing `t` during a transition** — both from/to may reference `t`. | Works naturally (per-frame env has `t`); just costs 2 evals. | None needed; add a test. |

### Testing strategy
- **Unit** (pure sampling): blend math correctness, NaN propagation, parametric Vec2 blend, boundary p=0/p=1 identity.
- **Build** (diagnostics): arity/kind mismatch, assignment on non-PlotCurve, missing prior declaration.
- **Integration** (`.amx` → rendered scene): example file; assert no panics and key sample points match expected blend. Reuse the `scene_eval.rs` test harness.
- **Regression**: `07_plots.amx`, `23_plot_kinds.amx`, `fft_explain.amx`, `gradient_descent.amx` byte-identical output (static plots unchanged).

---

## 6. Alternative Approaches

1. **Strategy B (path morph) per-frame.** Sample both funcs each frame, `morph_paths` the two BezPaths. Rejected (§2): worse math semantics, alignment artifacts, and it doesn't actually reuse the morph infra cleanly (procedural plots bypass `evaluate_vector_paths`). *Could* be revived later **only** for kind-change transitions, which Strategy A cannot express.

2. **User-level workaround (no new feature).** Users can already animate a *parameter* inside one func: `func: (x) => lerp(sin(x), cos(x), k)`, `curve.k = 0 → 1`. This delivers ~80% of the value today with zero code. **Recommend documenting this pattern** regardless — it's the honest MVP and may reduce pressure on the feature. Limitation: requires the user to write the blend manually and know both funcs up front; no easing per-segment unless they hand-roll it.

3. **Promote `func` to a real `ASSIGNABLE` ValueType::Closure with step-interpolation.** Would make `func` snap at t=0.5 (like `String`). Rejected: a *snap* is not a "transition" — users expect smooth blending, which only Strategy A provides. The special-case assignment handler (Task 5) is strictly more capable.

4. **Domain warping** (the roadmap's third suggested strategy). Morph the *input* parameter: `x' = lerp(x, g⁻¹∘f(x), p)`. Powerful but requires invertibility and is overkill. Not recommended.

### Smallest useful MVP
If 3–5 days is too much: **cartesian-only, single transition, no `t`-referencing funcs.** Drop Tasks for polar/parametric/implicit (keep `Single`-only path for them → they error on `func` assignment). This is ~1.5 days and still covers the headline use case (sin→cos). But the sampling refactor (Task 2) makes all kinds cheap, so I recommend doing them together.

---

## 7. Open Questions for Maintainer

1. **Implicit plots in scope?** The scalar-field blend is clean to code but visually needs validation (moving zero-contour). Include in first PR, or defer? *(My recommendation: defer; ship cartesian/polar/parametric first.)*
2. **`morph:` modifier on func transitions** — silently ignore, or emit a build diagnostic hinting it's unsupported? *(Recommend: ignore + doc note.)*
3. **Mid-transition reassignment** — snap-and-chain (my proposal), or forbid (error if a new func assignment starts before the prior ends)? *(Recommend: snap-and-chain, documented.)*
4. **Is the ~2× per-frame eval cost during transitions acceptable**, or should I add an adaptive quality downgrade (lower `max_depth` during transition windows) from day one? *(Recommend: ship without; measure; add only if profiling demands.)*
5. **`func` transitions on `VectorField`/`Heatmap`/`ContourSet`** — same `func` property, different renderers. Out of scope here; confirm this is a separate follow-up. *(Assumed yes.)*
6. **Should the existing user-level workaround (§6.2) be documented now** as the interim solution, independent of this feature's timeline? *(Recommend: yes, low-cost, high-value.)*

---

## Files to touch (summary)

- `crates/animatix/src/timeline/plot.rs` — `FuncTransition`, `PlotFuncRef`, `ProceduralPlot.is_dynamic`, refactor 3 `sample_recursive_*` + `sample_procedural_plot` signature. (Tasks 1–3)
- `crates/animatix/src/timeline/dispatch.rs` — `AnimationTrack.func_transitions`; `max_keyframe_time`/`has_any_keyframes` scans. (Task 1)
- `crates/animatix/src/timeline/build/plot.rs` — always-create `ProceduralPlot`; `build_plot_curve_paths` uses `PlotFuncRef::Single`. (Tasks 2, 4)
- `crates/animatix/src/timeline/scene_eval.rs` — guard resample on `is_dynamic || transitioning`; pass transitions to `sample_procedural_plot`. (Task 4)
- `crates/animatix/src/timeline/assignments.rs` — `func` assignment special case → push `FuncTransition`; kind/arity validation. (Task 5)
- `crates/animatix/src/timeline/frame_env.rs` / `mod.rs` — guard `needs_frame_env` so static plots with a (non-active) procedural_plot don't force frame-env. (Task 4)
- `crates/animatix/src/timeline/tests/plot_transitions.rs` (new) — tests. (Task 6)
- `examples/24_plot_transitions.amx` (new). (Task 6)
- `docs/spec.md`, `docs/primitives.md`, `docs/properties.md`, `docs/roadmap.md`. (Task 7)

**No parser changes** — `func` assignment already parses via the existing `label.prop = expr` grammar; closures already parse as `Expr::Closure`. The feature is build/runtime only.
