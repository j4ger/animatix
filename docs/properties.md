# Animatix Property Reference

> Generated property tables are derived from the syntax property specs and the engine registry.
> The docs-only group map controls presentation order; it does not define property semantics.
> Run `cargo run -p animatix --example property_reference -- --write` to update them;
> the docs gate checks the generated output. For value examples and usage, see [`spec.md`](spec.md).

---

## Legend

| Column | Meaning |
|--------|---------|
| **Property** | Canonical name as used in source text |
| **Type** | Authored expression type from the language type system. `Any` remains unconstrained. |
| **Runtime kind** | Engine value representation; `BuildTimeOnly` has no animated track. |
| **Animated** | Supports keyframe animation (`✓`) or not (`—`) |
| **Assignable** | Supports the generic property assignment path (`✓`) or not (`—`); dedicated transition paths are explained in prose |
| **Applies to** | Exact built-in actor types from the catalog; scene/internal-only entries are explained in prose |

`Any` means the type checker does not constrain the expression; runtime property
handling can still reject an unsupported value. Runtime kinds describe that
representation separately and do not narrow the type-checker contract.

---

## Legend Participation

<!-- generated-property-table:legend-participation:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `legend` | `Bool \| Str` | `Sum<auto: Bool, hidden: Bool, label: String>` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
<!-- generated-property-table:legend-participation:end -->

## Legend Primitive

<!-- generated-property-table:legend-primitive:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `title` | `Str` | `String` | ✓ | ✓ | `Legend` |
| `label_color` | `Color \| Str` | `Color` | ✓ | ✓ | `Legend` |
| `swatch_size` | `Num` | `F32` | ✓ | ✓ | `Legend` |
<!-- generated-property-table:legend-primitive:end -->

`label_color: auto` chooses a color with contrast against the scene background;
an explicit color overrides that choice.

`Legend` also reads the shared rows `font_size` and `text_max_width` (under
Text) and `gap` (under Containers).

## Geometry

<!-- generated-property-table:geometry:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `position` | `Vec2` | `Vec2` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `at` | `Vec2` | `Vec2` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `offset` | `Vec2` | `Vec2` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `anchor` | `Any` | `SceneAnchor` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `size` | `Vec2` | `Vec2` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `corner_radius` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Glass`, `Connector` |
| `width` | `Num` | `F32` | ✓ | — | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Filter`, `Glass` |
| `height` | `Num` | `F32` | ✓ | — | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Filter`, `Glass` |
| `min_width` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Filter`, `Glass` |
| `min_height` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Filter`, `Glass` |
| `max_height` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Filter`, `Glass` |
| `max_item_width` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Image`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Filter`, `Glass` |
| `rotation` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `scale` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `transform` | `Vec2 \| Vec4 \| (Num, Num, Num, Num, Num, Num)` | `Transform` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `rotate_x` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `rotate_y` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `perspective` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
<!-- generated-property-table:geometry:end -->

## Style

<!-- generated-property-table:style:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `color` | `Color \| Str` | `Color` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `opacity` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `fill_opacity` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Arrow`, `Polygon`, `Path` |
| `stroke` | `Color \| Str` | `Color` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `stroke_width` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `stroke_progress` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `dash_pattern` | `List<Num>` | `F32List` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `dash_offset` | `Num` | `F32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `blend` | `Str` | `String` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `fill_gradient` | `Any` | `Gradient` | ✓ | ✓ | `Rect`, `Ellipse`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `stroke_gradient` | `Any` | `Gradient` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `gradient_extend` | `Str` | `String` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `gradient_space` | `Str` | `String` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `Callout`, `Connector` |
| `shadow` | `Vec4` | `Vec4` | ✓ | ✓ | `Rect`, `Ellipse`, `Arrow`, `Polygon`, `Path` |
| `shadow_color` | `Color` | `Color` | ✓ | ✓ | `Rect`, `Ellipse`, `Arrow`, `Polygon`, `Path` |
| `solo` | `Bool` | `Bool` | — | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `idle` | `Str` | `String` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
<!-- generated-property-table:style:end -->

