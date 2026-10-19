# Primitives

Quick reference for all Animatix primitives. For the full language specification, see [`spec.md`](spec.md). For the complete property registry, see [`properties.md`](properties.md).

---

# 1. Scene Primitives

## Text

**Properties:**
- `text`: String
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`

**Shorthand:** `title: "Hello World"` desugars to `Text { text: "Hello World", ... }`

**Example:**
```animatix
title: Text { text: "Hello World", font_size: 24, at: (640, 120) }
```

Timed `text` assignments cross-fade the source and target glyph sets instead of snapping at the midpoint; `Typst.content` follows the same behavior.

## Typst

Replaces the deprecated `Math` primitive.

**Properties:**
- `content`: String — Typst markup content (accepts `text`, `math`, `code`, `latex` for backward compatibility)
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`
- `highlight_color`: Color — color of whole-actor highlight overlay (default `white`)
- `highlight_opacity`: Number — opacity of highlight, 0 = hidden, 1 = full (default `0.0`)
- `highlight_blend`: String — blend mode: `difference`, `exclusion`, `normal`, `multiply`, `screen` (default `"difference"`)
- `highlight_padding`: Number — padding around actor bounding box (default `4.0`)
- `highlight_radius`: Number — corner radius of highlight rectangle (default `2.0`)

**Shorthand:** `$$ ... $$` desugars to a `Typst` actor with the content taken as raw text (unquoted). A label is required. Modifiers are supported.

```animatix
eq: $$ x^2 + y^2 $$                    // desugars to: eq: Typst, content: "x^2 + y^2"
eq: $$ x^2 $$ [2s, ease: bounce]       // with modifiers
```

**Actions:**
- `highlight target [color: C, blend: B, padding: P, radius: R, duration, ease]` — animate `highlight_opacity` from 0 to 1 (whole-actor highlight)
- `unhighlight target [duration, ease]` — animate `highlight_opacity` from current to 0

This is distinct from Equation+Fragment per-segment highlighting — a Typst actor is highlighted as a single unit.

**Example:**
```animatix
eq: Typst, content: "x^2 + 3", font_size: 18, at: (640, 360)

#2s
  highlight eq [color: yellow, blend: multiply, 800ms]

#4s
  unhighlight eq [400ms]
```

## Code

Renders via the text-path pipeline (Typst `raw` block). Setting `language` to
one of Typst's built-in highlighting languages turns on syntax highlighting.
Token colours follow the active colorscheme: each token role is mapped to a
named scheme colour (keyword→`accent.danger`, function→`accent.primary`,
string→`accent.success`, number→`accent.secondary`, comment→`text.muted`, …),
resolved once at build like any other primitive's `color`. The un-highlighted
text keeps the actor's `color`, and glyph metrics (and thus layout) are
unchanged. An empty or unrecognized `language` renders plain (a warning is
logged for the latter, never an error).

**Properties:**
- `code`: String
- `language`: String — highlighting language tag (`"rust"`, `"python"`, `"cpp"`, …; case-insensitive)
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`

**Example:**
```animatix
snippet: Code { code: "let velocity = x + 1", language: "rust", font_size: 28, at: (640, 360) }
```

## Counter

An animated rolling counter. Digit glyphs are prebaked once and rolled vertically, and every digit column shares the widest digit's advance so proportional fonts do not jitter as the value changes.

**Properties:**
- `value`: Number — the displayed number; animate or assign it to roll the digits (default `0`)
- `prefix`: String — text prepended to the number
- `suffix`: String — text appended to the number
- `decimals`: Number — decimal places to display (default `0`)
- `comma`: Boolean — insert thousands-grouping commas (default `false`)
- `font_size`: Number
- `color`: Color
- `at`: Tuple `(x, y)`

`value`, `decimals`, and `comma` are sampled every frame, so they animate; `prefix` and `suffix` are compiled once at build time and stay fixed.

**Example:**
```animatix
kpi: Counter, value: 1250, font_size: 48, color: accent.primary, suffix: " req/s", comma: true, at: (640, 360)

