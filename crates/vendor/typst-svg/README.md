# Vendored `typst-svg`

This copy comes from the published `typst-svg` 0.15.1 crate at
[Typst revision `9dfd3a08500b7896045f907433cf7b4b02434fad`](https://github.com/typst/typst/tree/9dfd3a08500b7896045f907433cf7b4b02434fad/crates/typst-svg).
See [LICENSE](LICENSE) for the Apache 2.0 license.

The SVG exporter uses `hayro-svg` for PDF conversion and imported interpreter
types via the `hayro` rasterizer's re-export. Import those same types directly
from `hayro-interpret` so rich text no longer brings in the rasterizer and its
`vello_cpu` dependency.

Remove this patch when a compatible upstream release drops the rasterizer
dependency and passes Animatix's `renderer-dependencies` and `text-parity` gates.
