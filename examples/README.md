# Animatix Examples

A curated suite of runnable `.amx` demos. The numbered files remain a
progressive learning track, but are grouped by the language area they exercise.
Standalone real-content showcases live under `projects/`.

In-progress dogfooding scenes and grammar probes are intentionally kept out of
this directory; see `../dogfood/README.md`.

## Running Examples

From the workspace root:

```bash
# Check / parse an example
cargo run --bin animatix -- check examples/basics/00_hello.amx

# Render a still frame
cargo run --bin animatix -- image examples/basics/00_hello.amx

# Export GIF
cargo run --bin animatix -- gif examples/gallery/brand_reel/main.amx -o reel.gif --fps 15

# View AST
cargo run --bin animatix -- ast examples/components/09_components.amx --compact
```

## Gallery

Real-content capstones demonstrating the full language. Start here.

| File | What it shows |
|------|---------------|
| `gallery/dashboard_story.amx` | Three-scene data story: Grid/% sizing, count-up text, `swap`/`reorder`, morph transitions |
| `gallery/motion_poster.amx` | Single-poster motion piece: stagger choreography, path morph, Filter sweeps |
| `gallery/epicycles.amx` | Fourier story: Graph-hosted PlotCurves, `stroke_progress` reveals, always-driven pen |
| `gallery/sorting_theatre.amx` | Insertion-sort theatre: `for` loops, build-time `if`, `[step:]` clocks, `list_swap` |
| `gallery/fft_explain.amx` | Explainer: Typst equation fragments, colored plots, per-fragment reveals |
| `gallery/brand_reel/` | Multi-file capstone: all six `play` transitions, `persist`/`remove` mascot chain, Audio bed, cross-file scenes (`import as` + `play alias.Scene`) |

Shared library the gallery builds on lives in [`lib/`](lib/) (`theme.amx`
tokens + colorscheme, `motion.amx` motion vocabulary, `TitleCard`).

## Basics

| File | Description |
|------|-------------|
| `basics/00_hello.amx` | Minimal scene: title card with staggered fade-in |
| `basics/01_shapes.amx` | Primitive shapes, image, SVG, Typst, Text, Code |
| `basics/03_timing.amx` | Timing: sequence, stagger, easing curves |
| `basics/22_expressions.amx` | Expressions: index access, methods, lerp, clamp, rand |

## Layout

| File | Description |
|------|-------------|
| `layout/02_layout.amx` | Containers: Row, Col, Grid, Stack |
| `layout/11_colors.amx` | Color system, semantic tokens, `auto`, Colorscheme |
| `layout/12_reorder.amx` | Reorder: `swap`, `reorder`, dynamic layout |
| `layout/13_paths.amx` | Paths: bezier curves, morphing, affine transforms |
| `layout/27_layout_text.amx` | Percentage sizing, constraints, typography, wrapping |

## Animation

| File | Description |
|------|-------------|
| `animation/04_motion.amx` | Actions and reactive `always` expressions |
| `animation/05_morph.amx` | Re-declaration morphing |
| `animation/06_reactive.amx` | Reactive expressions and `_animating_*` flags |
| `animation/08_effects.amx` | Filter, shake, pulse, bounce |
| `animation/16_showcase.amx` | Combined layout, morphing, paths, transforms, always |
| `animation/21_actions.amx` | Entrance, motion, exit, and effect actions |

## Components

| File | Description |
|------|-------------|
| `components/09_components.amx` | `pub component`, slots, custom actions, typed params |
| `components/10_modules.amx` | Imports, namespaces, `pub let`, re-export chains |
| `components/29_strict_types.amx` | Strict type annotations and subtyping |

## Data

| File | Description |
|------|-------------|
| `data/07_plots.amx` | Graph, PlotCurve, VectorField, Heatmap |
| `data/18_number_plane_contours.amx` | NumberPlane, ContourSet, VectorField |
| `data/23_plot_kinds.amx` | Polar, parametric, and implicit curves |
| `data/24_plot_transitions.amx` | Animated transitions between plot kinds |
| `data/26_data_math.amx` | BarChart and Equation with Fragment animations |

## Composition

| File | Description |
|------|-------------|
| `composition/14_multiscene.amx` | Scene declarations and transitions |
| `composition/19_cross_file_scenes.amx` | Imported scene modules and `play alias.Scene` |
| `composition/20_feature_reel.amx` | Compact capstone showcase |
| `composition/25_persistence.amx` | `persist` / `remove` across scene transitions |

## Generation

| File | Description |
|------|-------------|
| `generation/15_for_loop.amx` | Procedural per-frame wave with `for` |
| `generation/17_audio_reactive.amx` | Audio-reactive waveform and embedded track |
| `generation/28_generation_reactive.amx` | Group, Mask, array actors, advanced `always` |

## Projects

These are real-content showcases, not feature demos. They are useful
dogfooding material because they stress combinations of features and expose
language design gaps.

| File | Description |
|------|-------------|
| `projects/callout_example.amx` | Callout primitive with annotations |
| `projects/legend_example.amx` | Legend primitive |
| `projects/fft_explain.amx` | FFT walkthrough across five scenes |
| `projects/gradient_descent.amx` | Gradient descent explainer |
| `projects/leetcode_climbing_stairs.amx` | Algorithm animation with probes |
| `projects/leetcode_reverse_linked_list.amx` | Linked-list reversal with probes |
| `projects/leetcode_sort_colors.amx` | Dutch national flag as a timeline function (`fn dnf_pass` + `list_swap` + `[step: ...]` loops) |
| `projects/plugin_pulse.amx` | Native plugin showcase: custom primitive, enum property, action, function, cached image |

`projects/plugin_pulse.amx` requires the native demo plugin and its manifest:

```bash
cargo build -p animatix-plugin-demo
cargo run --bin animatix -- check examples/projects/plugin_pulse.amx \
  --plugin crates/animatix-plugin-demo/demo.amx-plugin.toml
```

## Library Modules (`lib/`)

Reusable modules imported by examples and available for dogfood projects.

| File | Purpose |
|------|---------|
| `lib/palette.amx` | `pub let` color exports |
| `lib/tokens.amx` | Design token constants |
| `lib/reexport.amx` | Re-export chain example |
| `lib/card.amx` | Reusable `Card` component |
| `lib/slide.amx` | Reusable `Slide` layout with slots |
| `lib/components.amx` | Reusable components for future demos |
| `lib/actions.amx` | Reusable custom action blocks |
| `lib/colorschemes.amx` | Custom `Colorscheme` definitions |

## Assets (`assets/`)

| File | Used By |
|------|---------|
| `assets/checker.png` | Image primitive and grid backgrounds |
| `assets/animatix-mark.svg` | SVG branding overlays |
| `assets/pulse.wav` | Embedded audio track |

## Scene Modules (`scenes/`)

| File | Purpose |
|------|---------|
| `scenes/reel_intro.amx` | Imported scene fragment used by `composition/19_cross_file_scenes.amx` |
