# Probe: Rect default stroke leaves asymmetric white edge artifacts

Status: resolved.

## Intent

A plain `Rect` with no explicit `stroke` should render as a clean filled
rectangle. In practice it has a white edge on the left and right sides at
some zoom levels, with the top/bottom edges clean.

## Minimal Repro

```animatix
config { colorscheme: "editorial-dark", resolution: (640, 360) }

#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
```

## Expected DSL

The same declaration, with no explicit border.

## Current Workaround

None needed after the fix.

## Diagnostics / Behavior

`animatix check` reports no diagnostics. Rendering at 1280x720 showed the
default `stroke_width: 2` white stroke only on the left and right edges; A/B
frames are pixel-identical, so this is not a variant-specific rendering bug.

## Impact

Any plain `Rect` used as a card or panel can look like it has a white hairline
border, which is confusing during visual review.

## Recommendation

Filled shapes now default to `stroke_width: 0`, while stroke-only actors
(`Line`, `Arrow`, `Callout`) keep a visible default stroke. `draw-in` and
`reveal-in` add a fill-colored outline when a filled shape has no authored
stroke, so the default render stays clean without removing the reveal effect.

## Regression coverage

- `filled_shape_defaults_to_no_stroke`
- `stroke_only_shape_keeps_default_stroke`
- `explicit_filled_shape_stroke_is_preserved`
- `draw_in_adds_visible_stroke_to_filled_shape`
- `reveal_in_adds_visible_stroke_to_filled_shape`
