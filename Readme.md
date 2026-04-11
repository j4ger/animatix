# Animatix

Animatix is a declarative animation language and rendering engine, built in Rust, that enables solo creators to quickly compose explanatory mathematical and vector animations.

## Core Capabilities

Animatix is powered by **Vello** and currently ships a working vector-first runtime for:
- Standard Text and Math (via Typst)
- SVG loading and rendering (via `usvg`)
- Shape actors backed by the runtime today: `Circle` and `Rect`
- Graph containers with `CartesianPlot` and `PolarPlot`
- `Row`, `Col`, and `Group` scene containers (`Row`/`Col` auto-layout; `Group` transform grouping)
- Reactive evaluation primitives: `always`, `loop`, `for`, `yield`, `stop`/`pause`/`resume`

## Demo Showcase

```animatix
// examples/showcase.amx
#0s
t: Text { text: "Animatix Engine", font_size: 100, color: (1.0, 1.0, 1.0, 1.0), at: (640, 200) }
m: Math { math: "E = mc^2", font_size: 80, color: (0.5, 0.8, 1.0, 1.0), at: (640, 400) }
v: Svg { url: "examples/vector.svg", scale: 2.0, at: (640, 600) }
c: Circle, radius: 80, color: (1.0, 0.2, 0.5, 1.0), at: (1000, 400)

#1s
t.color = (1.0, 0.5, 0.0, 1.0) [1s, ease: ease-in-out]
t.at = (640, 250) [1s, ease: ease-in-out]
m.color = (1.0, 0.9, 0.2, 1.0) [1s, ease: ease-in-out]
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
cargo run -- video examples/showcase.amx --output showcase.mp4 --fps 30

# Render to PNG image (at a specific timestamp)
cargo run -- image examples/showcase.amx --output debug_showcase.png --time 1.0
```

## Documentation

The `docs/` folder contains detailed technical information:
- [`architecture.md`](docs/architecture.md): Details of the Vello-based Vector-First rendering architecture.
- [`primitives.md`](docs/primitives.md): Current runtime-supported primitives, graph containers, and parser-only/planned items.
- [`morphing_design.md`](docs/morphing_design.md): The planned design for Manim-style vector morphing between `kurbo::BezPath` instances.
- [`implementation_plan.md`](docs/implementation_plan.md): Detailed implementation status and roadmap for Reactive System, Math/Graph, Containers, and Components.

## Roadmap

The `docs/implementation_plan.md` file tracks what's left to build.

### What's Left
- [ ] Component runtime (instantiation, parameter passing, lifecycle hooks, custom actions)
- [ ] Grid/Stack layout algorithms
- [ ] Additional runtime primitives (`Line`, `Path`, `Polygon`, `Arc`, `Ellipse`, `Image`, `Code`)
- [ ] DSL-level morph strategy controls (`strategy`, `path_arc`, `stretch`)
- [ ] Parametric and implicit plotting primitives
- [ ] Advanced Path Effects (Trimming, dashing, etc.)
- [ ] Interactive UI for building animations
