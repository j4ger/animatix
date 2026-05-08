# Animatix Examples

Runnable `.amx` demos. The GUI opens `showcase.amx` by default.

## Files

| Example | Lines | Demonstrates |
|---|---|---|
| `showcase.amx` | ~80 | Hero scene: layout, animation, primitives, composition, expressions |
| `primitives.amx` | ~35 | All primitives: shapes, text, math, code, image, svg |
| `layout.amx` | ~30 | Grid, Col, Row, Stack, anchors, offsets, percentages, nesting |
| `animation.amx` | ~35 | Fades, shift, rotate, scale, morph, sequence, stagger, timing |
| `plotting.amx` | ~25 | Cartesian, polar, parametric, implicit curves in Graph |
| `expressions.amx` | ~25 | let bindings, tuple math, closures, conditionals, paths, rand |
| `colorschemes.amx` | ~20 | Colorscheme selection, auto colors, explicit overrides |
| `modules.amx` | ~25 | `import ... as`, `pub let`, namespaced access |
| `reactive.amx` | ~20 | `always` blocks, time-driven behavior, `if/else` |
| `font_demo.amx` | ~55 | `font_family` selection, runtime text recompilation, dynamic `font_size` |

## Helper Modules

- `modules/palette.amx` — `pub let` exports for the modules demo
- `modules/card.amx` — `pub component` definition for the modules demo

## Assets

- `checker.ppm` — raster image for Image/Svg demos
- `vector.svg` — vector graphic for Svg demo

## Running

```bash
# Render a still frame
cargo run --bin animatix -- image examples/showcase.amx

# View AST
cargo run --bin animatix -- ast examples/showcase.amx --compact

# Open in GUI
cargo run --bin animatix-gui
```
