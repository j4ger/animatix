# BarChart Primitive Design

## Goal

Add a first-class `BarChart` primitive so data-driven spectrum scenes can declare labelled bars, axes, styling, and baseline growth without hand-authored `Rect` and `Text` actors.

## Context

Gap G1 in `docs/FFT_THOUGHT_EXPERIMENT.md` identifies that the FFT frequency spectrum currently needs manual `Rect` actors, hand-calculated bar positions, labels, and morph targets. The target primitive should make the spectrum scene read like data, not pixel math.

The current runtime has two relevant paths:

- `crates/animatix/src/primitives/mod.rs` exposes the `Primitive` trait and `PRIMITIVES` registry.
- Plot-like primitives are built through `Timeline::process_plot_actor_dispatch()` in `crates/animatix/src/timeline/build/actor.rs` and `Timeline::process_plot_actor()` in `crates/animatix/src/timeline/build/plot.rs`.
- Frame-time procedural sampling currently stores `AnimationTrack::procedural_plot: Option<ProceduralPlot>` in `crates/animatix/src/timeline/track.rs`, then `scene_eval.rs` calls `sample_procedural_plot()`. Today `ProceduralPlot` only models `PlotCurve`, so BarChart needs either a generalized procedural visual enum or a separate payload.

## 1. Syntax & Usage

### Basic Bar Chart

Ideal concise FFT spectrum syntax:

```animatix
spectrum: BarChart,
  data: ((2, 1.0), (5, 0.55), (9, 0.3)),
  size: (600, 260),
  at: (640, 400)
```

Tuple interpretation:

- First value is the category key or x-coordinate.
- Second value is the bar magnitude.
- Numeric first values display as labels by default (`"2"`, `"5"`, `"9"`) unless label text is provided.

### With Labels

The requested shorthand is ideal but requires parser support for keyed tuple entries (`2 Hz: 1.0` inside an expression):

```animatix
spectrum: BarChart,
  data: (2 Hz: 1.0, 5 Hz: 0.55, 9 Hz: 0.3),
  size: (600, 260),
  at: (640, 400)
```

Implementation-compatible syntax should use quoted labels or `Bar` objects:

```animatix
spectrum: BarChart,
  data: (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)),
  size: (600, 260),
  at: (640, 400)
```

Optional structured form for future extensibility:

```animatix
spectrum: BarChart,
  data: (
    Bar { label: "2 Hz", value: 1.0 },
    Bar { label: "5 Hz", value: 0.55 },
    Bar { label: "9 Hz", value: 0.3 }
  )
```

### With Styling

```animatix
spectrum: BarChart,
  data: (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)),
  size: (600, 260),
  bar_width: auto,
  gap: 24,
  bar_colors: (accent.danger, accent.success, accent.warning),
  show_axis: true,
  show_labels: true,
  axis_color: text.muted,
  color: accent.primary,
  stroke: text.primary,
  stroke_width: 0,
  max_value: auto,
  direction: "vertical",
  at: (640, 400)
```

Rules:

- `color` is the fallback bar fill when `bar_colors` is absent or shorter than `data`.
- `bar_colors: auto` cycles through colorscheme auto colors per bar.
- `gap` is screen-space pixels for standalone charts and math-coordinate spacing inside a `Graph` only when `bar_width` is explicitly math-space.
- `show_axis` draws the baseline axis only; full tick/grid customization stays with `Graph` and `NumberPlane`.

### Animated Growth

Recommended user-facing animation is re-declaration with the same labels and changed values:

```animatix
#0s
spectrum: BarChart,
  data: (("2 Hz", 0), ("5 Hz", 0), ("9 Hz", 0)),
  size: (600, 260),
  max_value: 1.0,
  bar_colors: (accent.danger, accent.success, accent.warning),
  show_axis: true,
  show_labels: true,
  at: (640, 400)

#2s
spectrum: BarChart,
  data: (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)) [800ms, ease: ease-out]
```

A convenience property can be added later:

```animatix
#0s
spectrum: BarChart,
  data: (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)),
  grow: 0

#2s
spectrum.grow = 1 [800ms, ease: ease-out]
```

The MVP should not add `grow`; it can use existing vector-path morphing with zero-value re-declaration.

### Inside a Graph

A `BarChart` inside `Graph` should respect the graph's math-coordinate mapping:

