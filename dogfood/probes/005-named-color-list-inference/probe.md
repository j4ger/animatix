# Probe: named colors in lists infer as List<Color>

Status: resolved for type checking; remaining linter gap is unrelated.

## Intent

A reusable `Swatches(colors: List<Color>)` component should accept the same
named color literals that work in ordinary color properties.

## Minimal Repro

See `repro.amx`.

```animatix
pub component Swatches(colors: List<Color>) {
  for c, i in colors {
    dot[i]: Ellipse, size: (40, 40), color: c
  }
}

s: Swatches, colors: {red, green, blue}
```

## Expected DSL

`{red, green, blue}` should infer as `List<Color>`.

## Resolution

`animatix check` now accepts the named color list. The shared symbol-aware
`TypeEnv` in `animatix-syntax::typing` knows named colors, color namespaces,
color constructors, component params, actor labels, and list common types.

## Remaining

None for this probe. The repro references `s` with `fade-in` so the linter
has a direct use; the original type mismatch is gone.

## Regression coverage

- `typing::tests::named_colors_are_colors`
- `typing::tests::list_infers_common_color_type`
- `typecheck::tests::named_color_list_accepted_for_list_color_param`
- The repro remains as a dogfood regression fixture.
