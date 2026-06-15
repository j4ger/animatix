# Curve Trace Animation for PlotCurve

## Goal

Resolve G3 by making `PlotCurve` traceable through the existing `stroke_progress` property first, then optionally adding a small `trace` action as syntax sugar.

## Context

Educational curve animations usually reveal a curve by drawing it from its start parameter to its end parameter. Animatix already has the right conceptual property for this: `stroke_progress`, a normalized `0.0..1.0` stroke reveal value. The missing piece is making `PlotCurve` participate in that path-trimming pipeline.

Today `PlotCurve` is built as one or more stroked `VelloPath`s:

- `crates/animatix/src/timeline/build/plot.rs::build_plot_curve_paths()` samples the closure and returns stroked `VelloPath` values.
- `crates/animatix/src/timeline/build/plot.rs::Timeline::process_plot_actor()` stores those paths in `track.vector_paths`.
- `crates/animatix/src/timeline/scene_eval.rs::evaluate_node()` samples `track.evaluate_vector_paths(time_ms)`, re-samples dynamic `procedural_plot` curves when needed, and passes the paths to `PlotCurvePrimitive::evaluate()`.
- `crates/animatix/src/primitives/plot.rs::PlotCurvePrimitive::evaluate()` currently returns the full `ctx.vector_paths` unchanged.

That means `PlotCurve` already uses the same vector-path storage as shapes and morphing, but the evaluated curve path is not trimmed by `stroke_progress` before rendering.

## Existing Mechanisms

`stroke_progress` is declared in `crates/animatix/src/timeline/property_registry.rs`:

```rust
schema!("stroke_progress", ValueType::F32, F::ASSIGNABLE_AI,
    ActorField::StrokeProgress, None,
    Applicable::AllShapes,
    |_| super::property_engine::PropertyValue::F32(1.0)),
```

It maps to `AnimationTrack::stroke_progress` in `crates/animatix/src/timeline/track.rs`, so it can be keyframed and sampled like other numeric style properties.

The existing reveal actions use this track directly:

- `wipe-in` in `crates/animatix/src/timeline/actions/entrance.rs::WipeIn`
- `draw-in`, `reveal-in`, `wipe-out`, `reveal-out`, `draw-out` in `crates/animatix/src/timeline/actions/reveal.rs`

For example, `draw-in` inserts:

```rust
track.stroke_progress.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
```

Why it does not work for `PlotCurve` yet:

1. Registry applicability is `Applicable::AllShapes`, and `Applicable::AllShapes` only matches `ActorKindId::Shape(_)`, not `ActorKindId::PlotCurve`.
2. `Timeline::process_plot_actor()` does not currently parse a declaration-time `stroke_progress` property for plot actors, so `curve: PlotCurve, ..., stroke_progress: 0` may not affect the initial keyframe unless the generic property path handles it later.
3. `PlotCurvePrimitive::evaluate()` returns full paths and does not sample or apply `ctx.track.stroke_progress`.
4. `RenderCommand::Paths::execute()` renders `VelloPath.path` directly with a full stroke; `VelloPath` has no per-path progress field.

## Option A: `stroke_progress` on `PlotCurve`

This is the minimal language change. Users write an explicit initial progress and animate it later:

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(x),
  stroke: accent.primary,
  stroke_width: 4,
  stroke_progress: 0

#1s
curve.stroke_progress = 1 [1s, ease: ease-out]
```

For curves inside a `Graph`:

```animatix
#0s
graph: Graph, x_domain: (-6.28, 6.28), y_domain: (-1.5, 1.5), size: (720, 320), at: scene.center {
  signal: PlotCurve,
    kind: "cartesian",
    func: (x) => sin(x),
    stroke: accent.primary,
    stroke_width: 4,
    stroke_progress: 0
}

#500ms
signal.stroke_progress = 1 [1.5s, ease: ease-out]
```

### Implementation

#### Property Registry

Add a dedicated applicability variant or a small explicit actor set. The explicit set is the smallest diff:

```rust
schema!("stroke_progress", ValueType::F32, F::ASSIGNABLE_AI,
    ActorField::StrokeProgress, None,
    Applicable::ActorKinds(&[
        A::Rect,
        A::Ellipse,
        A::Line,
        A::Arrow,
        A::Polygon,
        A::Path,
        A::PlotCurve,
    ]),
    |_| super::property_engine::PropertyValue::F32(1.0)),
