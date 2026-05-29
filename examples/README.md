# Animatix Examples

A progressive suite of 17 runnable `.amx` demos. Each builds on the previous, introducing one or two language features in isolation before the final showcase combines everything.

## Quick Reference

| # | File | Duration | What It Shows |
|---|------|----------|---------------|
| 00 | `00_hello.amx` | ~1.5s | **Minimal scene** — one actor, one fade-in |
| 01 | `01_shapes.amx` | ~1.5s | **All primitives** — Rect, Ellipse, Polygon, Line, Path, Math, Code |
| 02 | `02_layout.amx` | ~6s | **Containers** — Row, Col, Grid, Stack with animated gaps |
| 03 | `03_timing.amx` | ~5s | **Keyframes** — absolute `#0s`, relative `#+0.3s`, easing curves |
| 04 | `04_motion.amx` | ~12s | **Actions** — shift, rotate, scale, fade, draw, wipe |
| 05 | `05_morph.amx` | ~7s | **Morphing** — re-declaration, `strategy`, `path_arc` |
| 06 | `06_reactive.amx` | ∞ | **Reactive** — `always`, `t`, scene anchors, `if/else` |
| 07 | `07_plots.amx` | ~4s | **Math plots** — cartesian, polar, parametric, implicit |
| 08 | `08_effects.amx` | ~9s | **Effects** — shake, pulse, bounce |
| 09 | `09_components.amx` | ~5s | **Components** — `pub component`, slots, custom actions |
| 10 | `10_modules.amx` | ~4s | **Modules** — `import ... as`, `pub let`, dotted access |
| 11 | `11_colors.amx` | ~3s | **Color system** — `auto`, built-in constants, scheme tokens |
| 12 | `12_reorder.amx` | ~10s | **Reorder** — `swap`, `reorder` with `dynamic_layout` |
| 13 | `13_paths.amx` | ~6s | **Paths** — bezier curves, polygon morphing |
| 14 | `14_multiscene.amx` | ~6s | **Scenes** — `# SceneName`, `play`, transitions |
| 15 | `15_for_loop.amx` | ~4s | **For loops** — compile-time expansion |
| 16 | `16_showcase.amx` | ~6s | **Hero** — everything combined in one polished scene |

## Running Examples

```bash
# Render a still frame
cargo run --bin animatix -- image examples/00_hello.amx

# Export GIF
cargo run --bin animatix -- gif examples/16_showcase.amx -o showcase.gif --fps 15

# View AST
cargo run --bin animatix -- ast examples/09_components.amx --compact

# Open GUI (opens 16_showcase.amx by default)
cargo run --bin animatix-gui
```

## Design Principles

1. **Progressive complexity** — each example introduces at most two new concepts
2. **Visual interest** — every demo is animated, not static
3. **Self-contained** — no hidden dependencies; each file parses and renders standalone
4. **Consistent palette** — all use `editorial-dark` colorscheme for visual coherence
5. **Concise** — most under 40 lines; none exceed 80

## Library Modules

| File | Purpose |
|------|---------|
| `lib/palette.amx` | `pub let` color exports for the modules demo |
| `lib/card.amx` | Reusable `Card` component with `highlight` action |
| `lib/slide.amx` | Reusable `Slide` layout with `@slot` markers |

## Assets

| File | Used By |
|------|---------|
| `assets/checker.png` | Image primitive demos |
| `assets/vector.svg` | Svg primitive demos |