`dash_pattern` takes a brace list of alternating segment and gap lengths in scene
pixels (`dash_pattern: {8, 6}`); animating `dash_offset` gives marching ants.
`blend` names a CSS mix-blend mode (`"screen"`, `"multiply"`, …) and composites
the actor's whole subtree against what is behind it. "All stroke paths" includes
the plot actors — a `PlotCurve` accepts `dash_pattern` and the gradients just
like a `Path` does, and the ramp survives a `stroke_progress` trace.

These properties also accept their text form inside a frame-time override:
`always { r.stroke = "#ff2d55" }` reads the same as `always { r.stroke =
(1.0, 0.18, 0.33, 1.0) }`. A `size` written there is authored in full width and
height, like the declaration is, not in the half-extents the shape state stores.

### Gradients

`fill_gradient` / `stroke_gradient` replace the solid paint with a ramp. The
value is a call naming the ramp shape, its geometry, and its stops:

```amx
bar: Rect, size: (400, 70),
  fill_gradient: linear(90, {"#ff2d55", "#ff9f0a"})
heading: Text, text: "Delightful", font_size: 48, font_weight: "bold",
  fill_gradient: linear(135, {"#ffd60a", "#f5b942"})
spot: Ellipse, size: (300, 300),
  fill_gradient: radial((0.5, 0.5), 0.5, {(0%, "#5e5ce6"), (100%, "#5e5ce600")})
arc: Path, commands: {…}, stroke_width: 10, fill_opacity: 0.0,
  stroke_gradient: sweep(0, {"#30d158", "#ffd60a", "#ff2d55", "#30d158"})
```

- `linear(angle, stops)` — `angle` in degrees, CSS convention (`0` points up,
  `90` to the right). The ramp spans the actor's bounding box along that axis
  (for text, the combined bounding box of all glyphs).
- `radial(center, radius, stops)` — `center` in normalized bounding-box
  coordinates, `radius` relative to the box's larger side. Both are optional:
  `radial(stops)` is centered with radius `0.5`.
- `sweep(angle, stops)` — rotates around an optional `center`, starting at
  `angle`.
- A stop is either a color (`"#ff2d55"`, `(1.0, 0.18, 0.33)`, `accent.primary`,
  `rgb(255, 45, 85)`) or an `(offset, color)` pair, where `offset` is `0..1` or a
  percentage. Stops without an offset are spread evenly.

The two shared settings apply to whichever ramps an actor carries:
`gradient_extend: "pad" | "repeat" | "reflect"` (default `"pad"`) and
`gradient_space: "oklab" | "srgb"` (default `"oklab"`, which avoids the dark band
a saturated hue ramp gets when interpolated channel-wise).

Ramps are keyframable: two `linear()` values of the same kind and stop count
interpolate their geometry and colors, so a gradient can turn, stretch, or shift
hue over time.

## Filter

Only applicable to `Filter` actors. See [`architecture.md`](architecture.md) §6 "Post-Processing (Filter)".

<!-- generated-property-table:filter:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `bounds` | `Vec4` | `Vec4` | ✓ | ✓ | `Filter` |
<!-- generated-property-table:filter:end -->

`bounds` is the registry-backed clip rectangle for the processed region.
Effect parameters (`Blur.radius`, `Bloom.intensity`, and the rest) are **not**
actor properties: they are declared per-stage children of the scope and
animate as `scope.stage.param`. See [`effects.md`](effects.md) for the full
parameter and identity tables.

## Shape-Specific