```animatix
spectrum_graph: Graph,
  x_domain: (0, 12), y_domain: (0, 1.1),
  size: (700, 300), at: (640, 390),
  grid: true, ticks: true, tick_labels: "both" {

  spectrum: BarChart,
    data: ((2, 1.0), (5, 0.55), (9, 0.3)),
    bar_width: 0.8,
    gap: auto,
    show_axis: false,
    show_labels: true,
    bar_colors: (accent.danger, accent.success, accent.warning)
}
```

Graph behavior:

- Numeric category keys are math x-values.
- Bar values are math y-values for vertical charts.
- Baseline is `0` when `y_domain` contains zero, otherwise the nearest visible domain edge.
- `show_axis: false` is common inside `Graph` because the parent graph already owns axes.
- `Graph { BarChart, PlotCurve }` should work; both children render into the same math-coordinate frame.

## 2. Properties

| Property | Type | Default | Animated | Assignable | Notes |
|---|---|---:|---|---|---|
| `data` | `BarData` build-time list | `()` | via re-declaration | no | List of `(label_or_x, value)` tuples or `Bar { label, value }` objects. |
| `bar_width` | `F32` or `"auto"` | `"auto"` | no | no | Pixel width standalone; math x-units inside `Graph` when numeric. |
| `gap` | `F32` or `"auto"` | `"auto"` | no | no | Space between bars; reuse name conflicts with layout `gap`, so registry applicability must include `BarChart`. |
| `bar_colors` | `ColorList` or `"auto"` | `"auto"` | no | no | Per-bar fills; falls back to `color` after the list ends. |
| `show_axis` | `Bool` | `true` standalone, `false` in `Graph` | no | no | Draws baseline axis. Requires `ValueType::Bool` in registry or `String` workaround. |
| `show_labels` | `Bool` | `true` | no | no | Generates bar category labels. Requires `ValueType::Bool` or `String` workaround. |
| `axis_color` | `Color` | `text.muted` | yes | yes | Baseline/axis stroke color. |
| `direction` | `String` | `"vertical"` | no | no | `"vertical"` or `"horizontal"`. |
| `max_value` | `F32` or `"auto"` | `"auto"` | no | no | Positive scale cap for standalone charts; can be explicit to keep animations stable. |
| `x_domain` | `Vec2` | parent graph or `(0, n)` | no | no | Standalone override; inside `Graph`, inherited unless explicitly set. |
| `y_domain` | `Vec2` | `(0, max_value)` | no | no | Standalone override; inside `Graph`, inherited unless explicitly set. |
| `size` | `Vec2` | `(500, 260)` | yes | yes | Chart visual bounds. Existing `SizedActors` must include `BarChart`. |
| `color` | `Color` | colorscheme plot default | yes | yes | Fallback bar fill. |
| `stroke` / `stroke_color` | `Color` | transparent or `stroke.default` | yes | yes | Optional bar outlines. |
| `stroke_width` | `F32` | `0` | yes | yes | Optional bar outline width. |
| `fill_opacity` | `F32` | `1` | yes | yes | Applies to bars. |
| `font_size` | `F32` | `14` | yes | yes | Label text size if labels are generated by the primitive. |

Registry implications:

- `ValueType` currently lacks `Bool`, `BarData`, `ColorList`, and union types like `F32 | "auto"`; this design should introduce those only if the generic property engine will own them.
- MVP can register `data`, `bar_width`, `bar_colors`, `direction`, `max_value`, `show_axis`, and `show_labels` as `BuildTimeOnly` under a new bar/chart group, then parse/evaluate them manually in `bar_chart.rs` or `build/plot.rs`.
- `gap` already exists for layout containers; extend its `Applicable` set to include `ActorKindId::BarChart` and route the BarChart use through the chart build handler, not `ContainerLayout`.
- The `PROPERTY_REGISTRY` slice must remain sorted by property name.

## 3. Implementation Strategy

### Options Considered

#### Option A: Procedural Plot Path

`BarChart` stores chart data and style in the track at build/re-declaration time, converts bars and axes into `Vec<VelloPath>`, and relies on existing vector-path keyframes for morphing.

Pros:

