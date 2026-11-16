# Web reboot: implementation notes

## Architecture

`web/app/` is an Astro static site. No UI framework is hydrated. TypeScript is reserved for explicit scene loading, presets and the source workspace. The existing Rust/Wasm SDK remains an external, versioned distribution at `runtime/`; Vite must not bundle it or relocate its manifest. All generated site URLs use the configured base, including GitHub Pages `/animatix/`.

Primary navigation is Home → Showcase / Learn / Playground. Astro provides archive and compatibility routes. The former interactive HTML and its generator are retired; selected old AMX remains as regression input and is not deployed. Developer diagnostics live under `web/dev/`.

`web/content/scenes/` contains new canonical AMX sources. Learn imports those exact bytes at build time. Playground fetches those files, explicitly runs edited text and downloads the textarea contents. It keeps no permanent local storage. Switching source or resetting altered text requires confirmation; a failed fetch/build retains text. Runtime initialization is single-flight and failed initialization is retryable. Source loading and building are mutually exclusive.

Showcase presets use `setVars`; only a successful rebuild changes the editable/downloadable preset. `withOverride` writes that same scalar into the canonical source. Phase means initial phase; the animation subsequently traverses a cycle. No hidden two-way editor/preset synchronization exists.

## Content and delivery boundaries

The initial collection is One Rule Many Forms, When Waves Meet, and Words Keep Time. The latter is a **silent edition**, not a demonstrated audio-sync workflow. The six lessons have separate minimal sources rather than duplicated excerpts. The homepage decorative icon animation uses CSS, pauses on request and respects reduced motion; it is not represented as engine-rendered evidence.

Covers are native-rendered PNG frames with timestamps and source hashes recorded in `web/content/preview-manifest.json`. Actual scene playback only begins after a user requests it; pages never initialize nine simultaneous players. A preview failure leaves the explanation, source editing and downloads available.

## Recovery provenance

The previous cloud workspace was reset. The public `aa4d8d1` checkout and the Library bundles for `b86d32b` and `87f74a2` were restored. Subsequent alignment/layout/domain/redesign/type/showcase patches were applied in chronological order. The alignment checkpoint contained only a diff, not its commit object, so this working tree does **not** claim to recreate the full original `700876f` history. Old content was restored file-by-file without deleting it. New work is on `experiment/astro-reboot`, based on `87f74a2`; record new commits honestly rather than manufacturing prior identities.

No remote push or user-local checkout modification is part of this work.

## Final native and browser evidence

- Native CLI+video and the standard production SDK were rebuilt in Nix from cold caches. The final SDK verification is owned by the distribution stage; do not reuse a stale manifest after build inputs change.
- All ten new AMX files and three preset variants passed CLI check. Thirteen storyboard sets were actually inspected. A check-passing exponential precedence mistake in One Rule was fixed after it produced an empty render.
- Three native recordings are available at 1280×720,24fps, with exact20/28/20-second durations. Encoded-video1fps contact sheets were inspected. They are explicitly default-parameter recordings, not interactive renders.
- The browser loaded the real SDK/Wasm but reported **no available WebGPU adapter**. Live Run, live parameter changes and live seek/replay cannot be marked passed on this cloud browser.
- The final 1280×720 recording played in the real browser; Pause held at 8.04 seconds and Restart resumed from 0.28 seconds. Moving from recording to unavailable interactive playback showed a concise error and allowed returning to recording. Recording controls are distinct from live parameters.
- Learn → first lesson → Playground → Download was exercised. The downloaded file was byte-identical to the canonical AMX. Cancelled Reset and failed runtime initialization retained edits.
- 500 CSS-pixel layout overflow was discovered and fixed; 400 CSS-pixel Learn/Playground layouts were checked with no horizontal overflow. The 400-width case used browser zoom on a 500-pixel window, not a physical phone.
- Presets are source-synchronized: successful setVars updates the code link and downloadable scalar default. Unit tests cover supported values, rejected query injection and exact source replacement. Actual successful browser setVars still needs a GPU-enabled browser.

## Remaining acceptance

Gate1's successful live edit→Run loop remains blocked by the cloud browser's adapter availability. Recordings make the showcase viewable, but do not substitute for that test. Gate2 audio is also outstanding: existing repository narration assets have no separately recorded provenance matching Words Keep Time. The silent edition does not claim narration alignment. Full native exports and sampled frames are not a claim of watching every frame in real time.

The static build uses a Nix-pinned Node toolchain plus npm's exact dependency lock and `npm ci`; it does not yet provide an offline Nix npm-cache closure. The review overlay is separate from the current navigation. The [retired work manifest](https://github.com/j4ger/animatix/blob/530b5f8843d31a77b19540ff8f42daf662fddfff/web/works.json) is available only at its historical revision.

## Final distribution checks

Both root and `/animatix/` assemblies passed SDK current/release verification, 13 new page checks (28 HTML including compatibility pages), 10 canonical AMX checks, preview hashes, internal links and the 58 MB size budget (41.63 MB actual). Pages uses configure-pages base_path, supporting both project paths and custom-domain roots. Actual HTTP browser verification of `/animatix/` covered Home → work → recorded playback (1280×720) → Open code with the expected source loaded.

## Editorial and interaction polish

The first review exposed an unclear premise in two works. One Rule now explains its visual cause (one actor, copies, a distance-based delay, then a different distance rule). It is not advertised as a benchmark. The old Words URL now contains Same Words, New Meaning: one complete sentence receives three different emphases, followed by a readable comparison. Waves uses separate semantic labels with an analytical fade-through; its continuously changing numerical readouts remain immediate and accurate.

Six lessons now explain the concept before the exercise. Source fetches and player-module registration have bounded waits and retryable errors. The editor warns before discarding unsaved edits, distinguishes diagnostics for an older submitted version, and disables downloading before the initial source arrives. Successful preset changes restart playback so a user at the held ending can actually see the new rule.

Publication uses an independent generated site manifest with exact new-page and bundle hashes. Old compatibility pages cannot hide a missing new route. The manifest check does not depend on the builder's current generated Astro output directory. Every site-build also checks the `/animatix/` base variant. Nix supplies the toolchain; npm's lock and `npm ci` supply the JavaScript dependency graph. This is still not an offline Nix dependency closure.

### Final native gates

`cargo check --workspace --all-targets`, the complete `scripts/ci.sh gate test`, and `scripts/ci.sh gate fmt` passed. The test log reports 2,526 passed, zero failed and 27 ignored across 53 result groups. A first parallel compilation was interrupted because it overloaded the shared machine; the successful run used two build jobs with the same test gate. Native export used a separately retained video-capable binary after the test build replaced the default CLI executable.

During the original reboot, resting compositions were accepted through native storyboards and video contact sheets; the old HTML-discovery checker was not evidence for Astro. `scripts/site-rest-check.py` now selects canonical `web/content/scenes/` directly, but its current GPU measurements must still be run before claiming this manual check passed.