<!-- generated-property-table:shape-specific:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `radius_x` | `Num` | `F32` | ✓ | ✓ | `Ellipse` |
| `radius_y` | `Num` | `F32` | ✓ | ✓ | `Ellipse` |
| `shift` | `Vec2` | `Vec2` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `line_cap` | `Num` | `U32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path` |
| `line_join` | `Num` | `U32` | ✓ | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path` |
| `from` | `Vec2` | `Vec2` | ✓ | ✓ | `Line`, `Arrow`, `Callout`, `Connector` |
| `to` | `Vec2` | `Vec2` | ✓ | ✓ | `Line`, `Arrow`, `Callout`, `Connector` |
| `head_size` | `Num` | `F32` | ✓ | ✓ | `Arrow`, `Callout` |
| `points` | `List<Vec2>` | `PointList` | ✓ | ✓ | `Polygon` |
| `commands` | `Any` | `CommandList` | ✓ | ✓ | `Path` |
| `icon` | `Str` | `BuildTimeOnly` | — | — | `Path` |
| `routing` | `Str` | `String` | ✓ | ✓ | `Connector` |
| `arrow` | `Bool` | `Bool` | ✓ | ✓ | `Connector` |
| `label` | `Str` | `String` | ✓ | ✓ | `Callout` |
| `label_at` | `Vec2` | `Vec2` | ✓ | ✓ | `Callout` |
| `place` | `Str` | `Enum<auto, top, bottom, left, right, above, below>` | — | ✓ | `Callout` |
| `target` | `Any` | `BuildTimeOnly` | — | ✓ | `Callout` |
| `standoff` | `Num` | `F32` | ✓ | ✓ | `Callout` |
| `to_offset` | `Vec2` | `Vec2` | ✓ | ✓ | `Callout` |
<!-- generated-property-table:shape-specific:end -->

## Text / Typst / Code

<!-- generated-property-table:text-typst-code:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `text` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `code` | `Str` | `String` | ✓ | — | `Code` |
| `language` | `Str` | `String` | ✓ | ✓ | `Code` |
| `font_family` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Captions` |
| `font_size` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Equation`, `Captions`, `Cue`, `Legend` |
| `font_style` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Captions` |
| `font_weight` | `Num \| Str` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Captions` |
| `line_height` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Captions` |
| `letter_spacing` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Captions` |
| `word_spacing` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Captions` |
| `max_width` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst` |
| `text_max_width` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter`, `Captions`, `Legend` |
| `text_align` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `text_anchor` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `overflow` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `char_progress` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
<!-- generated-property-table:text-typst-code:end -->

`language` is the syntax-highlighting tag for `Code` (Typst's built-in `raw`
languages, e.g. `"rust"`, `"python"`, `"cpp"`; matched case-insensitively).
Empty or unrecognized renders plain — an unrecognized spelling warns and
falls back, never drops the actor. Highlighting only recolors tokens; the
un-highlighted text keeps the actor's `color`, and glyph metrics (and thus
layout) are unchanged. Token colours follow the active colorscheme (each role
maps to a named scheme colour, resolved at build like any other `color`).

### Split text

<!-- generated-property-table:split-text:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `split_by` | `Str` | `String` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `split_stagger` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `split_offset_y` | `Num` | `F32` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
| `split_mask` | `Bool` | `Bool` | ✓ | ✓ | `Text`, `Code`, `Math`, `Typst`, `Counter` |
<!-- generated-property-table:split-text:end -->

### Captions and cues

<!-- generated-property-table:captions-and-cues:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `caption_mode` | `Str` | `String` | ✓ | ✓ | `Captions` |
| `active_color` | `Color` | `Color` | ✓ | ✓ | `Captions`, `Cue` |
| `inactive_color` | `Color` | `Color` | ✓ | ✓ | `Captions` |
| `backdrop` | `Bool` | `Bool` | ✓ | ✓ | `Captions` |
| `backdrop_color` | `Color` | `Color` | ✓ | ✓ | `Captions` |
| `backdrop_radius` | `Num` | `F32` | ✓ | ✓ | `Captions` |
| `backdrop_padding` | `Vec2` | `Vec2` | ✓ | ✓ | `Captions` |
| `pop_scale` | `Num` | `F32` | ✓ | ✓ | `Captions` |
| `max_lines` | `Num` | `U32` | ✓ | ✓ | `Captions` |
| `transition_time` | `Num` | `F32` | ✓ | ✓ | `Captions` |
| `start` | `Num` | `F32` | ✓ | ✓ | `Cue` |
| `end` | `Num` | `F32` | ✓ | ✓ | `Cue` |
<!-- generated-property-table:captions-and-cues:end -->

### Counter

<!-- generated-property-table:counter:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `value` | `Num` | `F32` | ✓ | ✓ | `Counter` |
| `prefix` | `Str` | `String` | ✓ | ✓ | `Counter` |
| `suffix` | `Str` | `String` | ✓ | ✓ | `Counter` |
| `decimals` | `Num` | `U32` | ✓ | ✓ | `Counter` |
| `comma` | `Bool` | `Bool` | ✓ | ✓ | `Counter` |
<!-- generated-property-table:counter:end -->