- Fits current plot primitives: `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, and `NumberPlane` already return vector paths.
- Reuses `AnimationTrack::vector_paths`, `evaluate_vector_paths()`, morphing, opacity, transforms, and render dispatch.
- Keeps animation random-access and deterministic.
- Requires the least new runtime surface for the FFT use case.

Cons:

- `ProceduralPlot` currently only supports `PlotCurve`; dynamic bar data would require a generalized enum.
- Text labels are not naturally represented by `VelloPath` unless compiled into text paths or generated as child `Text` tracks.
- Individual bar value assignment (`spectrum.data[1] = ...`) is not supported.

#### Option B: Full `evaluate()` Path

`BarChartPrimitive::evaluate()` samples a `PropertyTrack<BarChartData>` at frame time and returns `RenderCommand::Paths` plus `RenderCommand::Text`.

Pros:

- Best long-term model for dynamic charts and per-frame reactive values.
- Can interpolate bar values explicitly without path morph edge cases.
- Can produce label text at frame time through `TextCompileCtx`.

Cons:

- Requires new `BarChartData` storage on `AnimationTrack`, `Interpolate` implementation, property engine support, and likely a new `RenderCommand` text positioning pattern.
- More work than needed for static FFT data with simple growth.
- Bypasses useful existing path-morph code unless duplicated.

#### Option C: Container with Auto-Generated Children

`BarChart` expands to `Rect`, `Line`, and `Text` children during build, similar to a macro or component.

Pros:

- Uses existing primitives and actions.
- Exposes generated bars as ordinary actors if names are stable.

Cons:

- The current `for` mechanism is compile-time structural expansion, not runtime data-driven generation.
- Generated child labels need stable naming, collision handling, source indexing, inspector behavior, and deletion semantics.
- Re-declaration and morphing across generated children would be brittle.

### Recommendation

Use **Option A for MVP**, with one important adjustment: treat BarChart as a plot-like primitive that writes prebuilt `vector_paths` on each declaration/re-declaration, not as a `ProceduralPlot` until dynamic data is needed.

Implementation approach:

1. Add `ActorKindId::BarChart` as a non-shape plot actor.
2. Add `BarChartPrimitive` in `crates/animatix/src/primitives/bar_chart.rs` with `category() -> ActorCategory::Plot`, `icon_id() -> CHART_BAR`, and `evaluate()` mirroring plot primitives by returning `ctx.vector_paths`.
3. Add a bar-chart builder path that parses `data` and produces a stable ordered list of Vello rectangle paths, optional axis path, and optional generated label tracks.
4. Store generated paths in `track.vector_paths` using existing timed re-declaration machinery so morphing handles growth from zero.
5. Defer full frame-time data interpolation until a later `ProceduralVisual::BarChart` or `PropertyTrack<BarChartData>` is needed.

Why this wins:

- It resolves G1 with minimal runtime churn.
- It preserves existing animation semantics: re-declaration morphing, `opacity`, `scale`, `rotation`, `transform`, `color`, and scene graph inheritance all work.
- It keeps `Graph { BarChart, PlotCurve }` composition consistent with current plot-child rendering.
- It avoids creating many hidden tracks for generated bars, which would make the inspector and source write-back confusing.

## 4. Files to Touch

- `crates/animatix/src/primitives/mod.rs` — add `mod bar_chart`, `pub use bar_chart::BAR_CHART`, include `&BAR_CHART` in `PRIMITIVES`, and extend tests that enumerate `ActorKindId` variants.
- `crates/animatix/src/primitives/bar_chart.rs` — new `BarChartPrimitive` implementation with metadata, build dispatch, `evaluate()`, default props, and docs-style examples in tests if nearby primitive tests exist.
- `crates/animatix/src/timeline/track.rs` — add `ActorKindId::BarChart`; update `Applicable::SizedActors` consumers if needed; optionally add `PropertyTrack<BarChartData>` only if choosing Option B later.
- `crates/animatix/src/timeline/property_registry.rs` — add sorted bar-specific properties (`bar_colors`, `bar_width`, `data`, `direction`, `max_value`, `show_axis`, `show_labels`, `axis_color` if not using `stroke`); extend `gap`, `size`, style applicability to include `BarChart`.
- `crates/animatix/src/timeline/build/actor.rs` — allow `process_plot_actor_dispatch()` to accept `ActorKindId::BarChart` or introduce `process_bar_chart_actor_dispatch()` if cleaner.
- `crates/animatix/src/timeline/build/plot.rs` — add `build_bar_chart_paths()` and route `ActorKindId::BarChart` through it; inherit parent `Graph` domains and size like `PlotCurve` does.
- `crates/animatix/src/timeline/plot.rs` — only needed if adding a generalized `ProceduralVisual`/`ProceduralBarChart`; MVP can avoid changes here except shared coordinate helpers.
- `crates/animatix/src/timeline/scene_eval.rs` — no new legacy dispatch should be necessary if `BarChartPrimitive::evaluate()` returns `RenderCommand::Paths`; only touch if procedural visual sampling is generalized.
- `crates/animatix-syntax/src/icon_glyphs.rs` — reuse `CHART_BAR` for BarChart or add a separate alias only if the UI needs distinct chart icons.
- `docs/properties.md` — add BarChart properties after the registry update; note that this file is generated from `PROPERTY_REGISTRY` but currently maintained in docs.
- `docs/primitives.md` — add a `BarChart` section under Graph Primitives with standalone and `Graph` examples.
- `docs/spec.md` — update primitive lists and LLM checklist to include `BarChart` when implemented.
- `examples/fft_explain.amx` — replace manual spectrum `Line`, `Rect`, and `Text` bar declarations with one `BarChart` and a re-declaration growth animation.
- `docs/FFT_THOUGHT_EXPERIMENT.md` — mark G1 resolved after implementation, replace workaround text with the new `BarChart` syntax, and update the summary table.
- `crates/animatix-analyzer` — update completion/hover data if primitive/property completions are not entirely driven by runtime metadata.
- `tree-sitter-animatix` — no change for MVP tuple/object syntax; needed only if adding keyed tuple syntax like `2 Hz: 1.0`.

## 5. Data Model

### MVP Stored Model

Use a compact build-time struct in `bar_chart.rs` or `timeline/build/plot.rs`:

```rust
struct BarChartSpec {
    data: Vec<BarDatum>,
    bar_width: AutoOr<f32>,
    gap: AutoOr<f32>,
    bar_colors: AutoOr<Vec<[f32; 4]>>,
    show_axis: bool,
    show_labels: bool,
    axis_color: [f32; 4],
    direction: BarDirection,
    max_value: AutoOr<f32>,
    x_domain: Option<[f64; 2]>,
    y_domain: Option<[f64; 2]>,
}

