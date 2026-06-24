# Animatix Examples

A progressive suite of 24 numbered `.amx` demos (00–23). Each builds on the previous, introducing language features in isolation before the final capstone combines everything.

## Quick Reference

| # | File | What It Shows |
|---|------|---------------|
| 00 | `00_hello.amx` | **Minimal scene** — title card with staggered fade-in of 4 actors |
| 01 | `01_shapes.amx` | **Primitives** — Rect, Ellipse, Polygon, Path, Image, Typst, Text, Code |
| 02 | `02_layout.amx` | **Containers** — Row, Col, Grid, Stack with animation |
| 03 | `03_timing.amx` | **Timing** — sequence, stagger, easing curves |
| 04 | `04_motion.amx` | **Actions + reactive** — shift, rotate, scale, fade; `always` expressions |
| 05 | `05_morph.amx` | **Morphing** — re-declaration triggers shape interpolation |
| 06 | `06_reactive.amx` | **Reactive always** — `always` expressions + `_animating_*` flag detection |
| 07 | `07_plots.amx` | **Data viz** — Graph, PlotCurve, VectorField, Heatmap |
| 08 | `08_effects.amx` | **Effects** — Filter primitive, shake, pulse, bounce |
| 09 | `09_components.amx` | **Components** — `pub component`, `@slot` markers, typed params, dotted assignment |
| 10 | `10_modules.amx` | **Modules** — `import ... as`, `pub let`, re-export chains, design tokens |
| 11 | `11_colors.amx` | **Color system** — semantic tokens, `auto`, built-in constants, `Colorscheme` with `extends` |
| 12 | `12_reorder.amx` | **Reorder** — `swap`, `reorder` with `dynamic_layout` |
| 13 | `13_paths.amx` | **Paths** — bezier curves, polygon morphing, affine transform matrices |
| 14 | `14_multiscene.amx` | **Multi-scene** — scene declarations, transitions, per-scene config overrides |
| 15 | `15_for_loop.amx` | **For loop** — procedural per-frame wave with `for` inside `always` |
| 16 | `16_showcase.amx` | **Showcase** — layout, morphing, paths, transforms, `always` combined |
| 17 | `17_audio_reactive.amx` | **Audio reactive** — embedded soundtrack, `Audio` primitive, waveform bars |
| 18 | `18_number_plane_contours.amx` | **NumberPlane** — grid backdrop, ContourSet level curves, VectorField |
| 19 | `19_cross_file_scenes.amx` | **Cross-file scenes** — import scenic modules, `play alias.SceneName` |
| 20 | `20_feature_reel.amx` | **Capstone** — compact showcase combining layout, animation, paths, always |
| 21 | `21_actions.amx` | **Actions** — move, draw-in, wipe-in, reveal-in, draw-out, wipe-out, reveal-out |
| 22 | `22_expressions.amx` | **Expressions** — index access, method calls, rgb/rgba, lerp, clamp, rand |
| 23 | `23_plot_kinds.amx` | **Plot kinds** — polar, parametric, implicit PlotCurve in a standalone demo |
| 26 | `26_data_math.amx` | **BarChart & Equation** — standalone BarChart, BarChart inside Graph, Equation with Fragment highlight/unhighlight animations |
| 27 | `27_layout_text.amx` | **Layout & Text** — percentage sizing, min/max constraints, typography (font_weight, letter_spacing, line_height), text wrapping and overflow |

## Running Examples

```bash
# Render a still frame
cargo run --bin animatix -- image examples/00_hello.amx

# Export GIF
cargo run --bin animatix -- gif examples/20_feature_reel.amx -o reel.gif --fps 15

# View AST
cargo run --bin animatix -- ast examples/09_components.amx --compact

# Open GUI (opens 20_feature_reel.amx by default)
cargo run --bin animatix-gui
```

## Design Principles

1. **Progressive complexity** — each example introduces at most two new concepts
2. **Visual interest** — every demo is animated, not static
3. **Self-contained** — no hidden dependencies; module imports point to sibling `lib/` files
4. **Consistent palette** — all use `editorial-dark` colorscheme for visual coherence
5. **Concise** — most under 40 lines; none exceed 80

## Library Modules

| File | Purpose |
|------|---------|
| `lib/palette.amx` | `pub let` color exports for the modules demo |
| `lib/tokens.amx` | Design token constants for modules demo |
| `lib/reexport.amx` | Re-export chain example (used by `10_modules.amx`) |
| `lib/card.amx` | Reusable `Card` component with `highlight` action |
| `lib/slide.amx` | Reusable `Slide` layout with `@slot` markers |
| `lib/components.amx` | Additional reusable components for slot demos |
| `lib/actions.amx` | Reusable custom action blocks |
| `lib/colorschemes.amx` | Custom `Colorscheme` definitions with `extends` |

## Assets

| File | Used By |
|------|---------|
| `assets/checker.png` | Image primitive (`01_shapes.amx`), grid background (`08_effects.amx`) |
| `assets/animatix-mark.svg` | Overlay branding in `08_effects.amx` |
| `assets/pulse.wav` | Embedded audio track in `17_audio_reactive.amx` |