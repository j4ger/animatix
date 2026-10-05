# Bundled Fonts (vendored assets)

Animatix bundles a small, deterministic font set so text renders identically
everywhere (offline, CI, export) without depending on the host's fonts. System
fonts are still used as glyph fallback for scripts the bundled set cannot cover
(e.g. CJK).

## Open Sans (default family)

`OpenSans-Regular.ttf` (400 normal), `OpenSans-Bold.ttf` (700 normal),
`OpenSans-Italic.ttf` (400 italic), `OpenSans-BoldItalic.ttf` (700 italic) — four
static faces, each carrying its own weight/style into `BUNDLED_FONTS`. Bold ships in
every build (the slim web embed too); the two italics are `rich-text` only.

- **License**: Apache License 2.0 (`LICENSE-OpenSans.txt`). This is the
  license of the packaged Open Sans these faces came from. (Upstream releases
  have also been distributed under SIL OFL 1.1; either license permits
  redistribution with attribution.)
- **Why the fast path can pick among them**: `BundledFont` records each face's
  own `weight`/`style`, and `bundled_face()` chooses the way a font database
  does — style first, then nearest weight. Before that metadata existed the
  plain-text fast path took the family's *first* entry, so `font_weight` and
  `font_style` drew nothing for bundled text on a machine without the family
  installed (measured: identical ink at weights 400/600/800, and `fc-match
  "Open Sans"` on this box falls back to Noto Sans CJK — the bold that renders
  now is the bundled face, not a system one).
- **Static *and* variable, by text path.** The four statics above are what the
  plain-text fast path uses: it shapes with `ttf-parser`, which reads `fvar`
  but never applies the deltas to outlines, so a variable face there would
  render only its default instance. `fontdb` 0.23's lack of a variation API is
  a separate thing — it is only used to *scan system fonts*.
  The rich (Typst-powered) path additionally registers the variable pair
  (`OpenSans-Variable.ttf`, `OpenSans-Italic-Variable.ttf`; axes `wght 300–800`,
  `wdth 75–100`), whose `wght` axis Typst instances during shaping
  (`Font::instantiate` → `FontVariations::resolve`). Serving a weight from the
  axis instead of a face is what makes `font_weight: 625` draw 625 and an
  animated weight ramp smoothly; measured compiled ink for one 64 pt line runs
  510.7 / 516.4 / 522.1 / 527.7 / 533.4 / 544.6 / 555.9 px across 400…700 in
  50-step increments. The statics stay in the book: they are the fallback for
  families without an axis and what the fast path needs, and Typst's face
  distance prefers the axis-capable face for any in-range request.
- **The cost of that swap**, measured: at a canonical weight the variable
  instance is about 0.4% wider than the packaged static (400: 508.85 → 510.67 px,
  700: 553.83 → 555.88 px for the same line), which shows up in
  `examples/layout/27_layout_text.amx` as 496 differing pixels of 921,600 and
  does not rewrap any line. Serving canonical weights from the statics and only
  the in-between values from the axis was tried and rejected on the same
  measurement: it makes the ramp non-monotone (`450` drew heavier than `500`,
  and `599 / 600 / 601` came out 533.2 / 553.8 / 533.5).
- **SHA-256** (verify with `scripts/refresh-fonts.sh`):

  | File | SHA-256 |
  |---|---|
  | OpenSans-Regular.ttf | `8ab4aa561e7db0eb3e1af8b0bed2a315e0a33fe2ed3070e645d1b89f8efc1d5c` |
  | OpenSans-Bold.ttf | `1a6bc6775358bfed0e4191b6f2c4d7d75d122f0c6e5a255f264ab455c67237b7` |
  | OpenSans-Italic.ttf | `e5178be12cd740aeafebea15ec563fe577bbb4fab42d9e40500bd49ec8c9ce16` |
  | OpenSans-BoldItalic.ttf | `b5c44af3cb55f65fadb2f1b20edc38e1008bb71388d04ad127c5ad340c9329f2` |
  | OpenSans-Variable.ttf | `36643644f318a812aab2d2ed3bb98f8cf0872527f835fe9398d95fe6b9adb878` |
  | OpenSans-Italic-Variable.ttf | `fe269381e992f32e135801740998544d6235061e37c93ec067ad2be3edd5b17b` |

## Fira Math

`FiraMath-Regular.otf` — the math font used by `Math`/`$...$` rendering
(SIL OFL; Fira is an open-source project by Carmen and Bernhard).

## Refreshing

`scripts/refresh-fonts.sh` verifies the vendored files against the pinned
SHA-256 table (fails loudly on mismatch). To re-vendor on purpose, replace the
files from a trusted source, update the hash table (in this README and in the
script), and re-run the script.