struct BarDatum {
    key: BarKey,
    label: String,
    value: f32,
}

enum BarKey {
    CategoryIndex(usize),
    Numeric(f64),
}
```

This struct is parsed from declaration properties, used immediately to build Vello paths, and not stored on the track for MVP. The resulting path list is stored in `AnimationTrack::vector_paths` as a normal keyframed vector payload.

### Later Dynamic Model

If BarChart needs `always` or assignment-driven data, add:

```rust
pub struct BarChartData {
    pub bars: Vec<BarDatum>,
}
```

and store it as `AnimationTrack::bar_chart_data: Option<PropertyTrack<BarChartData>>` or as part of a generalized procedural enum:

```rust
pub enum ProceduralVisual {
    PlotCurve(ProceduralPlot),
    BarChart(ProceduralBarChart),
}
```

`Interpolate for BarChartData` should require equal label/key sequences and linearly interpolate only `value`; if labels differ, hold-then-swap at `t >= 0.5` and emit a diagnostic for timed re-declarations.

### Re-Declaration Morphing

Re-declaration works by generating the same number of paths in the same order:

1. Axis path, if present.
2. Bar fill path for each datum in declaration order.
3. Optional bar stroke path can be embedded as each `VelloPath.stroke` rather than a separate path.

For smooth bar growth:

- Declare zero values at `#0s` with final labels and colors.
- Re-declare final values at `#2s [duration]`.
- `vector_paths` interpolation morphs each bar rectangle independently because path order and segment topology are stable.
- `max_value` should be explicit during animations to avoid auto-scale changing between declarations.

## 6. Rendering

### Coordinate Spaces

Standalone chart:

- `size` is full chart bounds in local actor pixels, matching other plot primitives' visual bounds.
- Local chart origin is actor center.
- Baseline is near the bottom for non-negative vertical data: `baseline_y = size.h / 2`.
- Plot area reserves margins when labels or axis are visible: bottom label margin, left value-label margin if later added.

Graph child chart:

