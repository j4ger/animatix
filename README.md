# Animatix

Animatix is a declarative animation language and Rust rendering engine for building explanatory math, diagram, and vector animations. It ships a CLI renderer, an egui-based desktop GUI, a Tree-sitter grammar, and an LSP server.

## Example

```animatix
config { colorscheme: "editorial-dark", resolution: (1280, 720) }

title: Text, text: "ANIMATIX", font_size: 96, color: text.primary, anchor: scene.center
accent: Rect, size: (60, 3), color: accent.primary, anchor: scene.center, offset: (0, -30)

#0.5s
fade-in title [1s, ease: ease-out]
accent.size = (640, 3) [800ms, ease: ease-out]
```

## Quick Start

```bash
cargo build

# Live preview
cargo run --bin animatix -- render examples/basics/00_hello.amx

# Export
cargo run --bin animatix -- image examples/data/07_plots.amx -o frame.png

# Video/GIF/WebM export (requires the `video` feature, see AGENTS.md)
cargo run --features animatix/video --bin animatix -- video examples/gallery/fft_explain.amx -o showcase.mp4 --fps 30
cargo run --features animatix/video --bin animatix -- gif examples/gallery/brand_reel/main.amx -o reel.gif --fps 15

# Named export presets are shared by CLI and GUI and can be set in config:
cargo run --features animatix/video --bin animatix -- video examples/gallery/fft_explain.amx --export-preset 1080p30 -o showcase.mp4

# GUI
cargo run --bin animatix-gui -- examples/gallery/brand_reel/main.amx
```

Nix users: `nix develop` sets up all dependencies.

Feature targets: the default build includes `render`, `text`, and `svg`.
Bare `cargo check -p animatix --no-default-features` is intentionally not
supported; CI validates the supported combination with
`--no-default-features --features render,text,svg`.

## What's Shipped

**Primitives:** `Text`, `Typst`, `Code`, `Svg`, `Image`, `Audio`, `Rect`, `Ellipse`, `Line`, `Arrow`, `Polygon`, `Path`, `Equation`/`Fragment`

**Plotting:** `Graph`, `PlotCurve` (cartesian/polar/parametric/implicit), `BarChart`, `VectorField`, `Heatmap`, `ContourSet`, `NumberPlane`

**Containers:** `Row`, `Col`, `Grid`, `Stack`, `Group`, `Filter`, `Mask`

**Animation:** Keyframes, morphing, 20+ built-in actions (`fade-in`, `draw-in`, `move`, `shift`, `rotate`, `scale`, `swap`, `reorder`, `shake`, `pulse`, `bounce`, …), easing curves, sequence/stagger composition

**Language:** Reactive `always` blocks, compile-time `for`, `pub component` with slots, module imports, gradual type system, colorschemes

**Multi-scene:** Scene declarations, transitions (fade/wipe/cut), cross-file composition, auto-routed CLI export

**Tooling:** CLI renderer (image; video/GIF/WebM behind the `video` feature), GUI shell, Tree-sitter grammar, LSP server

Full language spec: [`docs/spec.md`](docs/spec.md) · All primitives: [`docs/primitives.md`](docs/primitives.md) · All properties: [`docs/properties.md`](docs/properties.md)

## Examples

A curated set of 50 runnable entry demos in [`examples/`](examples/) covering shapes, layout, timing, morphing, reactive expressions, plotting, effects, components, modules, multi-scene, and more. See [`examples/README.md`](examples/README.md).

## Documentation

| Document | Description |
|----------|-------------|
| [`docs/spec.md`](docs/spec.md) | Language specification and status matrix |
| [`docs/architecture.md`](docs/architecture.md) | System architecture and design decisions |
| [`docs/contributing.md`](docs/contributing.md) | Development workflows and project structure |
| [`docs/roadmap.md`](docs/roadmap.md) | Planned features and known gaps |
| [`docs/gui_design_language.md`](docs/gui_design_language.md) | GUI design language and migration plan |
| [`dogfood/README.md`](dogfood/README.md) | In-progress real-content projects and grammar probes |

## For Developers

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for contribution guidelines and [`docs/contributing.md`](docs/contributing.md) for detailed workflows.
