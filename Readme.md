# Animatix

Animatix is a declarative animation language and Rust rendering engine for building explanatory math, diagram, and vector animations.

It currently ships a runnable CLI renderer, a growing `.amx` language surface, an egui-based desktop GUI shell, and a Tree-sitter grammar for editor tooling.

The GUI shell now includes basic transport shortcuts (`Space` to play/pause, `←` / `→` to scrub) and an Explorer-side action registry panel sourced from the runtime's built-in action signatures.

## Quick Example

```animatix
// examples/showcase.amx
#0s
scene.background_color = (0.07, 0.08, 0.12, 1.0)

title: Text { text: "Animatix", font_size: 92, color: (1.0, 1.0, 1.0, 1.0), anchor: scene.top, offset: (0, 92) }
formula: Math { math: "E = mc^2", font_size: 80, color: (0.50, 0.80, 1.0, 1.0), at: (30%, 34%) }
logo: Svg { url: "examples/vector.svg", scale: 2.0, at: (72%, 34%) }
stage: Row, anchor: scene.bottom, offset: (0, -120), gap: 180, align: "center" {
  orb: Circle, radius: 82, color: (1.0, 0.25, 0.55, 1.0),
  signal: Line, from: (-80, 0), to: (80, 0), stroke: (0.50, 0.80, 1.0, 1.0), stroke_width: 6,
  panel: Rect, size: (250, 130), color: (0.25, 1.0, 0.65, 1.0)
}

#1.5s
orb.radius = 120 [1s, ease: ease-in-out]
orb.color = (0.25, 1.0, 0.65, 1.0) [1s, ease: ease-in-out]
```

## Quick Start

From the repo root:

```bash
cargo build
cargo run --bin animatix -- render examples/showcase.amx
cargo run --bin animatix -- render examples/showcase.amx --loop
cargo run --bin animatix -- image examples/showcase.amx --time 1.0 --output showcase.png
cargo run --bin animatix -- video examples/showcase.amx --fps 30 --duration 5 --output showcase.mp4
cargo run --bin animatix -- gif examples/showcase.amx --fps 15 --duration 5 --output showcase.gif
cargo run --bin animatix-gui -- examples/showcase.amx
```

`render` opens a live preview window. Use `render --loop` to replay the authored timeline instead of holding on the last frame. `image` writes a PNG for one timestamp, `video` exports an MP4, and `gif` exports an animated GIF.

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

# Render an animated GIF (great for web sharing)
cargo run --bin animatix -- gif examples/showcase.amx --fps 15 --duration 5 --output showcase.gif
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
- `motion_rotate_demo.amx` — current local rotation action surface (`rotate`)
- `motion_scale_demo.amx` — current local visual scale action surface (`scale`)
- `composition_sequence_demo.amx` — current ordered composition surface (`sequence`)
- `composition_stagger_demo.amx` — current staggered composition surface (`stagger`)
- `primitive_breadth_demo.amx` — current primitive breadth slice (`Dot`, `Square`, `RegularPolygon`)
- `arrow_demo.amx` — current arrow primitive slice (`Arrow`)
- `parametric_plot_demo.amx` — current parametric plotting slice (`ParametricPlot`)
- `implicit_plot_demo.amx` — current implicit plotting slice (`ImplicitPlot`)
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
- [`docs/spec.md`](docs/spec.md) — language specification and status matrix
- [`docs/architecture.md`](docs/architecture.md) — system architecture and design
- [`docs/primitive_architecture.md`](docs/primitive_architecture.md) — unified primitive system design
- [`docs/contributing.md`](docs/contributing.md) — development workflows and project structure
- [`docs/roadmap.md`](docs/roadmap.md) — known gaps and planned features
- [`examples/colorscheme_demo.amx`](examples/colorscheme_demo.amx) — built-in colorscheme example
- [`tree-sitter-animatix/README.md`](tree-sitter-animatix/README.md) — editor grammar scope and sync rules

## Roadmap

Current work is focused on keeping the documented language surface honest, expanding the runtime where it already has a clear contract, and improving the GUI/editor experience without overpromising unfinished syntax.

See [`docs/spec.md`](docs/spec.md) for the language status matrix and [`docs/roadmap.md`](docs/roadmap.md) for known gaps, planned features, and work items.
