# Browser smoke contracts

Browser checks have two different targets. Neither replaces native scene checks,
SDK integrity, text/raster parity, or transaction tests.

| Gate | Target | Purpose |
| --- | --- | --- |
| `web-smoke` | `web/dev/smoke/` with the just-built SDK | Small capability fixtures: shapes, external text/fonts, SVG, shared runtime and an explicit nested module chain. |
| `site-smoke` | Assembled `_site/` only | Astro navigation, automatic previews, live poster-to-player handoff, WebGL2 and recording fallback, playback and seek. |
| `web-site-smoke` | Assemble `_site/`, then run `site-smoke` | PR/main integration in the existing Web Build job, reusing its SDK. |

The homepage and Playground load their previews automatically. Docs pages load
without a player, and Showcase and tutorial scenes initially show a poster.
Site tests click the public **Play live** action and check that the page
has no player or media requests before activation. They then check poster
visibility during loading, keyboard focus on the ready player and no MP4
requests during live playback.

## Run

Use the pinned environments; do not set shell markers manually:

```sh
nix develop .#web-ci --command tools/ci/ci.sh gate web-build web-smoke web-site-smoke
# An already verified/assembled site:
nix develop .#web-ci --command tools/ci/ci.sh gate site-smoke
```

PR selection retains the existing dependency-aware Web Build job. Main/reusable
release CI still runs the full matrix. The site journey runs before the tested
SDK artifact is uploaded. The standalone Pages workflows and release packaging
also check their assembled site before deploy/package. A static site build alone
is not browser acceptance. `SITE_BASE` must match assembly (for example
`/animatix/` for project Pages); the smoke server mounts the artifact at that
prefix and contracts check prefixed navigation, module and scene URLs.
The SDK suite also accepts `SITE_BASE` to exercise its relative module chain
under a prefix; run it at both `/` and `/animatix/` after import-loader changes.
The Docs and Quickstart journeys check their navigation. The Showcase preset journey
checks the enabled selector, committed public vars, Open code query, downloadable
source and subsequent painted frame. One separate journey disables WebGPU and
checks that WebGL2 actually paints a Showcase scene. A second disables both
backends, verifies the visible poster and late recording choice, starts the MP4
with a user action and retries live playback.

## Failure evidence and retries

`target/web-smoke/<suite>/attempt-<n>/` is uploaded on success and failure. Keep
recovered failures: a green second attempt must not erase the first attempt's
browser logs, timing records or failure evidence. Compare like-for-like runs
(commit, browser, viewport, backend and warm/cold loading), not only total job
duration.

Only exit status **75**, reserved by the runner for recognized browser startup
adapter unavailability during a manifest-only preflight, before any journey, gets one clean restart. JavaScript, missing
resources, scene errors, shader validation, paint assertions and timeouts after
navigation fail immediately. The second failed attempt is final. Browser and
CDP watchdogs must bound hung operations independently of page JavaScript.

## Scope and follow-up

The normal contract is a small representative set, not every historical demo.
Do not lower assertions to make software rendering green. Product-side timeout
errors remain failures even when the harness has a larger watchdog budget.

`SMOKE_DEPTH=deep` covers all five current Showcase works, six lessons and four
techniques, alongside the static navigation, preset, fallback and editor
recovery journeys. A catalog test checks this explicit coverage against the
current route metadata. The additional pages check loading, public seek and
nonuniform painted output.

PRs and pushes (including direct main commits) touching the renderer, graphics
bridge, frame/ABI types, embed loader/player, shared site playback component,
SDK packaging, shaders or scene fixtures automatically use deep in the existing Web
Build job. Missing diff information and conservative full-matrix classifications (including
unknown paths, Nix/Cargo settings and vendored changes) also upgrade to deep.
This can add thirteen journeys for an unfamiliar low-risk path; classify it explicitly
in the planner once its impact is understood rather than silently skipping coverage. Other PRs/pushes and reusable release runs stay critical. The plan records matched paths and
reason; browser timing evidence records the chosen depth. There is no extra
build/job for escalation: the same SDK feeds SDK checks and the assembled site.

Maintainers can run **Web Regression** manually on a selected branch/ref. It has
read-only contents permission, no secrets, no publication steps and a 90-minute
job cap; a newer manual run of the same ref cancels the older one. It builds one
SDK and runs deep site acceptance, retaining evidence for seven days. No new
schedule is enabled: this avoids a nightly rebuild while Actions time is scarce.
Deep adds thirteen page journeys.
A risky PR followed by its merge may run deep twice: without proof that the tested
merge result is identical, skipping main acceptance would create a coverage gap.

Long playback, repeated mount/unmount, lazy-load cancellation and broad font
matrices remain future contracts, not coverage implied by the word "deep".
Record cold and warm measurements before setting thresholds or adding parallel
Chromium workers. A direct source checkout without the built SDK and Chromium
can validate harness contracts, but cannot establish rendered correctness.

## Linux compositor preflight

The browser runs under its own `xvfb-run -a` display from the pinned `web-ci`
shell, with GPU compositing enabled and ANGLE/SwiftShader plus Vulkan/SwiftShader.
It does not connect to the developer's desktop display. A bare WebGPU adapter is
not sufficient: the previous headless/Vulkan setup returned ready/drawn counters
while even native clear-only canvases captured as transparent.

Before importing the SDK, the harness submits one cyan 64×64 WebGPU clear and
requires its pixels in a CDP compositor screenshot. It polls captures, not
renders. Blank preflight pixels fail as an environment/compositor error without
relaxing any scene assertion. `preflight.png` and browser GPU status are retained.
The harness does not use canvas `toDataURL`: it can be transparent even when the CDP
screenshot correctly contains the displayed frame.

Isolated desktop checks verified native task/rAF draws and all four SDK fixtures
with this configuration. Final acceptance must rebuild the SDK for the final
source fingerprint and pass both SDK and assembled-site contracts. Related reproduction: [luma.gl compositor investigation](https://github.com/visgl/luma.gl/issues/2874).

`web-transactions` uses the same isolated Xvfb/SwiftShader launch and native
compositor preflight before its SDK scenarios. It reloads a fresh document before
counting SDK devices/resources. Startup or preflight failures are recorded as
`environment`, not transaction failures; neither category is automatically retried.
The transaction assertions and explicit hardware-only mode are unchanged. Cleanup
signals the detached process group, allowing Xvfb to clean up before forced exit.