- Use parent `Graph` values stored in `Timeline.env` (`{graph}_x_domain`, `{graph}_y_domain`, `{graph}_size`) like `PlotCurve` does.
- Numeric keys and values are math coordinates mapped to graph-local pixels.
- Label offsets remain screen-space so text stays legible.

### Bars

Each bar is a filled rectangle path:

```text
move_to(left, baseline)
line_to(right, baseline)
line_to(right, value_y)
line_to(left, value_y)
close()
```

Vertical positive bars grow upward from baseline; negative bars grow downward. Horizontal bars mirror this with baseline on the value axis.

Bar geometry:

- Auto width standalone: `(plot_width - gap * (n - 1)) / n`, clamped to `>= 1px`.
- Auto width in `Graph`: infer from sorted numeric x positions (`min_delta * 0.8`) when possible; otherwise use category slots.
- Explicit `bar_width` in `Graph`: math x-units for vertical charts, math y-units for horizontal charts.
- Explicit `bar_width` standalone: pixels.

### Axis

`show_axis` draws a stroked baseline:

- Vertical standalone: x-axis from plot-left to plot-right at baseline.
- Horizontal standalone: y-axis from baseline to full plot height.
- Graph child: draw only if requested; parent `Graph` usually owns axes.
- Axis color uses `axis_color`, falling back to `text.muted` or `stroke.default`.

### Labels

MVP recommendation: generate child `Text` tracks for labels at build time, following the precedent of graph tick labels.

Label rules:

- Bar labels sit below vertical bars or left of horizontal bars.
- Labels are not part of `vector_paths`, so they avoid path morph distortion during bar growth.
- Generated labels should use stable internal labels like `{chart_label}__label_{index}` and be marked as children of the chart node.
- Generated labels should not be source-editable by default; inspector can show them as derived children or hide them.

Alternative: compile labels in `BarChartPrimitive::evaluate()` with `TextCompileCtx` and return `RenderCommand::Text`, but that needs per-label transforms/offsets that `RenderCommand::Text` does not currently encode cleanly.

## 7. Animation

### MVP Animation

Use timed re-declaration:

```animatix
#0s
spectrum: BarChart, data: (("2 Hz", 0), ("5 Hz", 0), ("9 Hz", 0)), max_value: 1.0

#2s
spectrum: BarChart, data: (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)) [800ms, ease: ease-out]
```

Expected behavior:

- Each bar path morphs from zero-height to final height.
- Axis and labels are stable; use `fade-in spectrum` or explicit opacity if the whole chart should appear gradually.
- Color and stroke properties can animate with existing property tracks.

### Individual Bar Interpolation

For timed re-declarations, builder should validate:

- Same bar count.
- Same labels or numeric keys in the same order.
- Same `direction`.
- Same explicit `max_value` recommended; if omitted and auto max changes, warn because animation will rescale while growing.

When these hold, morphing is deterministic. If they do not hold, use `strategy: fade` or hold-then-swap and emit a diagnostic.

### Adding and Removing Bars

MVP policy: fixed set only for smooth morphs.

- Adding/removing bars in a timed re-declaration should warn that topology changed.
- Instant re-declaration may add/remove bars.
- Timed topology changes can use `strategy: fade` once the existing morph strategy is plumbed through chart re-declarations.

## 8. Edge Cases

| Edge Case | Behavior |
|---|---|
| Empty `data` | Render optional axis only; emit a warning if `show_axis` and `show_labels` are both false because actor is invisible. |
| Zero values | Render zero-height bars at baseline; keep a minimal degenerate rectangle path for morph stability. |
| Negative values | Use baseline `0` when domain includes it; vertical bars extend downward, horizontal bars extend left. |
| Mixed positive/negative values | Domain must include both sides or auto-domain expands to `[min_value, max_value]` with zero included. |
| Domain excludes zero | Clamp baseline to nearest visible edge and warn if values cross outside domain. |
| Very large values | Clamp screen coordinates to chart plot area; optionally warn on overflow when explicit domain clips values. |
| Very short bars | Keep geometric height accurate; consider `min_bar_pixels` later but do not add in MVP. |
| Many bars | Generate one `VelloPath` per bar; for thousands of bars, labels auto-disable or warn above a threshold like 200. |
| Duplicate labels | Allowed for category charts; matching for animation uses index plus label. |
| Duplicate numeric x keys | Allowed but bars overlap; warn if inside `Graph`. |
| Non-numeric values | Emit build diagnostic and skip invalid entries. |
| `bar_colors` shorter than data | Cycle colors or fall back to `color`; choose cycle for `auto`, fallback for explicit partial list. |
| Horizontal labels | Place labels on the category axis; rotate only if a future `label_rotation` property is added. |

