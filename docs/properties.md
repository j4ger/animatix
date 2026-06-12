# Animatix Property Reference

> Canonical list of all actor properties, generated from `PROPERTY_REGISTRY`.
> For property value types and usage, see [`spec.md`](spec.md).

---

## Legend

| Column | Meaning |
|--------|---------|
| **Property** | Canonical name as used in source text |
| **Type** | Value type: `F32`, `Vec2`, `Vec4` (Color), `String`, `U32`, etc. |
| **Animated** | Supports keyframe animation (`✓`) or not (`—`) |
| **Assignable** | Can be set via `actor.prop = value` (`✓`) or not (`—`) |
| **Applies to** | Which actor kinds support this property |

---

## Geometry

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `position` | Vec2 | ✓ | ✓ | Everything |
| `at` | Vec2 | ✓ | ✓ | Everything |
| `offset` | Vec2 | ✓ | ✓ | Everything |
| `anchor` | SceneAnchor | ✓ | ✓ | Everything |
| `size` | Vec2 | ✓ | ✓ | Sized actors |
| `width` | F32 | ✓ | ✓ | Sized actors |
| `height` | F32 | ✓ | ✓ | Sized actors |
| `rotation` | F32 | ✓ | ✓ | Everything |
| `scale` | F32 | ✓ | ✓ | Everything |
| `transform` | Transform | ✓ | ✓ | Everything |
| `placement_mode` | PlacementMode | — | — | Everything |
| `position_binding` | PositionBinding | — | — | Everything |

## Style

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `color` | Color | ✓ | ✓ | All drawables |
| `opacity` | F32 | ✓ | ✓ | Everything |
| `fill_opacity` | F32 | ✓ | ✓ | All shapes except Line |
| `stroke` | Color | ✓ | ✓ | All shapes |
| `stroke_width` | F32 | ✓ | ✓ | All shapes |
| `stroke_progress` | F32 | ✓ | ✓ | All shapes |

## Filter

Only applicable to `Filter` actors. See [`architecture.md`](architecture.md) §6 "Post-Processing (Filter)".

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `blur` | F32 | ✓ | ✓ | Filter |
| `brightness` | F32 | ✓ | ✓ | Filter |
| `contrast` | F32 | ✓ | ✓ | Filter |
| `saturate` | F32 | ✓ | ✓ | Filter |
| `hue_rotate` | F32 | ✓ | ✓ | Filter |
| `sepia` | F32 | ✓ | ✓ | Filter |

## Shape-Specific

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `radius_x` | F32 | ✓ | ✓ | Ellipse |
| `radius_y` | F32 | ✓ | ✓ | Ellipse |
| `shift` | Vec2 | ✓ | ✓ | Everything |
| `line_cap` | U32 | ✓ | ✓ | All shapes |
| `line_join` | U32 | ✓ | ✓ | All shapes |
| `from` | Vec2 | ✓ | ✓ | Line |
| `to` | Vec2 | ✓ | ✓ | Line |
| `points` | PointList | ✓ | ✓ | Polygon |
| `commands` | CommandList | ✓ | ✓ | Path |

## Text / Math / Code

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `text` | String | ✓ | ✓ | Text |
| `math` | String | ✓ | ✓ | Math |
| `code` | String | ✓ | ✓ | Code |
| `latex` | String | ✓ | — | Deprecated |
| `font_family` | String | — | ✓ | Text, Math, Code |
| `font_size` | F32 | ✓ | ✓ | Text, Math, Code |

## Media

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `url` | String | — | ✓ | Image, Svg (note) |
| `source` | String | — | — | Audio |
| `volume` | F32 | — | — | Audio |

`source` is the path to the audio asset. `volume` is a multiplier (0.0–1.0) applied at export time.
Audio actors support timing modifiers (`duration`, delay) for clip placement on the global timeline. See [`spec.md`](spec.md) §9 "Audio".

> **Note on `url` assignment:** `Image.url` assignment supports full keyframe
> animation (timed interpolation between image sources). `Svg.url` assignment
> is currently immediate/static (not timed); SVG source changes take effect
> instantly at the assignment time without crossfade. For animated SVG, use
> re-declaration at a new keyframe.

## Plotting

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `func` | BuildTimeOnly | — | — | PlotCurve, VectorField, Heatmap, ContourSet |
| `x_domain` | Vec2 | — | — | Graph, PlotCurve, VectorField, Heatmap, ContourSet, NumberPlane |
| `y_domain` | Vec2 | — | — | Graph, PlotCurve, VectorField, Heatmap, ContourSet, NumberPlane |
| `t_domain` | Vec2 | — | — | PlotCurve |
| `kind` | String | — | — | PlotCurve |
| `resolution` | F32 | — | — | PlotCurve, Heatmap, ContourSet |
| `density` | F32 | — | — | VectorField |
| `levels` | Vec2 | — | — | ContourSet |
| `tolerance` | F32 | — | — | PlotCurve |
| `max_depth` | F32 | — | — | PlotCurve, ContourSet |
| `grid` | String | — | — | Graph |
| `ticks` | String | — | — | Graph |
| `tick_labels` | String | — | — | Graph |

## Containers

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `gap` | F32 | — | — | Row, Col, Grid |
| `padding` | F32 | — | — | Row, Col, Grid, Stack |
| `align` | String | — | — | Row, Col, Grid |
| `cols` | U32 | — | — | Grid |

## Scene

| Property | Type | Animated | Assignable | Applies to |
|----------|------|----------|------------|------------|
| `background_color` | Color | ✓ | ✓ | Scene (via `scene.background_color`) |

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

*This file is generated from `crates/animatix/src/timeline/property_registry.rs`. If you add a property, update both the registry and this table.*

---

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

Each injectable property also gets an `_animating_{name}` flag in the
environment (see `spec.md` §10 Reactive System — Animation State Flags).