### Equation and fragment highlights

<!-- generated-property-table:equation-and-fragment-highlights:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `highlight_color` | `Color \| Str` | `Color` | ✓ | — | `Typst`, `Equation`, `Fragment` |
| `highlight_opacity` | `Num` | `F32` | ✓ | — | `Typst`, `Equation`, `Fragment` |
| `highlight_padding` | `Num` | `F32` | ✓ | — | `Typst`, `Equation`, `Fragment` |
| `highlight_radius` | `Num` | `F32` | ✓ | — | `Typst`, `Equation`, `Fragment` |
<!-- generated-property-table:equation-and-fragment-highlights:end -->

### Text metrics (internal)

<!-- generated-property-table:text-metrics-internal:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `ascent` | `Num` | `F32` | ✓ | — | — |
| `descent` | `Num` | `F32` | ✓ | — | — |
| `baseline` | `Num` | `F32` | ✓ | — | — |
<!-- generated-property-table:text-metrics-internal:end -->

`ascent`, `descent`, and `baseline` are written by the text build path and are
not author-facing; they are listed here because they are descriptor rows.

## Media

<!-- generated-property-table:media:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `url` | `Str` | `String` | — | ✓ | `Image`, `Svg` |
| `source` | `Str` | `String` | — | ✓ | `Audio` |
| `volume` | `Num` | `F32` | ✓ | ✓ | `Audio` |
| `transcript` | `Str` | `String` | — | ✓ | `Audio`, `Captions` |
<!-- generated-property-table:media:end -->

`source` is the path to the audio asset. `volume` is a multiplier (0.0–1.0) applied at export time.
Audio actors support timing modifiers (`duration`, delay) for clip placement on the global timeline. See [`spec.md`](spec.md) §9 "Audio".

> **Note on `url` assignment:** `Image.url` and `Svg.url` assignments support
> timed keyframe animation. Image sources and SVG path sets snap at the midpoint
> of a timed assignment rather than crossfading. SVG assignments also update the
> measured actor size at the assignment keyframe.

## Plotting

<!-- generated-property-table:plotting:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `func` | `Any` | `BuildTimeOnly` | — | — | `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet` |
| `data` | `Any` | `BuildTimeOnly` | — | — | `BarChart` |
| `bar_width` | `Num \| Str` | `F32` | — | — | `BarChart` |
| `bar_colors` | `Any` | `BuildTimeOnly` | — | — | `BarChart` |
| `direction` | `Str` | `String` | — | — | `BarChart` |
| `max_value` | `Num \| Str` | `F32` | — | — | `BarChart` |
| `show_axis` | `Bool \| Str` | `BuildTimeOnly` | — | — | `BarChart` |
| `show_labels` | `Bool \| Str` | `BuildTimeOnly` | — | — | `BarChart` |
| `x_domain` | `Vec2` | `Vec2` | — | — | `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart` |
| `y_domain` | `Vec2` | `Vec2` | — | — | `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart` |
| `x_range` | `Any` | `Vec2` | — | — | `Graph`, `PlotCurve`, `NumberPlane` |
| `y_range` | `Any` | `Vec2` | — | — | `Graph`, `PlotCurve`, `NumberPlane` |
| `t_domain` | `Vec2` | `Vec2` | — | — | `PlotCurve` |
| `kind` | `Str` | `String` | — | — | `PlotCurve` |
| `resolution` | `Num` | `F32` | — | — | `PlotCurve`, `Heatmap`, `ContourSet` |
| `density` | `Num` | `F32` | — | — | `VectorField` |
| `levels` | `Any` | `Vec2` | — | — | `ContourSet` |
| `tolerance` | `Num` | `F32` | — | — | `PlotCurve` |
| `max_depth` | `Num` | `F32` | — | — | `PlotCurve`, `ContourSet` |
| `grid` | `Bool` | `Bool` | — | — | `Graph` |
| `ticks` | `Bool` | `Bool` | — | — | `Graph` |
| `tick_labels` | `Str` | `String` | — | — | `Graph` |
| `x_scale` | `Str` | `String` | — | — | `Graph` |
| `y_scale` | `Str` | `String` | — | — | `Graph` |
<!-- generated-property-table:plotting:end -->