#2s
kpi.value = 8490 [3s, ease: ease-in-out]
```

## Svg

**Properties:**
- `url`: String
- `scale`: Number
- `at`: Tuple `(x, y)` or scene-relative percent tuple `(72%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`

**Example:**
```animatix
icon: Svg { url: "examples/assets/animatix-mark.svg", scale: 1.5, at: (640, 600) }
```

Note: Missing files or invalid SVG contents report build diagnostics. Source changes require re-declaration at a keyframe (assignment not yet supported).

## Image

**Properties:**
- `url`: String
- `at`: Tuple `(x, y)` or percent tuple `(30%, 38%)`
- `anchor`: Scene anchor
- `offset`: Tuple `(x, y)`
- `size`: Optional tuple `(width, height)` — defaults to intrinsic pixel size

**Example:**
```animatix
photo: Image { url: "examples/assets/checker.png", at: (640, 360), size: (180, 180) }
```

Note: Missing files report build diagnostics. Source changes are discrete (crossfade requires manual opacity layering).

## Audio

A non-visual actor: it embeds an audio file into the exported video and can anchor keyframes to its transcript. It draws nothing.

**Properties:**
- `source`: String — path to the audio file, relative or absolute; a missing source warns and plays nothing (default `""`)
- `volume`: Number — playback volume multiplier (default `1.0`)
- `transcript`: String — path to a transcript file (Universal JSON, Whisper JSON, WebVTT, or SRT); its word and marker timings become keyframe anchors `#label@marker` and `#label["word"]`

**Timing modifiers:** the first bare time literal (for example `[5s]`) trims playback to that duration — without it the whole file plays; `delay: <time>` shifts the start on the global timeline. Named `duration:` modifiers are rejected.

All `Audio` actors in a scene (or composition) are mixed together during export, respecting each clip's `volume` and start offset.

**Example:**
```animatix
voice: Audio, source: "voiceover.mp3", transcript: "voiceover.speech.json", volume: 0.8
```

## Rect

**Properties:**
- `size`: Tuple `(width, height)` — general rectangle dimensions
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Rectangle with explicit size
r: Rect, size: (160, 80), color: blue, at: (400, 300)

# Square
sq: Rect, size: (120, 120), color: green, at: (400, 500)
```

## Ellipse

**Properties:**
- `size`: Tuple `(width, height)` — bounding box dimensions, converted to `radius_x = width / 2`, `radius_y = height / 2`
- `radius_x`: Number — horizontal radius
- `radius_y`: Number — vertical radius
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Circle
c: Ellipse, size: (100, 100), color: red, at: (200, 200)

# Ellipse with explicit radii
halo: Ellipse, radius_x: 90, radius_y: 40, color: cyan, at: (640, 360)

# Ellipse via size tuple
halo: Ellipse, size: (180, 80), color: cyan, at: (640, 360)

# Dot
dot: Ellipse, size: (6, 6), color: gold, at: (320, 240)
```

## Line

Stroke-oriented — no fill property.

**Properties:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Plain line
axis: Line, from: (-120, 0), to: (120, 0), stroke: white, stroke_width: 4, at: (640, 360)
```

## Arrow

Stroke-oriented like `Line`, but with a filled triangular arrowhead. The head is painted with the same stroke color, so there is no separate fill.

**Properties:**
- `from`: Tuple `(x, y)` in local actor coordinates
- `to`: Tuple `(x, y)` in local actor coordinates
- `head_size`: Number — arrowhead length in scene pixels, clamped to at least 1 (default 10)
- `stroke` / `stroke_color`: Color — shaft and arrowhead
- `stroke_width`: Number
- `at`: Tuple `(x, y)`

`from` and `to` also accept an `actor.anchor` reference (for example `to: box.left`), resolved against the target's bounds each frame. Anchor names are the `scene.*` vocabulary: `top_left`, `top`, `top_right`, `left`, `center`, `right`, `bottom_left`, `bottom`, `bottom_right`.

**Example:**
```animatix
arrow: Arrow, from: (-40, 20), to: (40, -20), stroke: accent.primary, stroke_width: 4, head_size: 16
```

## Polygon

**Properties:**
- `points`: Tuple/list of point tuples — explicit vertex list
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Examples:**
```animatix
# Explicit vertex list
badge: Polygon, points: {(-80, 0), (0, -70), (90, 0), (0, 80)}, color: cyan, at: (640, 360)

# Regular hexagon
hex: Polygon, points: {(-70, 0), (-35, -60), (35, -60), (70, 0), (35, 60), (-35, 60)}, color: cyan, at: (640, 360)
```

## Path

Uses structured commands (`move_to(...)`, `line_to(...)`, `quad_to(...)`, `curve_to(...)`, `close()`).

**Properties:**
- `commands`: Tuple/list of path commands
- `color`: Color
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `fill_opacity`: Number
- `stroke_progress`: Number
- `at`: Tuple `(x, y)`

**Example:**
```animatix
guide: Path, commands: {
  move_to(-120, 0),
  line_to(-40, -80),
  curve_to(20, -120, 80, 40, 140, -10),
  close()
}, stroke: white, stroke_width: 4, at: (640, 360)
```

## Captions

Container for speech-synced subtitles. Cues come from `Cue` children or from a transcript file; the container draws the active cue and styles it, and only while a cue is active does anything render.

**Properties:**
- `transcript`: String — path to a transcript file (`.json`, `.vtt`, `.srt`); parsed at build and used when the container has no `Cue` children
- `caption_mode`: String — `"karaoke"`, `"pop"`, or `"line"`; any other value highlights only the current word
- `active_color`: Color — color of the current word (in `"karaoke"`, also of words already spoken)
- `inactive_color`: Color — color of the not-yet-spoken words
- `backdrop`: Boolean — draw a rounded backdrop box behind the cue text
- `backdrop_color`: Color — backdrop fill
- `backdrop_radius`: Number — backdrop corner radius (default 8)
- `backdrop_padding`: Tuple `(x, y)` — padding around the text inside the backdrop (default `(16, 8)`)
- `pop_scale`: Number — scale multiplier applied to the current word in `"pop"` mode
- `font_size`: Number — cue text size (default 32)
- `text_max_width`: Number — wrap width in pixels for the cue text; `0` (default) leaves it on one line
- `max_lines`: Integer — cap on how many *wrapped* lines a cue may show; `0` means no cap (default 2). Needs `text_max_width` to wrap first
- `transition_time`: Number — seconds to fade a cue in from its own start; `0` (default) snaps
- `color`: Color — base text color, used by `"line"` mode
- `at`: Tuple `(x, y)` — defaults to the lower part of the scene, `(width / 2, height * 0.85)`

Word timings for a `Cue` child are distributed evenly across its text; a transcript supplies real word timings.

**Example:**
```animatix
subs: Captions,
  at: (640, 580),
  transcript: "narration.speech.json",
  caption_mode: "karaoke",
  active_color: accent.primary,
  inactive_color: text.muted,
  font_size: 28,
  backdrop: true,
  backdrop_color: (0.05, 0.08, 0.14, 0.92),
  backdrop_radius: 14,
  backdrop_padding: (28, 14),
  pop_scale: 1.15
```

## Cue

A leaf subtitle cue inside a `Captions` container. Rendering is entirely the container's — a `Cue` never draws on its own — so its styling (`font_size`, colors, backdrop) comes from the parent.

**Properties:**
- `text`: String — the cue's subtitle text
- `start`: Number — cue start time in seconds
- `end`: Number — cue end time in seconds
- `active_color`: Color — per-cue override of the container's `active_color`

`start` and `end` are both required: a cue is shown only while `start <= t < end`, and both default to `0`, so a cue that omits either one is never visible.

**Example:**
```animatix
subs: Captions, at: (640, 580), caption_mode: "pop", font_size: 28 {
  cue1: Cue, text: "Welcome to Animatix.", start: 0.0, end: 2.4
  cue2: Cue, text: "Captions follow every word.", start: 2.4, end: 5.0
}
```

---

# 2. Graph Primitives

## Icon names

`Path` also accepts `icon: "<name>"` as build-time sugar for an authored
`commands:` list — 25 Lucide-derived stroke marks on a 24×24 grid, origin at the
top-left. Trace them on with `draw-in`; size them with `scale:`. See
[`spec.md`](spec.md) §Recipes for the full name list and the arc exclusion.

## Graph

A coordinate container that maps child actor positions to math coordinates. Use when you need to plot data (curves, vectors, etc.) with automatic coordinate mapping.

**Properties:**
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`
- `grid`: Boolean — draw grid lines at regular intervals
- `ticks`: Boolean — draw tick marks on axes
- `tick_labels`: `"auto" | "true" | "false" | "x" | "y" | "both"` — draw numeric labels on axes

## PlotCurve

Single-stroke curve plot. The `kind` property selects the sampling strategy.

**Properties:**
- `kind`: `"cartesian" | "polar" | "parametric" | "implicit"`
- `func`: Closure — signature depends on `kind`
  - `"cartesian"`: `(x) => y`
  - `"polar"`: `(theta) => r`
  - `"parametric"`: `(t) => (x, y)`
  - `"implicit"`: `(x, y) => scalar`
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `t_domain`: Tuple `(min, max)` — used by `"polar"` and `"parametric"`
- `color` / `stroke`: Color
- `width` / `stroke_width`: Number
- `tolerance`: Number — adaptive subdivision threshold
- `max_depth`: Number — max recursion depth for adaptive sampling
- `resolution`: Number — sampling grid resolution for `"implicit"`

**Function Transitions**

All plot primitives support animated transitions between functions using timed `func` assignments:

```animatix
curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke: accent.primary

#2s
curve.func = (x) => cos(x) [1s, ease: ease-in-out]
```

The default transition blends evaluated outputs at each sample point: `y = lerp(from(x), to(x), progress)`. This works for cartesian, polar, parametric, and implicit `PlotCurve`, plus `VectorField`, `Heatmap`, and `ContourSet` (their scalar/vector fields are blended per sample).

To cross-fade the two rendered plot outputs instead of blending function values, add `blend: opacity` to the assignment:

```animatix
curve.func = (x) => cos(x) [1s, blend: opacity]
```

`blend: output` is the default and can be written explicitly.

**Overlapping transitions:** If a new transition starts before the previous completes, the system freezes the current blend state and chains to the new target:

```animatix
#1s
curve.func = (x) => cos(x) [2s]  // 1s to 3s

#2s
curve.func = (x) => x^2 [1s]  // freezes at 50%, chains to x^2
```

See `examples/data/24_plot_transitions.amx` for demonstrations.

## VectorField

Grid-sampled vector field rendered as arrows.

**Properties:**
- `func`: Closure `(x, y) => (dx, dy)` — returns a vector at each point
- `density`: Number — grid resolution (default 16)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color` / `stroke`: Color

## Heatmap

Pixel-level scalar field visualization using colored rectangles.

**Properties:**
- `func`: Closure `(x, y) => scalar`
- `resolution`: Number — grid resolution (default 64)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color`: Color — used as the "hot" color (alpha varies by scalar value)

## ContourSet

Multiple level-set curves for a scalar function.

**Properties:**
- `func`: Closure `(x, y) => scalar`
- `levels`: Tuple of numbers — e.g. `(-2, 0, 2)`
- `resolution`: Number — sampling grid resolution (default 96)
- `x_domain`: Tuple `(min, max)`
- `y_domain`: Tuple `(min, max)`
- `size`: Tuple `(width, height)`
- `color` / `stroke`: Color

## NumberPlane

A standalone visual coordinate plane with auto-generated axes, grid lines, and tick marks. Use when you need a visual background grid/axes without hosting child plots.

**Properties:**
- `x_domain`: Tuple `(min, max)` — visible x-axis range
- `y_domain`: Tuple `(min, max)` — visible y-axis range
- `x_range`: Tuple `(min, max, step)` — grid/ticks x placement range and interval
- `y_range`: Tuple `(min, max, step)` — grid/ticks y placement range and interval
- `size`: Tuple `(width, height)`
- `at`: Tuple `(x, y)`
- `stroke` / `stroke_color`: Color — axis and grid color

Grid lines are drawn at each step interval within the specified range. Axes (horizontal at y=0, vertical at x=0) are drawn with thicker strokes. Tick marks appear at each step interval on both axes.

**Example:**
```animatix
plane: NumberPlane,
  x_domain: (-6, 6), y_domain: (-6, 6),
  x_range: (-6, 6, 2), y_range: (-6, 6, 2),
  size: (400, 400), at: (640, 360)
```

**Example:**
```animatix
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400), at: (400, 300), grid: true, ticks: true {
  parabola: PlotCurve, kind: "cartesian", func: (x) => x^2 + 3, color: red, width: 2,
  rose: PlotCurve, kind: "polar", func: (t) => 3 * sin(4 * t), t_domain: (0, 6), stroke: green, width: 2
}

graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  lissajous: PlotCurve, kind: "parametric", func: (t) => (sin(2 * t), cos(3 * t)), t_domain: (0, 6.28), stroke: cyan, width: 3
}

graph: Graph, x_domain: (-2, 2), y_domain: (-2, 2), size: (360, 360), at: (640, 360) {
  circle: PlotCurve, kind: "implicit", func: (x, y) => x * x + y * y - 1, resolution: 96, stroke: cyan, width: 3
}
```

---

## BarChart

A bar chart / column chart primitive for data visualization. Produces a set of
rectangular bars whose heights represent data values, with an optional baseline
axis. Supports standalone (pixel coordinates) and `Graph`-child (math coordinates)
modes.

**Properties:**
- `data`: Brace list of `(key, value)` tuples — the bar data. Assignable on a
  timed keyframe: `chart.data = {…} [800ms]` races the bars between the two
  datasets, matched by label (see [`spec.md`](spec.md)'s BarChart section for the
  three limits that come with it)
- `size`: Tuple `(width, height)` — chart visual bounds
- `bar_width`: Number or `"auto"` — per-bar width (default auto-distributes)
- `gap`: Number or `"auto"` — spacing between bars (default auto)
- `bar_colors`: `"auto"`, one color, or a brace list of colors — per-bar fill colors; colors may be RGBA tuples or scheme tokens such as `accent.danger`, `accent.success`, and `accent.warning`
- `show_axis`: Bool or string `"true"` | `"false"` — show baseline axis (default `"true"`)
- `show_labels`: Bool or string `"true"` | `"false"` — show bar labels (default `"true"`)
- `direction`: `"vertical"` | `"horizontal"` — bar orientation (default `"vertical"`, horizontal reserved)
- `max_value`: Number or `"auto"` — y-axis scale cap (default auto)
- `color`: Color — fallback bar fill color
- `stroke` / `stroke_color`: Color — bar outline color
- `stroke_width`: Number — bar outline width
- `x_domain`: Tuple `(min, max)` — math x-range (inherited from parent `Graph`)
- `y_domain`: Tuple `(min, max)` — math y-range (inherited from parent `Graph`)
- `at`: Tuple `(x, y)` — chart position
- `opacity`: Number — chart opacity

**Standalone example:**
```animatix
spectrum: BarChart,
  data: {("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3)},
  size: (600, 260),
  bar_colors: {accent.danger, accent.success, accent.warning},
  show_axis: true,
  show_labels: true,
  at: (640, 420)
```

**Inside a Graph:**
```animatix
graph: Graph, x_domain: (0, 12), y_domain: (0, 1.1), size: (700, 300) {
  spectrum: BarChart,
    data: {(2, 1.0), (5, 0.55), (9, 0.3)},
    bar_width: 0.8,
    show_axis: false,
    show_labels: false,
    bar_colors: {accent.danger, accent.success, accent.warning}

  envelope: PlotCurve,
    kind: "cartesian",
    func: (x) => exp(-0.2 * x),
    color: text.muted
}
```

---

## Equation

Container primitive for typeset equations with individually highlightable fragments.
Children are `Fragment` primitives whose content is concatenated and compiled as a
single Typst equation, preserving correct layout and spacing.

**Properties:**
- `font_size`: Number — font size in points (default 18)
- `color`: Color — default text color for all fragments (default `text.primary`)
- `at`: Tuple `(x, y)` — position (default `(0, 0)`)

**Example:**
```animatix
eq: Equation, font_size: 22, color: text.muted, at: (0, -230) {
  pre: Fragment, content: "x(t) = "
  f1: Fragment, content: "sin(2 pi dot 2t)"
  mid: Fragment, content: " + "
  f2: Fragment, content: "sin(2 pi dot 5t)"
}
```

## Fragment

Child primitive of `Equation`. Represents a named, addressable segment of the equation.
Does not render independently — the parent Equation handles all rendering.

**Properties:**
- `content`: String — Typst math content for this segment (default `""`)
- `highlight_color`: Color — color of highlight overlay rectangle (default `white`)
- `highlight_opacity`: Number — opacity of highlight, 0 = hidden, 1 = full (default `0.0`)
- `highlight_blend`: String — blend mode: `difference`, `exclusion`, `normal`, `multiply`, `screen` (default `"difference"`)
- `highlight_padding`: Number — padding around fragment bounding box (default `4.0`)
- `highlight_radius`: Number — corner radius of highlight rectangle (default `2.0`)

**Actions:**
- `highlight target [color: C, blend: B, padding: P, radius: R, duration, ease]` — animate `highlight_opacity` from 0 to 1
- `unhighlight target [duration, ease]` — animate `highlight_opacity` from current to 0

**Example:**
```animatix
#2s
  highlight eq.f1 [color: white, blend: difference, 800ms]

#4s
  unhighlight eq.f1 [400ms]
```

---

# 3. Containers

Auto-layout-first model with declaration-time measure/place contract. Explicit `at` opts into handcrafted placement.

## Row

**Properties:**
- `gap`: Number
- `padding`: Number
- `align`: `start | center | end`

## Col

**Properties:**
- `gap`: Number
- `padding`: Number
- `align`: `start | center | end`

## Group

Participates in scene graph and transform inheritance. Does not run a layout algorithm.

## Grid

Structured two-dimensional layout with `cols` and `gap` for deterministic declaration-order placement.

**Properties:**
- `cols`: Number
- `gap`: Number
- `padding`: Number

## Stack

Layered composition. Overlaps layout-managed children around a shared origin.

**Properties:**
- `gap`: Number
- `padding`: Number

Root layout containers can omit `at` and default to `scene.center`. Scene-relative placement via `anchor: scene.*`, `offset`, and percentage-based `at` is supported.

## Filter

Post-processes its children with `blur`, `brightness`, `contrast`, `saturate`,
`hue_rotate`, and `sepia`. Children render into an offscreen target that is filtered
and composited back. Without a GPU filter backend the effect is skipped, the children
render unfiltered, and a runtime diagnostic is emitted.

## Mask

Clips its children to a shape. By default the clip is the Mask's own rectangular
bounds; a child labelled `clip_shape` defines the clip geometry instead:

```animatix
m: Mask, size: (120, 90), at: (400, 300) {
  clip_shape: Ellipse, size: (60, 60)
  content: Rect, size: (300, 220), color: accent.primary
}
```

The `clip_shape` child is not painted itself. Its own transform applies to the clip
region, so `at`, `anchor`, `offset`, `rotation`, and `scale` on it move and scale the
clip. Any primitive that produces vector geometry can be a clip shape — the built-in
shapes (`Rect`, `Ellipse`, `Polygon`, `Path`, `Line`, `Arrow`, `Svg`) and extension
primitives that implement `clip_path`. A shape that produces no geometry (for example
`Text`) makes the Mask warn and fall back to a rectangular clip. Plot geometry is not
a clip provider — `Graph`, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`,
`NumberPlane`, and `BarChart` are time-varying/`stroke_progress`-trimmed, so using one
as a `clip_shape` warns and falls back to a rectangular clip.

## Glass

A backdrop-blur scope. Whatever the frame already holds behind the scope's rect is blurred, and the scope's children composite above that blur. It paints no surface of its own — the panel you see is normally its first child, because a fill or border drawn by the scope would sit inside the pixels that get frosted.

**Properties:**
- `size`: Tuple `(width, height)` — the frosted region
- `corner_radius`: Number — radius the frosted region is clipped to (default 0)
- `at`: Tuple `(x, y)`

The backdrop chain is declared as effect children exactly like a `Filter`'s (`frost: Blur, radius: 18`) and animates as `scope.stage.param`. Surface properties (`color`, `stroke`, `fill_opacity`, …) are reported inapplicable because the scope paints nothing; `corner_radius` is not, since the frost's clip consumes it. Without a GPU filter backend the scope renders its children unfiltered and emits a runtime diagnostic.

**Example:**
```animatix
card: Glass, at: (640, 360), size: (560, 240), corner_radius: 24 {
  frost: Blur, radius: 0
  surface: Rect, size: (560, 240), corner_radius: 24, color: "#ffffff1a", stroke: "#ffffff40", stroke_width: 1.5
  heading: Text, text: "Backdrop blur", font_size: 40, at: (-180, -34)
}

#1.2s
frost.radius = 18 [900ms, ease: ease-in-out]
```

---

# 4. Annotations

## Callout

A leader line with an arrowhead and an optional text label. In manual mode `from`/`to` set the endpoints; in targeted mode `target` attaches the tip to a named actor's edge automatically.

**Properties:**
- `from`: Tuple `(x, y)` — tail, manual mode
- `to`: Tuple `(x, y)` — arrow tip, manual mode
- `head_size`: Number — arrowhead length in scene pixels (default 10)
- `label`: String — label text drawn near the tip (an empty label draws nothing)
- `label_at`: Tuple `(x, y)` — label offset from the tip (default `(0, 50)`)
- `color`: Color — label text color
- `stroke` / `stroke_color`: Color — leader line and arrowhead
- `stroke_width`: Number
- `at`: Tuple `(x, y)`
- `target`: Actor reference — targeted mode: resolve the tip onto this actor's bounds (an unknown name is a build-time `CalloutTargetNotFound` diagnostic). Actor arrays use source syntax, e.g. `target: bars[2]`
- `place`: `top | bottom | left | right | auto` — which edge of the target the tip attaches to (default `right`; `auto` currently resolves to the right edge, and `above`/`below` are aliases for top/bottom)
- `standoff`: Number — gap in scene pixels between the tip and the target edge (default 40)
- `to_offset`: Tuple `(x, y)` — fine adjustment applied after placement (default `(0, 0)`)

Target resolution composes the target's full ancestor transform chain, so a callout attaches correctly to a nested, scaled, or rotated actor. Label text compiles at 24 pt by default.

**Example:**
```animatix
box1: Rect, at: (300, 200), size: (120, 60), color: (0.2, 0.6, 0.9, 1.0)

note: Callout {
  from: (500, 400),
  target: box1,
  place: bottom,
  standoff: 8,
  to_offset: (0, 0),
  label: "Tracked Actor",
  label_at: (0, -20),
  stroke: accent.primary
}
```

## Connector

An orthogonal connector between two actor anchors (or explicit points). Endpoints are resolved against the targets' bounds each frame, so a connector follows actors as they move.

**Properties:**
- `from`: Endpoint — `(x, y)`, `(actor, anchor)`, `actor` (auto side), or `actor.anchor` (default `(0, 0)`)
- `to`: Endpoint — same forms as `from` (default `(100, 100)`)
- `routing`: String — `"elbow"` (default), `"l-bend"`, or `"straight"`
- `arrow`: Boolean — draw an arrowhead at `to` (default `true`)
- `corner_radius`: Number — fillet radius at each bend (default 8)
- `head_size`: Number — arrowhead length in scene pixels (default 10)
- `stroke` / `stroke_color`: Color
- `stroke_width`: Number
- `at`: Tuple `(x, y)`

When no anchor is named (`to: database`), the side facing the other endpoint is chosen automatically. Anchor names are the `scene.*` vocabulary (`left`, `right`, `top`, `bottom`, the corners, and `center`).

**Example:**
```animatix
client: Rect, at: (340, 420), size: (160, 80)
database: Rect, at: (940, 420), size: (160, 80)

conn: Connector,
  from: (client, right),
  to: (database, left),
  routing: "elbow",
  corner_radius: 8,
  arrow: true,
  stroke: accent.primary,
  stroke_width: 2
```

## Legend

An annotation that auto-generates a color-coded legend from the scene's color-bearing actors. Entries are collected after the build; actors opt in or out with the generic `legend` property, and an auto label is derived from the actor label.

**Properties:**
- `at`: Tuple `(x, y)` — top-left corner of the legend (default `(100, 100)`)
- `title`: String — optional heading above the entries
- `font_size`: Number — label font size in points (default 14)
- `label_color`: Color or `auto` — label color; `auto` picks a contrasting color from the scene background
- `swatch_size`: Number — color swatch size in scene units (default 16)
- `gap`: Number — vertical gap between rows and between title and entries (default 8)
- `text_max_width`: Number — label wrap width, `0` disables wrapping (default 240)

The scan includes color-bearing leaf actors (shapes, `PlotCurve`, `VectorField`, `Heatmap`, `ContourSet`, `BarChart`) and excludes structural actors (graphs, grids, containers, other legends) and full-viewport backgrounds. An actor sets `legend: "Label"` to name its entry, `legend: false` to opt out, and `legend: true` to force inclusion.

**Example:**
```animatix
line_a: Line, from: (100, 100), to: (200, 100), color: red, stroke_width: 4, legend: "Series A"
line_b: Line, from: (100, 150), to: (200, 150), color: green, stroke_width: 4, legend: "Series B"

legend: Legend, at: (320, 100), title: "Series", swatch_size: 16, gap: 8, text_max_width: 240
```

---

# 5. Common Animated Properties

Runtime supports explicit assignment for: `color`, `stroke`, `stroke_width`, `stroke_progress`,
`fill_opacity`, `size`, `at`/`position`, `radius_x`, `radius_y`, `from`, `to`, `scene.background_color`,
and the paint/compositing set below.

**Stroke and compositing vocabulary** (see [`properties.md`](properties.md#style) for the full
rows):

| Property | Applies to | What it does |
|----------|-----------|--------------|
| `dash_pattern` | stroke paths | Brace list of alternating segment/gap lengths in scene pixels — `dash_pattern: {8, 6}` |
| `dash_offset` | stroke paths | Phase into the pattern; animating it is the marching-ants idiom |
| `blend` | everything | CSS mix-blend mode name (`"screen"`, `"multiply"`, …); composites the actor's whole subtree against what is behind it |
| `fill_gradient` | shapes except `Line`, and text-like actors (`Text`, `Math`, `Typst`, `Code`, `Counter`) | Ramp replacing the solid fill: `linear(angle, stops)`, `radial(center, radius, stops)`, `sweep(angle, stops)` |
| `stroke_gradient` | stroke paths | Same, on the stroke |
| `gradient_extend` | shapes, text-like actors, and stroke paths | `"pad"` (default), `"repeat"`, `"reflect"` |
| `gradient_space` | shapes, text-like actors, and stroke paths | `"oklab"` (default — no dark band across saturated hue ramps) or `"srgb"` |

Gradient geometry is normalized to the painted actor's bounding box (for text,
the combined bounding box across all glyphs), so a ramp follows an actor that
resizes. Ramps interpolate between keyframes when the two values share a kind and
Both dash and gradient ride *outside* the shape-command memo — an animated dash
offset or ramp would otherwise be served stale from a cached encoding, which is
why a scene that uses neither pays nothing for them.

`stroke_progress` (what `draw-in` writes) trims stroke-only geometry by arc
length on any shape, not just plots; closed and filled shapes keep the
fill-reveal semantics instead.

Text/Typst/Code use text-path keyframes; shapes use vector-path keyframes.

Nested property targeting via dotted paths works on both sides: `left.badge.color = red`
(assignment) and `copy.at = left.badge.at` (read). Component reads like `source.at.x`
are supported.

Geometry inputs (`points`, `commands`) are now fully animated tracks; assignments with
duration trigger path morphing via `vector_paths` interpolation.

Unsupported assignments report build diagnostics rather than silent ignore.

For bracket modifier details (`duration`, `delay`, `ease`, morphing strategies), see
[`spec.md`](spec.md).

---

# 6. Known Media Gaps

- `Svg.url` source assignment now supports timed keyframe transitions like `Image.url`; SVG path sets snap at the midpoint of a timed assignment and update measured size.