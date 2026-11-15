# Web reset review

## Delivered scope

The public collection is now five focused works, six sequential lessons and four
short techniques. Nine legacy showcase URLs have explicit dispositions. This is
an editorial reset, not nine cosmetic rewrites: material with unresolved causal
or mathematical explanations is archived rather than promoted.

New stories are `move-the-basis.amx` and `make-room.amx`. Their geometry, parameter
readouts and counters are described in `web-reset-completion.md`. The reference
model tests independently count insertion-sort comparisons/shifts and check the
shear's determinant and length distinction. They do not replace visual review.

## Findings from real rendering

- Nested `always` inside a loop did not animate every grid line in the intended
  matrix scene. The final source uses explicit line identities and a shared
  reactive block. This is a source workaround, not an engine fix.
- Timed caption string changes produced overlapping old/new text in a sampled
  sorting frame. Explicit fade-out, immediate string change and fade-in made the
  reading order unambiguous. This is not presented as a morph implementation.
- Four technique scenes were checked and rendered into six-frame storyboards
  with the native renderer using SwiftShader. Every storyboard was inspected.
  Static representative PNGs give readers without WebGPU a visible example.

These are useful engine regression candidates: loop-scoped reactive bindings and
successive string transitions should be isolated into small native/WebGPU tests
before either behaviour is advertised as reliable.

## Environment boundary

The restored source base is commit `39276ac`. The cloud computer had lost its
previous toolchain. Official pinned Node and Rust tooling were restored; the
non-video native CLI builds and renders. Videos for the new works use that same
native offscreen renderer at authored resolution and a separate FFmpeg encoder;
no video-feature build outside Nix is claimed.

The first Nix attempt was blocked by namespace/ptrace restrictions when emulating
`/nix`. A later recovery made the standard `/nix` path available and used Nix's
explicit physical-store setting, so the unchanged flake's real devShell now runs.
No host isolation policy or dependency pin was changed. A separate Cargo home
prevents an old rustup formatter plugin from shadowing Nix's pinned formatter.
The exact recovery is environment setup, not a project-code workaround.

Seven relevant gates now pass in the required Nix shell: `fmt`, `site-build`,
`site-scenes`, `content-sync`, `structure`, `web-runtime-tests`, and
`web-sdk-tests`. The batch exited zero. Earlier direct checks should not be
confused with these later Nix runs.

A fresh SDK and assembled distribution now pass their Nix gates. Live browser
WebGPU playback remains a separate acceptance step. A checked source, a native frame and a playing recording do not
prove parameter updates, seeking or recovery in WebGPU.

Six lesson sources were also checked and rendered as six-frame storyboards;
all were inspected without source changes. Their static previews are available
without WebGPU. The unloaded lesson/technique controls use a compact panel instead
of adding a second full-height empty canvas below the still.

Cloud Chromium inspection verified Showcase cards, lesson stills, archive links,
and the old Sorting URL's route to Make Room. The matrix recording loaded at
1280×720, reported 24 seconds, advanced in time and paused through the page control.
The preview server intentionally did not contain a rebuilt SDK; live interaction
was not tested against that incomplete preview.

## Focused verification results

- 16 canonical AMX source checks passed (including the brand scene); hints remain.
- Native storyboard review: two new works (5 and 7 sampled times), six lessons
  (six times each), four techniques (six times each).
- Two new MP4s: each 576 frames, 24 fps, 24 seconds, 1280×720, no audio. A decoded
  sorting frame at 8.5 seconds was also inspected after encoding.
- Astro check: zero errors/warnings; one existing `beforeunload.returnValue` hint.
- Root and `/animatix/` builds: 36 HTML outputs, internal targets valid.
- Focused JavaScript tests, publication negative-fixture tests, catalog validation,
  direct Rust formatting and whitespace checks passed. These were direct checks,
  not successful Nix gate invocations.

Source, poster and video digests bind curated recordings; lesson/technique stills
also record their source and poster digests so later edits cannot silently leave
old visual evidence behind. Historical media are not substituted for new sources.

Independent final code review found no blocking regression in legacy routing,
publication staging, provenance checks, compact panels or the new story models.

## Final SDK and publication acceptance

The first release `web-build` and `web-sdk-integrity` gates passed in `web-ci`;
`site-artifact` passed in a fresh `default` shell. The assembled site contains
36 HTML outputs, all 16 canonical sources and the verified SDK. Its size was
42,812,159 bytes against a 58 MB budget. The SDK was 38,506,526 bytes; primary
Wasm gzip was 1,734,728 bytes, below its 1,750,000-byte budget.

The build reports existing warnings about duplicate keys in generated split JS
and missing exact upstream license text for `fax`, `fax_derive` and `glidesort`.
These warnings are recorded, not presented as fixed by a content reset. A later
release review should resolve the license inventory warnings before treating the
inventory as complete.

The SDK embeds the Git commit. After the local source commit, its build and
assembly must be repeated against that commit; editing the manifest alone is not
an acceptable provenance update. No remote push is part of this checkpoint.

## Mainline integration

The publish merge retains main's event-commit provenance, stable job IDs,
dependency-owned selection, shared native CLI verification and two-worker test
scheduler. Astro's `site-build` belongs to the existing Fast Checks job, which is
selected for both HTML and Astro changes. Canonical AMX sources are also exercised
by the existing workspace SDK test via the real parse/type/build pipeline, with
negative syntax/type cases; the standalone `site-scenes` gate remains a manual
site-stage entry rather than duplicating it in the PR matrix. The selector treats
`web/content/scenes/` as scene content alongside the older scene directories.

The downloaded-SDK restore path also no longer imports esbuild eagerly: the
bundler is loaded only by asynchronous assembly, whose CLI call already awaits
completion. Verification keeps all integrity/provenance checks but has no npm
build dependency. An isolated-checkout regression verifies a complete fixture
without node_modules and still rejects a tampered payload. This addresses the
pre-existing Pages restore failure seen on main, not a reason to weaken checks.

Canonical source certification pairs the native SDK host build with the real
Analyzer's authored-property diagnostics (test-only dependency). The SDK host
alone accepts some invalid authored property types, and CLI `check` can return
success for warnings; the canonical test explicitly rejects warning/error
findings and retains syntax/type-negative tests.
