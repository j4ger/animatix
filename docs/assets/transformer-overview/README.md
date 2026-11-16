# Transformer README illustration

This archive preserves the README image and its historical demo, not an active
visual-regression baseline. All files originate from this MIT-licensed repository.

## Provenance

The PNG and original scene coexist at commit
[`52dabbfe`](https://github.com/j4ger/animatix/commit/52dabbfe318fab987d66b2adb7bf58f5d74aac91).
The saved baseline manifest's source SHA-256 matches that original scene exactly.
Its recorded sample is 11 seconds at 1280 × 720. `provenance.json` records the
original paths and hashes. The PNG is unchanged; only three import prefixes in
`overview.amx` were relocated. The three module files are unchanged historical
copies, deliberately independent of the current example library.

This is the complete local source-import closure. No external image, audio or
font file is referenced. Text still uses engine-provided fonts and rendering.
The exact renderer, GPU and font environment used for the original image was
not recorded. We have not rendered this archive with the current engine, and do
not claim pixel-identical reproduction or current-engine visual acceptance.

To inspect the demo from the repository root with a built CLI:

```sh
animatix check docs/assets/transformer-overview/overview.amx
animatix image docs/assets/transformer-overview/overview.amx --time 11 --output /tmp/transformer-overview.png
```

## Retirement

At main `bf1039f1`, all seven Transformer baseline manifests had source hashes
that differed from the current scenes. Their GPU-only verifier was not a CI gate.
The obsolete 56 PNGs, seven manifests and dedicated verifier were retired rather
than accepting unreviewed new baselines. The seven existing demo scenes and the
CLI's general `baseline` feature remain untouched. Only this README illustration
is retained here; Git history preserves the retired material.