`func` transitions use assignment modifiers, e.g. `[1s]` for output blending or
`[1s, blend: opacity]` for cross-fading the two generated plot outputs.
`func` and `data` use dedicated transition handlers rather than generic
property tracks; the timed assignment behavior is described below.

For `BarChart`, `bar_width` also accepts `"auto"` and derives its width from the
available chart space. `max_value: auto` scales to the largest data value; pin it
when comparing bar heights across data transitions.

> **Note on `data` assignment:** `BarChart.data` accepts a timed assignment that
> races the bars between two datasets, matched by label — see
> [`spec.md`](spec.md)'s BarChart section for the three limits: slots are chosen
> at the start of the window (heights animate, positions do not), captions are
> compiled at build into the declaration's slots so neither the label set nor its
> order may change, and `max_value: auto` normalises the tallest bar every frame,
> which hides the race unless pinned.

## Containers

<!-- generated-property-table:containers:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `gap` | `Num \| Str` | `F32` | — | — | `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Legend` |
| `padding` | `Num` | `F32` | — | — | `Graph`, `Row`, `Col`, `Grid`, `Stack` |
| `align` | `Str` | `String` | — | — | `Row`, `Col`, `Grid`, `Stack` |
| `cols` | `Num` | `U32` | — | — | `Grid` |
| `vertical_align` | `Str` | `String` | — | — | `Row`, `Col` |
<!-- generated-property-table:containers:end -->

`BarChart.gap` also accepts `"auto"` to derive spacing from the available chart
width.

## Scene

<!-- generated-property-table:scene:start -->
| Property | Type | Runtime kind | Animated | Assignable | Applies to |
|----------|------|--------------|----------|------------|------------|
| `background_color` | `Color \| Str` | `Color` | ✓ | ✓ | — |
| `camera_follow` | `Any` | `BuildTimeOnly` | — | — | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `parallax` | `Num` | `F32` | — | — | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
| `shared_id` | `Str` | `String` | — | ✓ | `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Text`, `Code`, `Math`, `Typst`, `Counter`, `Image`, `Svg`, `Audio`, `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`, `BarChart`, `Row`, `Col`, `Grid`, `Stack`, `Group`, `Mask`, `Filter`, `Glass`, `Equation`, `Fragment`, `Captions`, `Cue`, `Callout`, `Legend`, `Connector` |
<!-- generated-property-table:scene:end -->

`background_color` is scene-level and is assigned as `scene.background_color`;
it does not apply to actor declarations. `parallax` is read on root actors when
the scene camera transform is applied.

> **Note on `camera_follow`:** the scene camera wraps every root subtree, and
> `camera_follow: false` takes one of them back out — the HUD, watermark or
> corner label that must stay pinned to the viewport while the plate pushes in.
> It is a build-time declaration like `icon` (never keyframed, never assignable),
> it is read where the camera is applied — at the root — so declaring it on a
> nested actor is reported rather than ignored, and an opted-out `Filter` scope
> keeps its authored `bounds:` as screen coordinates. See
> [`spec.md`](spec.md)'s Scene Camera section.

---

## Value Types

| Type | Example | Notes |
|------|---------|-------|
| `F32` | `1.0`, `0.5` | 32-bit float |
| `Vec2` | `(100, 200)` | 2D vector / point |
| `Vec4` (Color) | `(1.0, 0.0, 0.0, 1.0)` | RGBA, each component 0–1 |
| `String` | `"hello"` | UTF-8 string |
| `U32` | `2` | Unsigned integer |
| `Transform` | `(1, 0, 0, 1, 0, 0)` | 6-element affine matrix |
| `PointList` | `{(0,0), (100,0)}` | List of Vec2 points |
| `CommandList` | `{move_to(0,0), line_to(100,0)}` | Path drawing commands |
| `SceneAnchor` | `scene.center` | Predefined anchor point |
| `PlacementMode` | `auto` | Layout placement strategy |
| `PositionBinding` | `at: (100, 100), anchor: scene.top_left` | Compound position spec |
| `BuildTimeOnly` | — | Evaluated at build time, not animated |

