# Animatix

Animatix is a declarative animation language and rendering engine, built in Rust, that enables solo creators to quickly compose explanatory mathematical and vector animations.

## Core Capabilities

Animatix is powered by **Vello** and currently ships a working vector-first runtime for:
- Standard Text and Math (via Typst)
- SVG loading and rendering (via `usvg`)
- Shape actors backed by the runtime today: `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Graph containers with `CartesianPlot` and `PolarPlot`
- `Row`, `Col`, `Grid`, `Stack`, and `Group` scene containers
- Layout-first composition primitives: root layout defaults, scene anchors, percentage placement, and manual child overrides
- Reactive evaluation primitives: stateless `always` and compile-time `for`
- Imported `pub component` runtime instantiation with parameter binding and isolated nested labels
- Multi-segment dotted assignment targets for nested labels such as `left.badge.color = red`
- Sampled rhs path lookup for actor and scene properties such as `copy.at = left.badge.at` and `echo.radius = right.badge.radius`

## Demo Showcase

```animatix
// examples/showcase.amx
// Runnable example: broad current-runtime overview with text, math, SVG, and basic shape animation.
#0s
scene.background_color = (0.07, 0.08, 0.12, 1.0)

title: Text { text: "Animatix", font_size: 92, color: (1.0, 1.0, 1.0, 1.0), at: (640, 150) }
formula: Math { math: "E = mc^2", font_size: 80, color: (0.50, 0.80, 1.0, 1.0), at: (340, 370) }
logo: Svg { url: "examples/vector.svg", scale: 2.0, at: (940, 360) }
orb: Circle, radius: 82, color: (1.0, 0.25, 0.55, 1.0), at: (250, 560)
panel: Rect, size: (240, 120), color: (0.22, 0.85, 0.55, 1.0), at: (980, 580)

#1.5s
title.at = (640, 190) [1s, ease: ease-in-out]
title.color = (1.0, 0.75, 0.25, 1.0) [1s, ease: ease-in-out]
orb.radius = 120 [1s, ease: ease-in-out]
orb.color = (0.25, 1.0, 0.65, 1.0) [1s, ease: ease-in-out]
panel.size = (320, 160) [1s, ease: ease-in-out]

#3s
scene.background_color = (0.10, 0.10, 0.16, 1.0) [1s, ease: ease-in-out]
logo.at = (900, 330) [1s, ease: ease-in-out]
panel.at = (980, 540) [1s, ease: ease-in-out]
```

## Quick Start

You can run Animatix from the project root to inspect scene structure, preview scenes, or render frames and videos.

```bash
# Print the parsed AST for a scene
cargo run -- ast examples/showcase.amx

# Print AST on one line for quick inspection or diffing
cargo run -- ast examples/showcase.amx --compact

# Open the scene in the live renderer
cargo run -- render examples/showcase.amx

# Render to MP4 video (30 FPS)
cargo run -- video examples/showcase.amx --output showcase.mp4 --fps 30

# Render to PNG image (at a specific timestamp)
cargo run -- image examples/showcase.amx --output debug_showcase.png --time 1.0
```

## Internal Debugging and Validation Utilities

Animatix currently exposes these contributor-facing utilities through the CLI:

- `ast <file>`: inspect the parsed module-expanded AST before runtime evaluation
- `render <file>`: open a live preview window for quick visual checks
- `image <file> --time <seconds>`: export a specific frame to PNG for timeline debugging
- `video <file>`: export a full MP4 render for end-to-end validation

Useful flags:

- `ast --compact`: print AST output on one line
- `ast --force`: currently present as a CLI/debug flag, but not meaningfully wired beyond argument parsing yet
- `image --time`: inspect the scene at an exact point on the timeline
- `image/video --width/--height`: validate layout and rendering at fixed resolutions

### About keyframes and exports

The engine has an internal keyframed timeline system, but there is **not** currently a dedicated public CLI command for exporting compiled keyframes or timeline tracks. Today, the practical debugging workflow is to inspect the AST with `ast` and validate timeline behavior with `image --time ...` or `video` renders.

## Examples

The curated runnable demos live in `examples/`:
- `showcase.amx`
- `layout_demo.amx`
- `plotting_demo.amx`
- `math_demo.amx`
- `line_and_ellipse_demo.amx`
- `arc_polygon_path_demo.amx`
- `text_morph_demo.amx`
- `shape_morph_demo.amx`
- `component_modules_demo.amx`
- `reactive_runtime.amx`

## Documentation

The `docs/` folder contains detailed technical information:
- [`architecture.md`](docs/architecture.md): Details of the Vello-based Vector-First rendering architecture.
- [`layout_design.md`](docs/layout_design.md): Concrete layout-model design, precedence rules, and Phase 1 implementation plan.
- [`development.md`](docs/development.md): Internal debugging utilities, validation workflow, and contributor-oriented development notes.
- [`primitives.md`](docs/primitives.md): Current runtime-supported primitives, graph containers, and parser-only/planned items.
- [`morphing_design.md`](docs/morphing_design.md): The planned design for Manim-style vector morphing between `kurbo::BezPath` instances.
- [`stateless_reactive_design.md`](docs/stateless_reactive_design.md): Implemented stateless reactive model and migration rationale.
- [`implementation_plan.md`](docs/implementation_plan.md): Detailed implementation status and roadmap for Reactive System, Math/Graph, Containers, and Components.
- [`gui_architecture.md`](docs/gui_architecture.md): Current GUI shell architecture, preview delivery model, and transport design.
- [`gui_implementation_plan.md`](docs/gui_implementation_plan.md): GUI status tracker covering the shipped editor/preview shell and current follow-up work.
- [`examples/README.md`](examples/README.md): Guide to the curated runnable demos.
- [`../CONTRIBUTING.md`](CONTRIBUTING.md): Contribution workflow, design/doc sync expectations, code quality rules, and validation standards.

Current language-plumbing note: `crates/animatix/src/parser.rs` is the executable source of truth for accepted `.amx` syntax. A shared Tree-sitter grammar package now lives in [`tree-sitter-animatix/`](tree-sitter-animatix/), and that grammar is intended to stay synchronized with the parser and `docs/spec.md` rather than define a separate language surface.

## Roadmap

The `docs/implementation_plan.md` file tracks what's left to build.

### What's Left
- [ ] Expand component runtime beyond imported `pub component` instantiation (custom actions, richer scoping)
- [x] Additional runtime primitives (`Code`)
- [ ] DSL-level morph strategy controls (`strategy`, `path_arc`, `stretch`)
- [ ] Parametric and implicit plotting primitives
- [ ] Advanced Path Effects (Trimming, dashing, etc.)
- [~] Interactive UI for building animations (editor-first GUI shell shipped; deeper visual authoring still remains)
