# Animatix Examples

Runnable demos live in `examples/`.

The suite is intentionally curated as a cohesive dark-stage technical showcase: consistent title/subtitle/caption framing, restrained editorial composition, and one focused runtime capability per file.

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
- `motion_rotate_demo.amx`
- `motion_scale_demo.amx`
- `composition_sequence_demo.amx`
- `composition_stagger_demo.amx`
- `primitive_breadth_demo.amx`
- `arrow_demo.amx`
- `parametric_plot_demo.amx`
- `implicit_plot_demo.amx`
- `modifier_timing_demo.amx`
- `code_demo.amx`
- `component_modules_demo.amx`
- `component_diagnostics_demo.amx`
- `reactive_runtime.amx`
- `colorscheme_demo.amx`
- `colorscheme_defaults_demo.amx`
- `colorscheme_ocean.amx`
- `module_reuse_demo.amx`

## Family Notes

- `showcase.amx` is the canonical hero example for the current shipped surface: scene-relative text/media framing plus a layout-managed primitive row on one polished stage.
- `layout_demo.amx` stays didactic and is the focused example for layout ownership: default placement, anchors, offsets, percentages, and one narrow manual child override inside otherwise layout-managed composition.
- `motion_shift_demo.amx`, `motion_rotate_demo.amx`, and `motion_scale_demo.amx` remain the focused motion ergonomics family for move vs shift, rotate, and visual-only scale.
- `composition_sequence_demo.amx` and `composition_stagger_demo.amx` remain the focused composition family for ordered lowering and shared-interval offsets.
- `plotting_demo.amx`, `parametric_plot_demo.amx`, and `implicit_plot_demo.amx` remain the plotting family for graph composition, tuple-return parametric closures, and implicit contour extraction.
- `image_demo.amx`, `image_animation_demo.amx`, `code_demo.amx`, and `text_morph_demo.amx` remain the media/text family for image sizing, animated image properties, code rendering, and text-path morphing.

## Focused Runtime Proofs

`component_modules_demo.amx` is the focused example for imported `pub component` instantiation, rhs property querying, and multi-segment dotted assignment targets against nested labels such as `left.badge.color = red`, `right.badge.radius = 20`, and `echo.radius = right.badge.radius`.

`component_diagnostics_demo.amx` is the focused example for the current component-diagnostics contract: valid dotted access remains live, while invalid nested targets and invalid rhs dotted lookups emit build diagnostics without crashing the stage.

`reactive_runtime.amx` is the focused example for the shipped stateless reactive model: `always` re-evaluates from the requested time, while repeated runtime behavior is expressed through explicit time math rather than coroutine state.

`colorscheme_demo.amx` is the focused example for built-in colorscheme selection with automatic primitive defaults and semantic aliases: `config { colorscheme: "editorial-dark" }` provides defaults for all primitives, `color: auto` cycles through the auto pool, and explicit overrides still win.

`colorscheme_defaults_demo.amx` is the minimal-boilerplate demonstration: primitives receive scheme-appropriate defaults automatically (Text→text.primary, shapes→surface.primary, strokes→stroke.default) with no explicit color properties needed in most cases.

`colorscheme_ocean.amx` is the focused example for inline colorscheme definition with `extends` inheritance: `let ocean = Colorscheme { extends: "default-dark", ... }` followed by `config { colorscheme: "ocean" }`.

`module_reuse_demo.amx` is the focused example for the module system v1: `pub let` exports in one file, `import "..." as name` namespaced imports in another, and qualified access like `theme.accent` in actor property expressions.

`code_demo.amx` is the focused example for the shipped v1 `Code` primitive: code content rendered through the existing text-path pipeline with animated position, color, and re-declaration updates.

`reveal_actions_demo.amx` is the focused example for the current reveal-action surface: opacity-based `fade-in` plus vector-first `draw-in`, `wipe-in`, `wipe-out`, `reveal-out`, and `draw-out`.

`motion_shift_demo.amx` is the focused example for the current motion-ergonomics slice: `move` sets a local offset target while `shift` adds relative local motion on top of existing placement for both manual and layout-managed nodes.

`motion_rotate_demo.amx` is the focused example for the current rotation slice: `rotate` applies relative local turns in radians for both manual and layout-managed nodes.

`motion_scale_demo.amx` is the focused example for the current scale slice: `scale` applies uniform visual growth without rebinding placement or reflowing layout.

`composition_sequence_demo.amx` is the focused example for composition v1a: `sequence { ... }` lowers actions and assignments into ordered timing without introducing playback-state semantics.

`composition_stagger_demo.amx` is the focused example for composition v1b: `stagger [150ms] { ... }` offsets each child statement by a shared interval from the parent keyframe.

`primitive_breadth_demo.amx` is the focused example for the current breadth slice: `Dot`, `Square`, and `RegularPolygon` all ride on the existing vector primitive pipeline.

`arrow_demo.amx` is the focused example for the current arrow slice: `Arrow` reuses line-style local coordinates with a generated vector arrowhead.

`parametric_plot_demo.amx` is the focused example for the current plotting breadth slice: `ParametricPlot` samples tuple-return closures inside the existing `Graph` runtime.

`implicit_plot_demo.amx` is the focused example for the current implicit plotting slice: `ImplicitPlot` extracts the zero contour of a scalar field over the parent graph domain.

`modifier_timing_demo.amx` is the focused example for the shipped shared timing vocabulary: duration shorthand, named `delay`, named `ease`, explicit instant changes, and delayed-first-declaration behavior.

`shape_morph_demo.amx` is the focused example for the shipped scoped morph modifier subset on path-morphing re-declarations: `strategy: auto|match`, `path_arc`, and `stretch`, while `strategy: fade` remains deferred.
