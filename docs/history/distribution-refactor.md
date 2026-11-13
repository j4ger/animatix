# Web and Linux distribution refactor (2026-11)

This change replaces the former browser profile packages with one SDK package:
one shared runtime plus on-demand rich text, media, and raster-encode capability
blocks. The package manifest identifies versioned files, fonts, sizes, hashes,
and the build identity. The loader verifies the entry module with SRI and checks
Wasm and capability bytes before use. Font faces are gzip assets selected from
the manifest; static hosts must serve the package files with appropriate
same-origin or CORS access. See [the web player guide](../../web/README.md).

Document replacement is transactional: preparation and build must succeed
before a candidate replaces the current document. `applySource()` and
`setVars()` return promises; `setVars()` batches writes over 150 ms. `data-vars`
replaces the declarative variable set, while `setVars()` merges overrides.
Failures retain the last committed document. Resource-fetch failures, including
font fetch failures, are reported as resource errors; build errors remain
distinct. Explicit `destroy()` cancels outstanding work and releases the
player; DOM detachment and remount preserve it.

Linux video-enabled distribution now includes a matching corresponding-source
archive with the allowlisted source snapshot, Cargo lockfile and vendored
dependencies, offline build setup, pinned media sources, and rebuild recipe.
The binary package records and checks the source archive identity. See the
[Linux packaging guide](../../packaging/linux/README.md).

## Evidence and remaining checks

Evidence below is from the current checkout unless stated otherwise. The files
under `target/perf/distribution-refactor/` are local, ignored artifacts and are
not included in source releases.

- Commits `d8a2c321`, `0b66e020`, `c301859b`, `6c303a9e`, `5dd2a5f4`,
  `96c1fddd`, `94c36356`, and `b02dab8b` record the Linux distribution tooling,
  WebM duration correction, scene text-fit fixes, repaired Wasm splitter,
  bounded audit scheduler, shared native input/palette preparation, canonical
  SPDX reference materials, and impact-based benchmark policy. These eight
  commits do not establish completion of the broader refactor.
- `shared-input-candidate-verified.log` and
  `shared-input-candidate-lint.log` record successful `fmt`, `structure`,
  workspace `check`, `lint`, and two focused native-input tests for the
  independently archived candidate at `5dd2a5f4`. Commit `96c1fddd` records
  this native input/palette group.
- `final-bidi-caption-native-gates.log` records the latest full native gates
  passing, including workspace formatting, structure, check, lint, tests,
  no-video, and eparts. It includes 87/87 `animatix-text` tests. Separate
  focused bidi evidence is in `bidi-focused-final.log` (11/11),
  `bidi-text-default-final.log` (87/87), and
  `bidi-text-rich-only-final.log` (21/21 without default features); structure
  passed in `bidi-structure-final.log`. Caption-focused coverage passed 17/17
  in `captions-focused-test.log`. The bidi tests exercise paragraph visual
  order, source gaps and indices, mirrored brackets, Arabic glyph coverage,
  default-ignorable controls, and combining-cluster tracking.
- `final-native-extra-gates.log` records passing documentation, example
  validation, 63 render-smoke scenes, and `site-scenes` checks.
  `final-audit-gates.log` subsequently records 103 successful `scene-audit`
  results, including `examples/gallery/epicycles.amx` and
  `web/demos/transformer/scenes/tokens.amx`. The 103-scene budget audit also
  passed in `final-budget-security-bench-gates.log`. Its first security-audit
  attempt failed while fetching the RustSec database due to a network error;
  `final-security-bench-retry.log` records a successful audit retry and
  `ext-bench` gate (overall exit 0).
- SDK build `fd9da6f9331bae3c9949952be19dc79dbe9a8c4495148f734dfdab255378bfd2`
  corresponds to source commit `94c36356`. Its browser transaction report
  `final-bidi-caption-final-gpu/report.json` records all 28 checks passing.
  `final-bidi-caption-refreshed-remainder.log` records passing SDK integrity,
  text parity, raster proof, and web-smoke routes. The 12-frame native/WebGPU
  visual comparison in `final-bidi-caption-visual-fd9.log` reports
  `visual_pass: true` across equation, captions, counter, and plain/CJK/bidi
  fixtures at three production-quality times on the NVIDIA adapter. These
  functional and visual results are for the fd9 SDK/source revision; they do
  not verify the later policy SDK build or its pending performance comparison.
- Generated Typst text has a diagnostic browser case for selecting its dynamic
  CJK fallback; native tests cover generated-text out-of-coverage errors. The
  earlier counter-prefix clipping observation was reproduced on the old
  renderer too, so it is pre-existing rather than a regression introduced by
  this change.
- `final-linux-duration-gate.log` records Linux container runtime checks on
  Ubuntu 22.04 and 24.04 after the audio-duration correction. The MP4 container
  duration is 1.0 s, with 1.021678 s decoded AAC due to one 1024-sample
  encoder-padding frame; WebM container duration is 1.008 s with 1.0 s decoded
  PCM. Both checks decoded five video frames and the 440 Hz tone. The final
  corresponding-source archive still needs regeneration after remaining source
  changes; these results do not establish final archive identity.
- The current policy SDK build `032145f1d5fc60187ac769d717face9254c8aa3aadb9adcfd314b0aa74079b3d`
  passes the size gate in `final-policy-web-gates.log`: primary gzip is
  1,701,880 bytes against the user-recalibrated 1,750,000-byte budget; raw Wasm
  is 4,911,055 bytes. Wasm, fonts, SDK, and site totals also pass their recorded
  limits. The earlier 1,700,000-byte limit failed by about 1.9 KB. No
  compression setting or production tracking implementation was changed to
  reach this result; the proposed allocation-free candidate was abandoned and
  not applied.
