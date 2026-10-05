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
- **Why static faces, not variable**: the axis is not reachable in this stack.
  `fontdb` 0.23 has no variation API (no `variation`-named item in its source),
  so a variable face registers as its default instance for both text paths —
  measured with the system `Noto Sans CJK JP` VF: identical ink at
  `font_weight` 400, 600 and 800. The variable pair
  (`OpenSans[wdth,wght].ttf` + italic, 532,636 + 583,992 bytes, axes
  `wght 300–800`, `wdth 75–100`, 14 named instances — vendored to
  `/tmp` during the 2026-10-05 probe, deliberately **not** committed) is
  therefore larger *and* no better until something can instance it. Swap it in
  when the font stack gains that: it would replace all four statics and update
  these hashes + `BUNDLED_FONTS`.

- **SHA-256** (verify with `scripts/refresh-fonts.sh`):

  | File | SHA-256 |
  |---|---|
  | OpenSans-Regular.ttf | `8ab4aa561e7db0eb3e1af8b0bed2a315e0a33fe2ed3070e645d1b89f8efc1d5c` |
  | OpenSans-Bold.ttf | `1a6bc6775358bfed0e4191b6f2c4d7d75d122f0c6e5a255f264ab455c67237b7` |
  | OpenSans-Italic.ttf | `e5178be12cd740aeafebea15ec563fe577bbb4fab42d9e40500bd49ec8c9ce16` |
  | OpenSans-BoldItalic.ttf | `b5c44af3cb55f65fadb2f1b20edc38e1008bb71388d04ad127c5ad340c9329f2` |

## Fira Math

`FiraMath-Regular.otf` — the math font used by `Math`/`$...$` rendering
(SIL OFL; Fira is an open-source project by Carmen and Bernhard).

## Refreshing

`scripts/refresh-fonts.sh` verifies the vendored files against the pinned
SHA-256 table (fails loudly on mismatch). To re-vendor on purpose, replace the
files from a trusted source, update the hash table (in this README and in the
script), and re-run the script.