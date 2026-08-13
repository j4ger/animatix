# Probe: Rect default stroke leaves asymmetric white edge artifacts

Status: open.

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

None clean. The artifact appears in both dogfood A/B variants and in the
minimal repro, but not at every resolution/position.

## Diagnostics / Behavior

`animatix check` reports no diagnostics. Rendering at 1280x720 shows the
default `stroke_width: 2` white stroke only on the left and right edges; A/B
frames are pixel-identical, so this is not a variant-specific rendering bug.

## Impact

Any plain `Rect` used as a card or panel can look like it has a white hairline
border, which is confusing during visual review.

## Recommendation

Inspect how `KurboShape::Rect` and `build_vello_path` emit geometry relative to
the centered half-size, and either suppress the default stroke for filled
shapes or align the path so the stroke is symmetric/outside the fill.