- Commit `b02dab8b` replaces the prior relative-only CPU verdict with a
  workload-impact policy. The 5% change is a signal boundary, and a direct
  user-facing workload blocks only when its conservative ratio interval is
  wholly above that boundary and its conservative absolute delta exceeds the
  workload budget. The current provisional budgets are 167,000 ns for one
  frame (1% of a 16.7 ms frame target), 100,000 ns for an edit/build action
  (1% of a documented 10 ms small-scene target), and 167,000 ns per frame for
  a 100-frame batch. Isolated leaf benchmarks without a documented invocation
  scale are informational; the 10,000 ns property lookup remains an emergency
  sentinel. These are configurable engineering budgets, not new elapsed-time
  SLOs or perceptual claims.
- The policy-only re-evaluation in `final-policy-evaluation.stdout.log` applies
  that policy to the preserved 72 baseline/current benchmark samples: all 72
  are accounted for, with zero material regression blocks, zero final-gate
  inconclusive results, and no policy/baseline failures. The cached
  `analyzer_diagnostics__large` leaf remains a signal (+57.5 ns, +10.26%,
  conservative ratio interval +10.11% to +10.42%) but is informational because
  its invocation scale is not established. This reclassification does not
  alter either raw sample or baseline. The earlier strict relative-only runs
  remain recorded: `final-perf-compare.log` had one failure at +7.94%, and
  `final-accepted-perf-compare.log` had one at +10.26%. Criterion's intervals
  compare within-run estimates and do not model process/session variation.
  The separate 12-pair analyzer comparison (ratio 1.0090, 95% interval
  [0.9911, 1.0273]) is supportive, not a replacement for the preserved 72
  samples. No new CPU timing run is implied by this policy evaluation.
- The Web paired-comparison policy and its tests are recorded in
  `performance-policy-checkpoint-gates.log`. The subsequent independent review
  closed partial CPU-suite and empty Web-scene false passes. The final
  `final-integration-fast-gates.log` records 14 Python policy tests and 26
  distinct Web runtime/statistics tests passing; duplicate test registration
  was removed. The policy requires at least three valid pairs; defaults are
  six cold-start pairs and three warm-start pairs. The provisional primary
  absolute floors are 50 ms for cold startup, 16.7 ms for warm startup, and
  0.167 ms for queue-depth-one p95, alongside a 5% relative signal boundary.
  A material block requires the paired 95% interval to clear both the relative
  and absolute floors; overlap is inconclusive. These intervals are based on
  pairs from one Chromium process, are limited at these sample counts, and do
  not provide family-wise or cross-session confidence. Other captured metrics
  are diagnostic. The Wasm memory metric is exported-memory capacity, not
  resident memory; GPU allocations and non-exported memories are not measured.
  The real-GPU paired Web A/B timing has not yet been run on the fresh SDK, so
  this policy test evidence is not a performance result.
- An isolated ad08 old-native renderer produced 12 production-quality
  960×540 frames on the NVIDIA RTX 4060 Laptop GPU with the explicit nine-face
  font set; adapter details and frame hashes are recorded under
  `old-native-visual/`. Nine frames are the original equation, captions, and
  counter fixtures at three times each. The remaining three use a compatible
  copy of the plain-text fixture: its original literal includes `#markup`,
  which the old Typst implementation rejects as an undefined identifier. The
  exact fixture-only substitution is documented in
  `old-native-visual/fixtures/plain-compatible.patch`. These are old-side
  captures. The ignored `human-review-fd9/` sheets now place the old frames
  beside the fresh fd9 native frames using the same nine font faces and hashes.
  The plain-text sheet crops out the differing literal line and compares the
  shared CJK, Arabic, bidi, and weight lines. Human visual review found whole
  Chinese phrases highlight together, Latin timing is correct, RLE brackets
  are ordered correctly, and weight 550 appears between 400 and 700. The old
  and new counter both retain the pre-existing half-height clipping. The stale
  earlier contact sheet remains superseded.
- License generation now includes canonical SPDX references for the pinned
  `fax` 0.2.6, `fax_derive` 0.2.0, and `glidesort` 0.1.2 revisions, separately
  from upstream license texts. Their `missingLicenseText` status remains true:
  exact upstream license text has not been verified and added. Distribution
  license material is therefore still incomplete.
- The Web A/B comparison on the fresh SDK remains pending; no current-source
  Web performance result is recorded here. Final Linux archive/runtime
  evidence must be regenerated from the current source; real macOS and Windows
  package/runtime runs remain unverified.
- Three runtime dependency license texts remain absent from the local registry
  package sources and `.crate` archives: `fax` 0.2.6 and `fax_derive` 0.2.0
  both identify pdf-rs/fax revision
  `40b226eb97b391355e971fd95a8dd532a4c127a0` (derive path `derive`), while
  `glidesort` 0.1.2 identifies orlp/glidesort revision
  `c4df9518ec23a424a867b3d50967f0aaafe3c0fd`. The pinned fax source tree has no
  `LICENSE` file; the exact-revision raw `LICENSE` URL returns 404. For
  glidesort, the upstream README declares MIT OR Apache-2.0, but no actual
  license text was found in the package archive or verified at that revision.
  Do not substitute generic license templates or texts from another revision.
  The distribution's license material is incomplete until exact upstream texts
  can be verified and included. Real macOS and Windows package/runtime runs
  have not been performed.

This section is an intermediate evidence record, not a completion report.