```

If `ActorKindId` does not have per-shape variants because shapes are represented as `ActorKindId::Shape(ShapeKind)`, use a new variant instead:

```rust
pub enum Applicable {
    // ...
    AllStrokePaths,
}

impl Applicable {
    pub fn includes(self, kind: ActorKindId) -> bool {
        match self {
            Applicable::AllStrokePaths => matches!(kind, ActorKindId::Shape(_) | ActorKindId::PlotCurve),
            // ...
        }
    }
}
```

Then change only the property row:

```rust
schema!("stroke_progress", ValueType::F32, F::ASSIGNABLE_AI,
    ActorField::StrokeProgress, None,
    Applicable::AllStrokePaths,
    |_| super::property_engine::PropertyValue::F32(1.0)),
```

`stroke` and `stroke_width` should receive the same applicability treatment if they are still restricted to `AllShapes`; `PlotCurve` already parses them manually today, but including it in the registry keeps assignment, injection, inspector, and docs consistent.

#### Plot Builder

Teach `Timeline::process_plot_actor()` in `crates/animatix/src/timeline/build/plot.rs` to respect declaration-time `stroke_progress`:

```rust
let mut stroke_progress = existing_track.stroke_progress.last(1.0);

for prop in props {
    match prop.name.as_str() {
        // ...
        "stroke_progress" => {
            let v = evaluate_expr_with_lookup_diagnostic(
                &prop.value,
                &initial_eval_env,
                diagnostics,
                &prop_subject,
            ).unwrap_or(Value::Num(1.0));
            stroke_progress = v.as_num() as f32;
        }
        // ...
    }
}
```

`ProcessedPlotActor` already carries `stroke_progress`, and `process_plot_actor_dispatch()` already writes that value into `track.stroke_progress` through the same path used by other actors, so no new storage is needed.

#### Rendering / Evaluation

Prefer a shared helper that trims a `BezPath` by arc length and returns a new `BezPath` containing only the prefix. Keep it in a reusable path utility module so shapes and curves can share it, for example `crates/animatix/src/timeline/path_progress.rs` or near the existing vector path helpers.

API shape:

```rust
pub fn apply_stroke_progress(paths: &[VelloPath], progress: f32) -> Vec<VelloPath> {
    let progress = progress.clamp(0.0, 1.0);
    if progress >= 1.0 {
        return paths.to_vec();
    }
    if progress <= 0.0 {
        return paths.iter().map(empty_stroked_path_like).collect();
    }

    let total_len = paths.iter().map(|path| path.path.arclen(0.25)).sum::<f64>();
    let mut remaining = total_len * progress as f64;

    paths.iter().filter_map(|path| {
        let len = path.path.arclen(0.25);
        if remaining <= 0.0 {
            return None;
        }
        let next_path = if remaining >= len {
            path.path.clone()
        } else {
            trim_bez_path_prefix(&path.path, remaining)
        };
        remaining -= len;
        Some(VelloPath { path: next_path, ..path.clone() })
    }).collect()
}
```

`trim_bez_path_prefix()` should flatten the path to line segments and emit a polyline prefix. This is enough for `PlotCurve` because sampled curves are already emitted as `move_to` + `line_to` segments. It also works acceptably for shape paths after flattening:

```rust
fn trim_bez_path_prefix(path: &kurbo::BezPath, target_len: f64) -> kurbo::BezPath {
    let mut out = kurbo::BezPath::new();
    let mut remaining = target_len.max(0.0);
    let mut current = None;

    for element in path.flatten(0.25) {
        match element {
            kurbo::PathEl::MoveTo(point) => {
                current = Some(point);
                out.move_to(point);
            }
            kurbo::PathEl::LineTo(point) => {
                let Some(start) = current else {
                    out.move_to(point);
                    current = Some(point);
                    continue;
                };
                let segment_len = start.distance(point);
                if segment_len <= remaining {
                    out.line_to(point);
                    remaining -= segment_len;
                    current = Some(point);
                } else {
                    let t = remaining / segment_len;
                    out.line_to(start.lerp(point, t));
                    break;
                }
            }
            kurbo::PathEl::ClosePath => {}
            _ => {}
        }
    }

    out
}
```

Then apply it in `PlotCurvePrimitive::evaluate()`:

```rust
fn evaluate(
    &self,
    ctx: &crate::primitives::EvaluateCtx,
    _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError> {
    use crate::primitives::RenderCommand;
    use crate::timeline::TrackAccessor;

    if ctx.vector_paths.is_empty() {
        return Ok(None);
    }

    let mut progress = ctx.track.stroke_progress.get(ctx.time_ms, 1.0);
    if let Some(overrides) = ctx.overrides {
        if let Some(crate::timeline::Value::Num(value)) = overrides.get("stroke_progress") {
            progress = *value as f32;
        }
    }

    let paths = crate::timeline::path_progress::apply_stroke_progress(ctx.vector_paths, progress);
    Ok(Some(vec![RenderCommand::Paths { paths }]))
}
```

This hook works for both static `vector_paths` and dynamic `procedural_plot` paths because `scene_eval.rs` already computes `vector_paths` before invoking the primitive.

A follow-up should also call the same helper in `evaluate_shape_render()` or inside `RenderCommand::Paths::execute()` if the current shape path does not already apply `stroke_progress`. The cleanest shared option is to sample progress in each primitive evaluate path, because `RenderCommand::Paths` currently has no actor/track context.

#### Tests

Add focused tests before broad rendering tests:

1. `crates/animatix/src/timeline/property_registry.rs` or adjacent property tests: assert `stroke_progress` applies to `ActorKindId::PlotCurve`.
2. `crates/animatix/src/timeline/build/plot.rs`: build a `PlotCurve` with `stroke_progress: 0` and assert `track.stroke_progress.get(0, 1.0) == 0.0`.
3. `crates/animatix/src/primitives/plot.rs` or path helper tests: trim a simple two-segment path at `0.5` and assert the emitted endpoint lands halfway through total length.
4. `crates/animatix/src/timeline/actions/reveal.rs` or `entrance.rs`: if keeping existing `draw-in` support for curves, assert `draw-in curve [1s]` creates `0 -> 1` stroke progress keyframes with no unsupported-target diagnostic.

Verification:

```bash
cargo test -p animatix stroke_progress
cargo test -p animatix plot
cargo test -p animatix actions::reveal
```

## Option B: `trace` Action

`trace` is dedicated syntax for curve tracing:

```animatix
#0s
curve: PlotCurve,
  kind: "cartesian",
  func: (x) => sin(x),
  stroke: accent.primary,
  stroke_width: 4

#1s
trace curve [1s, ease: ease-out]
```

It desugars to keyframing `stroke_progress` from `0` to `1` over the action duration. This should be implemented only after Option A, because the action depends on the same rendering path.

### Semantics

`trace curve [duration, ease]`:

- Valid targets: `PlotCurve` initially. Optionally, all stroke-path actors later.
- At start: set `curve.stroke_progress = 0`.
- At end: set `curve.stroke_progress = 1` using the action easing.
- Do not modify `fill_opacity`; curves are stroke-only.
- Preserve pre-delay values with the same guard-keyframe behavior used by existing actions.

Equivalent explicit form:

```animatix
#1s
curve.stroke_progress = 0
curve.stroke_progress = 1 [1s, ease: ease-out]
```

Implementation in `crates/animatix/src/timeline/actions/entrance.rs`:

```rust
/// Traces a PlotCurve by animating stroke progress from 0 to 1.
pub struct Trace;

impl BuiltinAction for Trace {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "trace".to_string(),
            category: "Entrance".to_string(),
            description: "Traces a PlotCurve by animating stroke progress.".to_string(),
            params: vec![],
            modifiers: base_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let t_start_ms = (time_ms + parsed.delay_ms) as u64;
        let t_end_ms = (time_ms + parsed.delay_ms + parsed.duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }
            let Some(track) = timeline.tracks.get_mut(target) else { continue };
            if track.kind != crate::timeline::ActorKindId::PlotCurve {
                super::push_unsupported_action_target_diagnostic(
                    &action.verb,
                    target,
                    "trace currently supports PlotCurve targets only",
                    diagnostics,
                    None,
                );
                continue;
            }

            if parsed.delay_ms > 0.0 && parsed.duration_ms == 0.0 && t_start_ms > 0 {
                super::ensure_guard_keyframe(&mut track.stroke_progress, t_start_ms.saturating_sub(1), 1.0);
            }

            track.stroke_progress.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, parsed.easing);
        }
    }
}
```

Then register it in `crates/animatix/src/timeline/actions/mod.rs`:

```rust
use entrance::{FadeIn, Trace, WipeIn};

