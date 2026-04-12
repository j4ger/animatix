# Animatix Examples

Runnable demos live in `examples/`. Future-facing sketches live in `examples/planned/`.

Each runnable file should explain itself through its filename, header comment, and on-screen copy.

The shipped GUI MVP opens these `.amx` files directly from `crates/animatix-gui` for editing, scrubbing, and preview.

## Runnable

- `showcase.amx`
- `layout_demo.amx`
- `plotting_demo.amx`
- `math_demo.amx`
- `line_and_ellipse_demo.amx`
- `arc_polygon_path_demo.amx`
- `image_demo.amx`
- `image_animation_demo.amx`
- `text_morph_demo.amx`
- `shape_morph_demo.amx`
- `component_modules_demo.amx`
- `reactive_runtime.amx`

`component_modules_demo.amx` is the focused example for imported `pub component` instantiation, rhs property querying, and multi-segment dotted assignment targets against nested labels such as `left.badge.color = red` and `echo.radius = right.badge.radius`.

`reactive_runtime.amx` is the focused example for the shipped stateless reactive model: `always` re-evaluates from the requested time, while repeated runtime behavior is expressed through explicit time math rather than coroutine state.

## Planned

Files under `examples/planned/` document intended future surface area and are not expected to run on the current runtime.
