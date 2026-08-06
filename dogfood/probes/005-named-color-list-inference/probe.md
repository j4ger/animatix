# Probe: named colors in lists infer as List<Any>, not List<Color>

Status: open.

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

## Current Workaround

Use colorscheme tokens (`{accent.primary, accent.success, accent.warning}`)
or `rgb(...)` calls so each list element is typed as a color.

## Diagnostics / Behavior

```text
error[build:type-mismatch] Type mismatch: parameter 'colors' of component
'Swatches' expects List<Color>, got List<Any> (from list of 3 items)
```

## Impact

Component parameter typing rejects the most readable list form for colors.

## Recommendation

Infer `Expr::Ident` values that match built-in named colors as `Color` when
typechecking list literals and component parameter lists. Add a regression
test for `List<Color>` with named colors.