fn get_builtin_actions() -> Vec<Box<dyn BuiltinAction>> {
    vec![
        Box::new(FadeIn),
        Box::new(WipeIn),
        Box::new(Trace),
        // ...
    ]
}
```

### Relationship to `draw-in`

`draw-in curve [1s]` could also work after Option A because `ensure_vector_reveal_target()` accepts vector-path actors and `PlotCurve` has `vector_paths`. However, `draw-in` also animates `fill_opacity`, which is irrelevant for stroke-only curves. `trace` is clearer for plot curves and avoids encoding shape-fill semantics into a curve action.

## Documentation Updates

Update `docs/spec.md`:

- In the property applicability table, change `stroke_progress` from “Shapes” to “Shapes and PlotCurve” or “Stroke paths”.
- In the `PlotCurve` section, add the trace example and state that `stroke_progress: 0..1` reveals the curve along its sampled path.
- If Option B is implemented, add `trace` to built-in entrance actions.

Update `docs/primitives.md`:

```markdown
## PlotCurve

Properties:
- `kind`: `"cartesian" | "polar" | "parametric" | "implicit"`
- `func`: Closure
- `stroke` / `stroke_color`: Color
- `stroke_width` / `width`: Number
- `stroke_progress`: Number from `0` to `1`; reveals the curve from start to end
```

Example:

```animatix
#0s
graph: Graph, x_domain: (-6.28, 6.28), y_domain: (-1.5, 1.5), size: (720, 320), at: scene.center {
  signal: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke: accent.primary, stroke_width: 4, stroke_progress: 0
}

