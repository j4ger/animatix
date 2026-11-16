# Text parity fixture font

`NotoNaskhArabicUI.ttf` is an extra test font for the Arabic shaping/bidi case.
It is not part of the default web SDK font pack.

- SHA-256: `4713198071a99afad0a596df8ab3c71b1b59da092ad4f3c27c439949a7118fc8`
- Installed source: Nix `noto-fonts-2026.10.01`, from `notofonts/notofonts.github.io` tag `noto-monthly-release-2026.10.01`.
- Upstream typeface source: `notofonts/arabic`, revision `133ccaebf922ca080a7eef22998611ac3c242df9` (Google Fonts `METADATA.pb` records this repository/revision and OFL license).
- Embedded family: Noto Naskh Arabic UI; copyright 2019 Google LLC.
- License: SIL Open Font License 1.1, reproduced in [OFL.txt](OFL.txt).

This font is passed explicitly to both native and wasm probe runs. It does not
change the SDK's shipped fonts or normal font fallback behavior.
