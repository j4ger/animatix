# Animatix

Animatix is a declarative animation language and Rust rendering engine for building explanatory math, diagram, and vector animations.

It currently ships a runnable CLI renderer, a growing `.amx` language surface, an egui-based desktop GUI shell, and a Tree-sitter grammar for editor tooling.

## Quick Example

```animatix
// examples/showcase.amx
#0s
scene.background_color = (0.07, 0.08, 0.12, 1.0)

title: Text { text: "Animatix", font_size: 92, color: (1.0, 1.0, 1.0, 1.0), at: (640, 150) }
formula: Math { math: "E = mc^2", font_size: 80, color: (0.50, 0.80, 1.0, 1.0), at: (340, 370) }
logo: Svg { url: "examples/vector.svg", scale: 2.0, at: (940, 360) }
orb: Circle, radius: 82, color: (1.0, 0.25, 0.55, 1.0), at: (250, 560)

#1.5s
title.at = (640, 190) [1s, ease: ease-in-out]
orb.radius = 120 [1s, ease: ease-in-out]
orb.color = (0.25, 1.0, 0.65, 1.0) [1s, ease: ease-in-out]
```

## Quick Start

From the repo root:

```bash
cargo build
cargo run --bin animatix -- render examples/showcase.amx
cargo run --bin animatix -- image examples/showcase.amx --time 1.0 --output showcase.png
cargo run --bin animatix -- video examples/showcase.amx --fps 30 --duration 5 --output showcase.mp4
cargo run --bin animatix-gui -- examples/showcase.amx
```

`render` opens a live preview window. `image` writes a PNG for one timestamp, and `video` exports an MP4.

Useful CLI commands:

```bash
# Inspect the parsed AST
cargo run --bin animatix -- ast examples/showcase.amx

# Compact AST output for quick diffing
cargo run --bin animatix -- ast examples/showcase.amx --compact

# Render a frame at a specific time
cargo run --bin animatix -- image examples/showcase.amx --time 1.5 --output frame.png

# Render a video at a chosen resolution
cargo run --bin animatix -- video examples/showcase.amx --width 1920 --height 1080 --fps 30 --duration 5 --output demo.mp4
```

If you use Nix, `nix develop` sets up the Rust, FFmpeg, Tree-sitter, and graphics dependencies used by the repo.

## What's Shipped Today

- Scene primitives: `Text`, `Math`, `Code`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Plotting: `Graph`, `CartesianPlot`, and `PolarPlot`
- Containers: `Row`, `Col`, `Grid`, `Stack`, and `Group`
- Reactive authoring: stateless `always` and compile-time `for`
- Components: imported `pub component` instantiation, parameter binding, dotted assignment targets, and rhs property lookup
- Tooling: CLI renderer, egui-based GUI shell in `crates/animatix-gui`, and `tree-sitter-animatix` for editor integration

For the exact implemented language surface, see [`docs/spec.md`](docs/spec.md) and [`docs/primitives.md`](docs/primitives.md).

In particular, square-bracket modifiers use a generic parser surface, but the shipped runtime currently supports only a smaller statement-specific subset. The spec calls out which modifier behaviors are runtime-real, partial, or still planned.

## Examples

Runnable demos live in [`examples/`](examples/):

- `showcase.amx` — broad runtime overview
- `layout_demo.amx` — layout containers and placement behavior
- `plotting_demo.amx` — graphing and plots
- `math_demo.amx` — math rendering
- `line_and_ellipse_demo.amx` — line and ellipse primitives
- `arc_polygon_path_demo.amx` — newer vector primitives
- `image_demo.amx` / `image_animation_demo.amx` — image rendering and animation
- `reveal_actions_demo.amx` — current reveal and exit action surface
- `motion_shift_demo.amx` — current local motion action surface (`move` + `shift`)
- `composition_sequence_demo.amx` — current ordered composition surface (`sequence`)
- `text_morph_demo.amx` / `shape_morph_demo.amx` — current morphing behavior
- `code_demo.amx` — the shipped `Code` primitive
- `component_modules_demo.amx` — imported components and dotted property access
- `reactive_runtime.amx` — the current stateless reactive model

For the full curated list, see [`examples/README.md`](examples/README.md).

## For Developers

Core workflows:

```bash
cargo build
cargo test
```

Grammar/tooling validation:

```bash
cd tree-sitter-animatix
tree-sitter generate
tree-sitter test
tree-sitter highlight ../examples/reactive_runtime.amx
```

Useful docs:

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution flow and validation expectations
- [`docs/spec.md`](docs/spec.md) — current language status matrix
- [`docs/implementation_plan.md`](docs/implementation_plan.md) — shipped vs planned work
- [`tree-sitter-animatix/README.md`](tree-sitter-animatix/README.md) — editor grammar scope and sync rules

## Roadmap

Current work is focused on keeping the documented language surface honest, expanding the runtime where it already has a clear contract, and improving the GUI/editor experience without overpromising unfinished syntax.

For the detailed phased plan, see [`docs/implementation_plan.md`](docs/implementation_plan.md).