#500ms
signal.stroke_progress = 1 [1.5s, ease: ease-out]
```

If Option B ships:

```animatix
#500ms
trace signal [1.5s, ease: ease-out]
```

Update `docs/properties.md` if it remains the generated/user-facing property table:

```markdown
| `stroke_progress` | F32 | ✓ | ✓ | Shapes, PlotCurve |
```

## Recommendation

Implement Option A first.

Reasons:

- It uses the existing property system, keyframes, easing, assignments, modifier overrides, and action infrastructure.
- It keeps `PlotCurve` consistent with the rest of the vector-path pipeline.
- It exposes a composable primitive capability rather than only a one-off action.
- It is minimal: one applicability change, one plot-property parse path if needed, one shared path-trimming helper, and one evaluate hook.
- It makes existing or future actions (`draw-in`, `reveal-in`) naturally work on curves where appropriate.

Add Option B only as ergonomic sugar after Option A is tested. `trace` is a good user-facing action for educational animation, but it should not be the underlying capability. The underlying capability should remain `stroke_progress` on stroke-path actors.

## Files to Touch

- `crates/animatix/src/timeline/property_registry.rs` — include `PlotCurve` in `stroke_progress` applicability; optionally do the same for `stroke` and `stroke_width` for consistency.
- `crates/animatix/src/timeline/build/plot.rs` — parse declaration-time `stroke_progress` for `PlotCurve` and carry it into `ProcessedPlotActor`.
- `crates/animatix/src/timeline/path_progress.rs` or a nearby path utility module — add shared path-prefix trimming by normalized progress.
- `crates/animatix/src/primitives/plot.rs` — sample `track.stroke_progress` and apply the helper before returning `RenderCommand::Paths`.
- `crates/animatix/src/primitives/mod.rs` or shape evaluate helpers — ensure shapes use the same helper if they do not already trim by `stroke_progress` in the current render path.
- `crates/animatix/src/timeline/actions/entrance.rs` — optionally add `Trace`.
- `crates/animatix/src/timeline/actions/mod.rs` — optionally register `Trace`.
- `docs/spec.md` — document `PlotCurve.stroke_progress` and optionally `trace`.
- `docs/primitives.md` — document `PlotCurve.stroke_progress` with examples.
- `docs/properties.md` — update applicability table if maintained manually.

## Risks

- Path trimming must preserve multi-subpath behavior for curves with discontinuities; progress should advance over total visible path length and skip gaps created by `NaN` samples.
- Flattening Bézier paths changes exact shape geometry for partial reveals; acceptable for stroke tracing, but tests should use tolerances.
- Closed shapes and filled paths need clear semantics: fill should usually remain hidden for `draw-in` until the stroke is complete, while `PlotCurve` has no fill.
- Dynamic `procedural_plot` curves must apply progress after frame-time re-sampling, not to stale build-time paths.
- Existing `draw-in` may start working on `PlotCurve`; confirm whether that is desired or whether `trace` should be the documented curve-specific verb.
