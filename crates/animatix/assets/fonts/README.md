# Bundled Fonts (vendored assets)

Animatix bundles a small, deterministic font set so text renders identically
everywhere (offline, CI, export) without depending on the host's fonts. System
fonts are still used as glyph fallback for scripts the bundled set cannot cover
(e.g. CJK).

## Open Sans (default family)

`OpenSans-Regular.ttf`, `OpenSans-Bold.ttf`, `OpenSans-Italic.ttf`,
`OpenSans-BoldItalic.ttf` — four static faces.

- **License**: Apache License 2.0 (`LICENSE-OpenSans.txt`). This is the
  license of the packaged Open Sans these faces came from. (Upstream releases
  have also been distributed under SIL OFL 1.1; either license permits
  redistribution with attribution.)
- **Why static faces, not variable**: typst 0.15 (the version this crate pins)
  does support variable font axes, but the upstream Open Sans variable pair
  (`OpenSans[wdth,wght].ttf` + italic, ≈1.1 MB) is *larger* than the four
  static faces (≈850 KB), and the static set is proven and simpler. If a future
  default wants one file per weight-range, the variable pair can be swapped in
  (update these hashes + `BUNDLED_FONTS`).
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