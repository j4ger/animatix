# Animatix Examples

This directory is split into two groups:

- **Current demos**: intended to run against the current runtime
- **Planned demos**: syntax sketches for future work, not expected to run yet

## Current Demos

- `showcase.amx` — broad overview of the current runtime surface
- `layout_demo.amx` — `Row`, `Col`, and `Group`
- `plotting_demo.amx` — `Graph`, `CartesianPlot`, `PolarPlot`
- `math_demo.amx` — expression evaluation and math helpers in scene properties
- `text_morph_demo.amx` — text path morphing
- `shape_morph_demo.amx` — circle-to-rect morphing

## Planned Demos

Files under `examples/planned/` are intentionally future-facing. They document the direction of the language, but they rely on features that are not implemented yet.

That planned folder also contains a reactive-runtime sketch. The engine has reactive evaluation machinery, but the polished public demo surface for that feature still needs work.
