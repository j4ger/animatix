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

Only applicable to `Filter` actors. See [`design/filter-system.md`](design/filter-system.md).

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
| `url` | String | — | ✓ | Image, Svg |
| `source` | String | — | — | Audio |
| `volume` | F32 | — | — | Audio |

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
