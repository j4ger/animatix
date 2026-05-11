# Animatix Examples

Runnable `.amx` demos organized by feature area. The GUI opens `showcase.amx` by default.

## Quick Reference

| Demo | Duration | Feature Focus |
|---|---|---|
| `showcase.amx` | ~10s | **Hero/overview** — layout, animation, primitives, composition |
| `primitives.amx` | ~5s | **Shape primitives** — Rect, Circle, Ellipse, Line, Arc, Polygon, Path, Arrow |
| `layout.amx` | ~5s | **Layout containers** — Row, Col, Grid, Stack, anchors, offsets |
| `animation.amx` | ~6s | **Animation actions** — fade, shift, rotate, scale, morph, sequence, stagger |
| `effects_demo.amx` | ~8s | **Effect actions** — shake, pulse, bounce, combined effect passes |
| `swap_demo.amx` | ~12s | **Container reordering** — `swap` and `reorder` actions |
| `reorder_demo.amx` | ~8s | **Explicit reorder** — `reorder` with full order specification |
| `component_actions_demo.amx` | ~8s | **Custom component actions** — user-defined `action` blocks inside components |
| `slot_demo.amx` | ~5s | **Component slots** — `@slot` markers, default content, slot fills |
| `modules.amx` | ~4s | **Imports & modules** — `import ... as`, `pub let`, namespaced access |
| `expressions.amx` | ~5s | **Expressions** — `let`, arithmetic, closures, conditionals, `rand` |
| `reactive.amx` | ~5s | **Reactive blocks** — `always`, time-driven properties, `if/else` |
| `colorschemes.amx` | ~4s | **Color themes** — `colorscheme`, `auto` colors, overrides |
| `plotting.amx` | ~6s | **Graph plotting** — Cartesian, polar, parametric, implicit curves |
| `font_demo.amx` | ~8s | **Font system** — `font_family`, runtime recompilation, dynamic sizing |
| `rotation_demo.amx` | ~5s | **Shape rotation** — `angle` property on primitives |
| `path_animation_demo.amx` | ~8s | **Path morphing** — `Polygon.points` animation, shape interpolation |

## Feature Categories

### Animation & Timing
- **`animation.amx`** — Core actions: `fade-in/out`, `shift`, `rotate`, `scale`, property animation, `sequence`, `stagger`
- **`effects_demo.amx`** — Effect actions: `shake`, `pulse`, `bounce` with staggered repeats
- **`swap_demo.amx`** — `swap` action for pairwise container reordering; bubble-sort visualization
- **`reorder_demo.amx`** — `reorder` action for explicit full-order container animation

### Components & Reuse
- **`component_actions_demo.amx`** — Custom `action` blocks inside `pub component`; `self` keyword; invocation modifiers override body modifiers
- **`slot_demo.amx`** — Component slots: `@slot` markers, `@header { ... }` fills, default slot content

### Layout
- **`layout.amx`** — `Row`, `Col`, `Grid`, `Stack`, percentage sizing, nested layouts
- **`showcase.amx`** — Combined hero scene with layout + animation + primitives

### Primitives & Media
- **`primitives.amx`** — All shapes: `Rect`, `Circle`, `Ellipse`, `Line`, `Arc`, `Polygon`, `Path`, `Arrow`, `Text`, `Math`, `Code`, `Image`, `Svg`
- **`rotation_demo.amx`** — `angle` property for rotating primitives
- **`path_animation_demo.amx`** — Morphing `Polygon.points` over time

### Language Features
- **`expressions.amx`** — `let` bindings, tuple math, `sin`/`cos`, `lerp`, modulo, closures, conditionals
- **`reactive.amx`** — `always` blocks, `t` variable, live property updates
- **`modules.amx`** — `import "./file.amx" as name`, `pub let` exports, dotted access (`theme.accent`)
- **`colorschemes.amx`** — Theme system with `auto` color derivation

### Specialized
- **`plotting.amx`** — `Graph`, `CartesianPlot`, `PolarPlot`, `ParametricPlot`, `ImplicitPlot`
- **`font_demo.amx`** — `font_family` selection, runtime text recompilation, dynamic `font_size`

## Helper Modules (not standalone demos)

| File | Purpose |
|---|---|
| `templates/slide_layout.amx` | Reusable `pub component` template with `@slot` markers |
| `modules/palette.amx` | `pub let` color exports for the modules demo |
| `modules/card.amx` | Reusable `pub component Card(title: ...)` for the modules demo |

## Assets

- `checker.ppm` — raster image for `Image`/`Svg` demos
- `vector.svg` — vector graphic for `Svg` demo

## Running Demos

```bash
# Render a still frame
cargo run --bin animatix -- image examples/showcase.amx

# Export GIF
cargo run --bin animatix -- gif examples/showcase.amx -o out.gif --fps 15

# View AST
cargo run --bin animatix -- ast examples/showcase.amx --compact

# Open GUI (opens showcase.amx by default)
cargo run --bin animatix-gui
```

## Writing New Demos

When adding a demo:
1. Keep it under 60 lines if possible
2. Use `config { colorscheme: "...", resolution: (1280, 720) }`
3. Include a header comment explaining what it demonstrates
4. Add to both the **Quick Reference** table and the **Feature Categories** section above
5. Verify it parses: `cargo run --bin animatix -- ast examples/your_demo.amx`