---

*Source of truth: `animatix_core::property::PROPERTY_DESCRIPTORS` declares names,
applicability, value kinds and authored-type hints. Syntax derives its input
types from those descriptors; the engine's `BINDINGS` in
`crates/animatix/src/timeline/property_registry/` supplies storage types, flags,
fields and defaults. Add the descriptor and engine binding, then regenerate
the marked tables with
`cargo run -p animatix --example property_reference -- --write`.
`docs/property-reference-groups.json` stores presentation groups only;
the docs gate checks complete coverage and generated contents.*

---

## Animatable vs assignable

`PropertyFlags::ANIMATED` decides more than keyframes: a property without it
cannot be written per frame at all. An `always` block that assigns a
non-animatable property (today: `solo`) is reported as
`always-write-not-animatable` and ignored, because the modifier runtime has no
track to write into. Set such properties in a declaration or a keyframe
assignment instead.

This is deliberate for `solo`: it selects what renders this frame, so animating
it would have to be threaded into the frame-cache key and the static-subtree key
rather than just sampled. If a property should be drivable from `always`, mark it
`ANIMATED` and make sure the caches that depend on it key on time.

## Environment Injection

Every `INJECTABLE` property is injected into the `always` evaluation environment
as `{actor}.{property}` (e.g. `ring.size`, `title.color`). The injection is driven
entirely by the property registry — adding a new `INJECTABLE` entry to
`PROPERTY_REGISTRY` is sufficient to make it available in `always` blocks.

Most properties are injected from the same storage field they write to at build
time. For properties that differ (`at`, `width`, `height`, `radius_x`, `radius_y`),
the schema's `ReadSource` declares the frame-time read strategy:

| Strategy | Schema entry | Read source |
|----------|-------------|-------------|
| Alias | `at` writes to `PositionBindingGroup` | Reads from `Position` field |
| Component | `width` writes to `Size` field | Reads `Size.x × 2` |
| Component | `height` writes to `Size` field | Reads `Size.y × 2` |
| Component | `radius_x` writes to `Size` field | Reads `Size.x × 1` |
| Component | `radius_y` writes to `Size` field | Reads `Size.y × 1` |

Each injectable property also gets an internal animation-state flag in the
environment (key shape `label.__anim__name`, see `env_keys::animating_flag`),
consumed by the `is_animating(&label.name)` query (see `spec.md` §10 Reactive
System — Property References & State Queries).

## Spatial, Presence, and Vector Path Semantics

Beyond animation and injection capabilities, `PropertyFlags` defines semantic metadata
directly on each property binding:

- `PropertyFlags::SPATIAL`: Marks properties that alter 2D spatial placement, dimensions,
  or coordinate transformations (`at`, `position`, `size`, `width`, `height`, `radius_x`,
  `radius_y`, `from`, `to`, `rotation`, `scale`, `shift`, `offset`, `transform`, `points`,
  `commands`, `min_width`, `min_height`, `max_height`).
- `PropertyFlags::PRESENCE`: Marks properties controlling visibility or presence (`opacity`).
- `PropertyFlags::PATH_AFFECTING`: Marks properties that require regenerating cached vector
  paths when animated or reassigned (`points`, `commands`, `from`, `to`, `radius_x`,
  `radius_y`, `size`, `width`, `height`, `corner_radius`, `head_size`).

This design makes the property definition in `BINDINGS` the single source of truth:
1. **Scene Statistics Exclusion (`scene.stats.*`)**: Querying `PropertySchema::affects_scene_statistics()`
   checks `is_spatial() || is_presence()`. Any actor whose spatial geometry or presence
   is dynamically modified in an `always` block is automatically excluded from build-time
   scene statistics.
2. **Element-Specific Path Properties**: Properties like `corner_radius` or `head_size` are
   flagged `PATH_AFFECTING` to trigger path rebuilds without being flagged `SPATIAL`, ensuring
   they do not accidentally disqualify actors from scene statistics.
3. **Decoupled Storage**: Storage fields (`ActorField`) remain pure storage tiers without
   semantic pattern matching or side-effect classification.