## 9. Integration with Graph

### Coordinate Ownership

`Graph` owns math-coordinate mapping. A child `BarChart` should consume the same parent env values that `PlotCurve` consumes:

- `{graph_label}_x_domain`
- `{graph_label}_y_domain`
- `{graph_label}_size`

Standalone `BarChart` owns its own pixel coordinate system and may accept explicit `x_domain`/`y_domain` for scaling.

### Composition

This should work:

```animatix
graph: Graph, x_domain: (0, 12), y_domain: (0, 1.1), size: (700, 300) {
  spectrum: BarChart,
    data: ((2, 1.0), (5, 0.55), (9, 0.3)),
    bar_width: 0.8,
    show_axis: false,
    bar_colors: (accent.danger, accent.success, accent.warning)

  envelope: PlotCurve,
    kind: "cartesian",
    func: (x) => exp(-0.2 * x),
    color: text.muted,
    stroke_width: 2
}
```

Render order follows declaration order, so bars can sit behind or in front of curves by ordering child declarations.

### Graph Labels and Bar Labels

- Parent graph tick labels describe numeric axes.
- BarChart `show_labels` describes categories/frequency bins.
- If numeric x-values are used inside a `Graph` with `tick_labels: "x"`, users may set `show_labels: false` to avoid duplicate x labels.

## Implementation Plan

1. Add `BarChart` primitive metadata: edit `crates/animatix/src/primitives/mod.rs`, add `crates/animatix/src/primitives/bar_chart.rs`, add `ActorKindId::BarChart` in `crates/animatix/src/timeline/track.rs`; verify with `cargo test -p animatix primitives::tests::registry_matches_primitives`.
2. Register BarChart properties: update `crates/animatix/src/timeline/property_registry.rs` with sorted build-time chart properties and applicability changes for `gap`, `size`, and style fields; verify `cargo test -p animatix timeline::property_registry::tests::registry_is_sorted`.
3. Parse bar data and options: implement `parse_bar_chart_spec()` in `crates/animatix/src/primitives/bar_chart.rs` or `crates/animatix/src/timeline/build/plot.rs`, accepting tuple pairs and `Bar { label, value }`; verify with focused unit tests for valid data, invalid entries, auto values, and color lists.
4. Generate chart paths: add `build_bar_chart_paths()` near plot builders in `crates/animatix/src/timeline/build/plot.rs`, producing stable bar path ordering plus optional axis; verify by asserting path count and bounding boxes in unit tests.
5. Wire build dispatch: route `ActorKindId::BarChart` through the plot-like declaration path in `crates/animatix/src/timeline/build/actor.rs` and store `vector_paths` keyframes for re-declarations; verify an `.amx` snippet builds without diagnostics and produces non-empty vector paths.
6. Add labels: create derived label handling either through generated child `Text` tracks in `Timeline::process_plot_actor()` or a dedicated evaluate command path; verify labels appear in the scene graph and inherit chart transforms.
7. Verify graph integration: test `Graph { BarChart, PlotCurve }` with parent domain mapping in `crates/animatix/src/timeline/build/plot.rs`; verify bar screen positions match `math_to_screen()` expectations.
8. Update docs and example: edit `docs/primitives.md`, `docs/properties.md`, `docs/spec.md`, `examples/fft_explain.amx`, and `docs/FFT_THOUGHT_EXPERIMENT.md`; verify `cargo test -p animatix` and render or preview the FFT example.

## Risks

- `data` and `bar_colors` need richer property value parsing than the generic property engine currently supports.
- Reusing `gap` for both layout and chart spacing may complicate `PROPERTY_REGISTRY` grouping unless BarChart handles it before generic group dispatch.
- Generated labels can confuse source write-back and inspector selection if they appear as normal editable actors.
- Auto `max_value` can make morph animations visually misleading when the maximum changes across re-declarations.
- Graph integration needs careful unit tests because standalone chart pixels and graph math coordinates have different width/gap semantics.
