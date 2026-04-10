# Animatix

Animatix is a declarative animation language and rendering engine, built in Rust, that enables solo creators to quickly compose explanatory mathematical and vector animations.

## Core Capabilities

Animatix is powered by **Vello**—a GPU compute-centric 2D renderer. It executes a "Vector-First" pipeline capable of infinite scaling and perfectly smooth edges, supporting:
- Mathematical Text (via Typst)
- Standard Text (via Typst)
- Basic Shapes (Circles, Rectangles, Lines, Polygons)
- SVG loading and rendering (via `usvg`)

## Demo Showcase

```animatix
// example/showcase.amx
#0s
t: Text { text: "Animatix Engine", font_size: 100, color: (1.0, 1.0, 1.0, 1.0), at: (640, 200) }
m: Math { math: "E = mc^2", font_size: 80, color: (0.5, 0.8, 1.0, 1.0), at: (640, 400) }
v: Svg { url: "example/vector.svg", scale: 2.0, at: (640, 600) }
c: Circle, radius: 80, color: (1.0, 0.2, 0.5, 1.0), at: (1000, 400)

#1s
t.color = (1.0, 0.5, 0.0, 1.0) [1s, ease: ease-in-out]
t.at = (640, 250) [1s, ease: ease-in-out]
m.font_size = 120 [1s, ease: ease-in-out]
c.radius = 120 [1s, ease: ease-in-out]
c.color = (0.2, 1.0, 0.5, 1.0) [1s, ease: ease-in-out]

#2s
scene.background_color = (0.1, 0.1, 0.15, 1.0) [1s, ease: ease-in-out]
v.at = (640, 650) [1s, ease: ease-in-out]
```

## Quick Start

You can run Animatix from the project root to generate videos or static images.

```bash
# Render to MP4 video (30 FPS)
cargo run -- video example/showcase.amx --output showcase.mp4 --fps 30

# Render to PNG image (at a specific timestamp)
cargo run -- image example/showcase.amx --output debug_showcase.png --time 1.0
```

## Documentation

The `docs/` folder contains detailed technical information:
- [`architecture.md`](docs/architecture.md): Details of the Vello-based Vector-First rendering architecture.
- [`primitives.md`](docs/primitives.md): List of supported primitives (Shapes, Text, Math, Svg) and their properties.
- [`morphing_design.md`](docs/morphing_design.md): The planned design for Manim-style vector morphing between `kurbo::BezPath` instances.
- [`implementation_plan.md`](docs/implementation_plan.md): Detailed implementation status and roadmap for Reactive System, Math/Graph, Containers, and Components.

## Architecture & Roadmap

We have transitioned `animatix` to a Vector-First pipeline powered by **Vello**. This bypasses raster/texture-atlas limitations and extracts exact mathematical Bézier curves (`kurbo::BezPath`) for Text, Math, and SVGs.

### Current Features
- [x] `.amx` AST parser and compiler
- [x] Basic animation timeline with easing functions
- [x] Mathematical formula parsing (Typst)
- [x] Precise Vector Text rendering (Typst to BezPaths)
- [x] CLI tool for `.png` and `.mp4` video rendering (via `rsmpeg`)
- [x] Compute-shader vector pipeline (Vello Integration)
- [x] Native SVG loading and rendering (`usvg`)
- [x] Timeline architecture redesigned to support vector-first unified properties
- [x] Manim-style Vector Morphing (Path Interpolation) for Text, Math, and Geometric Shapes (via `kurbo`)
- [x] Hierarchical scene graph with nested containers (Row, Col, Group)
- [x] Transform and opacity inheritance via recursive DFS evaluation
- [x] Auto-UID generation for anonymous container children
- [x] Row/Col container layout algorithm

### Planned
- [ ] Reactive System (`always`, `loop`, `if`) - requires per-frame evaluation model
- [ ] 2D Graph/Plot system - depends on reactive system for live data
- [ ] Component system with parameter passing and lifecycle hooks
- [ ] Grid/Stack layout algorithms
- [ ] Advanced Path Effects (Trimming, dashing, etc.)
- [ ] Math functions (sin, cos, format) in expressions
- [ ] Interactive UI for building animations
