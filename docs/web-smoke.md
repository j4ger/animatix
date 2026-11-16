# Browser smoke contracts

Browser checks have two different targets. Neither replaces native scene checks,
SDK integrity, text/raster parity, or transaction tests.

| Gate | Target | Purpose |
| --- | --- | --- |
| `web-smoke` | `web/dev/smoke/` with the just-built SDK | Small capability fixtures: shapes, external text/fonts, SVG and shared runtime. |
| `site-smoke` | Assembled `_site/` only | Real Astro navigation, click-to-load scenes, playback/seek and playground Run. |
| `web-site-smoke` | Assemble `_site/`, then run `site-smoke` | PR/main integration in the existing Web Build job, reusing its SDK. |

The historical `web/` HTML gallery is not the shipped website. `site-artifact`
removes that HTML and overlays Astro. Testing the old gallery therefore cannot
establish that a release works. Pages with no player are valid: the homepage is
static, compatibility pages link onward, and interactive scenes initially show a
load button. Site tests must follow those user journeys rather than eagerly
creating every player or invoking private loader state.

## Run

Use the pinned environments; do not set shell markers manually:

```sh
nix develop .#web-ci --command scripts/ci.sh gate web-build web-smoke web-site-smoke
# An already verified/assembled site:
nix develop .#web-ci --command scripts/ci.sh gate site-smoke
```

PR selection retains the existing dependency-aware Web Build job. Main/reusable
release CI still runs the full matrix. The site journey runs before the tested
SDK artifact is uploaded. The standalone Pages workflows and release packaging
also check their assembled site before deploy/package. A static site build alone
is not browser acceptance. `SITE_BASE` must match assembly (for example
`/animatix/` for project Pages); the smoke server mounts the artifact at that
prefix and contracts check prefixed navigation, module and scene URLs.
Compatibility pages must retain their onward links. The showcase preset journey
checks the enabled selector, committed public vars, Open code query, downloadable
source and subsequent painted frame.

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

This change does not yet claim a scheduled deep-regression lane or an empirically
validated performance budget. `SMOKE_DEPTH=deep` adds the remaining current showcase/lesson pages, but has not
been GPU-accepted here and is not scheduled automatically. Add long playback,
mount/unmount, lazy-load cancellation, font matrices and recovery tests as
explicit contracts, then use renderer/shader/loader/ABI impact to select them.
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
