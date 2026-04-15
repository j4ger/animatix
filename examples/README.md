# Animatix Examples

Runnable demos live in `examples/`.

Each runnable file should explain itself through its filename, header comment, and on-screen copy.

The shipped GUI opens these `.amx` files directly from `crates/animatix-gui` for editing, preview, visual timeline scrubbing, and transport-bar-driven playback checks.

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
- `reveal_actions_demo.amx`
- `motion_shift_demo.amx`
- `modifier_timing_demo.amx`
- `code_demo.amx`
- `component_modules_demo.amx`
- `reactive_runtime.amx`

`component_modules_demo.amx` is the focused example for imported `pub component` instantiation, rhs property querying, and multi-segment dotted assignment targets against nested labels such as `left.badge.color = red` and `echo.radius = right.badge.radius`.

`reactive_runtime.amx` is the focused example for the shipped stateless reactive model: `always` re-evaluates from the requested time, while repeated runtime behavior is expressed through explicit time math rather than coroutine state.

`code_demo.amx` is the focused example for the shipped v1 `Code` primitive: code content rendered through the existing text-path pipeline with animated position, color, and re-declaration updates.

`reveal_actions_demo.amx` is the focused example for the current reveal-action surface: opacity-based `fade-in` plus vector-first `draw-in`, `wipe-in`, and `wipe-out`.

`motion_shift_demo.amx` is the focused example for the current motion-ergonomics slice: `shift` adds local motion on top of existing placement for both manual and layout-managed nodes.

`modifier_timing_demo.amx` is the focused example for the shipped shared timing vocabulary: duration shorthand, named `delay`, named `ease`, explicit instant changes, and delayed-first-declaration behavior.

`shape_morph_demo.amx` is the focused example for the shipped scoped morph modifier subset on path-morphing re-declarations: `strategy: auto|match`, `path_arc`, and `stretch`, while `strategy: fade` remains deferred.
