# Typed colors

Use the [color syntax and ranges](../../../../docs/spec.md) for the target engine.

- Fixed custom color: `hex_color("ff2d55")`; alpha last in `hex_color("ff2d5580")`. Exactly 6 or 8 hex digits, case insensitive, no `#`. The argument must be one quoted literal, not a variable or computed string. Expand old shorthand: `"#f55"` → `hex_color("ff5555")`.
- Computed color: `rgb(1, (80 + 20 * sin(t)) / 255, 90 / 255)`, or `rgba(..., 0.5)`. Every channel is 0–1, including alpha. Divide byte RGB values by 255 to map 0–255 to 0–1. In the example, 80 is the green channel midpoint and 20 is its excursion in either direction. Choose both from the intended color range. If the expression can overshoot, use `clamp(x, 0, 1)` explicitly. No automatic byte mode. For a named scheme use tokens like `accent.primary`; use bare `auto` for palette assignment where supported, not `"auto"`.
- Migrate color properties, assignments, typed component arguments, gradient stops and color function inputs. Do not turn unrelated strings into colors; a plain CSS hex string or quoted color name is not a Color.

Run the [example](../assets/typed-colors.amx). Check the actual painted values, not only whether it builds.
