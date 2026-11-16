# Patched third-party dependencies

These are third-party sources, not Animatix workspace members or downloaded
build caches. The root `Cargo.toml` selects them through `[patch.crates-io]`.
Keep upstream license and provenance files with each copy. First-party structure
and release-version rules must not include this directory, but changes here
must participate in SDK input fingerprints and full CI coverage.

| Dependency | Source and local changes | Removal condition |
| --- | --- | --- |
| [Vello 0.11.0](https://github.com/linebender/vello) | Persistent image-atlas repair and a synchronous submit-scoped encoder callback; see [patch notes and boundary tests](vello/README.animatix.md). | Upgrade only when both boundaries have upstream equivalents and the image/presentation regression tests pass. |
| [wasm-split-cli 0.8.0-alpha.3](wasm-split-cli/README.md) | Pinned Dioxus source with relocation, overlapping data ownership and shared-capability fixes; the package README records the revision, patches and ABI checks. | Remove the local patch only when upstream handles these cases and passes Animatix's Web capability and graphics checks. |

Directory relocation must not change dependency behavior. Update Cargo patch
paths, CI classification, structure exclusions and SDK input coverage together.
Do not edit upstream source as part of repository housekeeping.
