# Development guide

For contribution scope and review expectations, start with [Contributing](../CONTRIBUTING.md).

## Build and run

The supported Linux development environment is pinned in `flake.lock`. With
[Nix](https://nixos.org/download/) and flakes enabled:

```sh
git clone https://github.com/j4ger/animatix.git
cd animatix
nix develop
cargo build --workspace
cargo run --bin animatix-gui -- web/content/scenes/first-move.amx
```

The default shell provides Rust, graphics/audio dependencies, FFmpeg headers,
Tree-sitter and optional release-note drafting with `git-cliff`. Workspace builds need ALSA on Linux even when
you only intend to test the engine, because the GUI depends on audio playback.
A missing ALSA or FFmpeg `pkg-config` result usually means the development shell
is not active; changing Cargo dependencies is not the fix.

Optional direnv users can create a local `.envrc` containing `use flake`, then
run `direnv allow`. The file is ignored so personal shell customizations stay local;
`nix develop` works without it.

From this shell, use the CLI to inspect and review a scene:

```sh
cargo run --bin animatix -- ast web/content/scenes/first-move.amx --compact
cargo run --bin animatix -- check web/content/scenes/first-move.amx
cargo run --bin animatix -- render web/content/scenes/first-move.amx --loop
cargo run --bin animatix -- image web/content/scenes/first-move.amx --time 1.5 --output frame.png
```

### Downloaded builds

[Releases](https://github.com/j4ger/animatix/releases) list the available artifacts.
For Linux x86-64, download and extract the Nightly archive ending in
`-linux-gnu-video.tar.gz` (the archive with `-source` is for rebuilding).
Run `./bin/animatix doctor` or `./bin/animatix-gui` from the extracted
directory; no Rust, Nix, or separate FFmpeg install is required. The
[Linux packaging guide](../tools/release/native/linux/README.md) describes
host graphics and audio requirements and optional desktop installation.

Documentation on main may describe features that have not reached Nightly.
See `share/BUILD-METADATA.txt` inside the Linux bundle for its exact source
commit. If the installed workbench does not offer the template picker, use
`./bin/animatix init` with a project path, then open the generated `main.amx`.
Older bundles may generate a different set of project files; keep all files
from the project directory together.

### Start from a built-in template

| `animatix init --template` | Starting point |
|---|---|
| `narrated` (default) | An 11-second project with bundled narration and synchronized captions |
| `math-explainer` | An animated graph with a formula and a takeaway |
| `metric-brief` | Three metric cards with a summary |

The CLI and the workbench's **New project…** menu offer the same starters.
On the welcome screen, **Start with narration** opens the narrated starter
directly. Choose a new or empty project folder.
Each project comes with `main.amx` and a portable `layout.amx`. Keep the files
together when copying the project. You can also import `layout.amx` from an
existing scene to reuse `ChapterHeader`, `Panel`, `CaptionPlate`, or `MetricCard`.
The same module supplies the backgrounds, sequence tiles, and signal lanes
used by the public scenes.
[Shared layout source](../web/content/templates/layout.amx)

Use `Backdrop` for a soft palette-aware stage. Its `center` and `radius` are
normalized to the canvas, and `strength: 0` yields the flat scene background. Add a
`FocusWash` behind the subject for a local tint. Its `at` can follow a
keyframe or a value already used by the scene, and `strength: 0` makes it
transparent. The Wash's `reach` scales with the canvas on each axis. Both
components are static unless the source animates them.

Use `Panel` with an `@content` slot to give content an auto-sized background.
Set `size: (fill, auto)` to fill the width of a definite parent and fit the
content height. `raised: true` adds a shallow shadow to the surface. Use
`CaptionPlate` for a label that follows wrapped glyphs or set `plate: true`
on `SceneCaption` to give a timed takeaway the same treatment. For a long
takeaway, place it with `at` to reserve enough space for the wrapped lines.

`MetricNumber`, `ReadingPanel`, `NarrationStep`, and the older scene-layout
components remain available for imported projects. Use `Counter, role: metric`
for a readout and compose new layouts with `Panel`, `ChapterHeader`, and
`SceneCaption`. If a keyframe targets the former `MetricCard` `.plate` child,
target the card itself with `backdrop_color` or `shadow` instead.

Edit the content bindings near the top of `main.amx`, or override them for a
single-scene preview. For example:

```sh
animatix init --template math-explainer Lesson
animatix image Lesson/main.amx --time 4 --set 'heading="Cosine near zero"' -o frame.png
animatix storyboard Lesson/main.amx
```

Math Explainer accepts `heading`, `takeaway`, `formula`, `horizontal_domain`,
and `vertical_domain`. Edit `reference_fn` and `approximation_fn` to replace
the curves. An empty `formula` gives the graph the full width. Metric Brief accepts `heading`,
`takeaway`, `passing`, `latency`, and `reviews`. Narrated Explainer accepts
`heading` and `summary`. Each template lets you choose the scene style and
colorscheme in its `config`. Change the authored `canvas_size` to `(720, 1280)`
to use the stacked math and metric layouts. Check longer copy with a
storyboard and keep the headline and takeaway short enough for the format.

### Create a narrated project from a video-enabled package

Keep the extracted binaries and bundled media tools together. From a folder
outside the repository, run the following commands using the package's CLI.

```sh
animatix doctor
animatix init "My Narrated Project"
animatix check "My Narrated Project/main.amx"
animatix-gui "My Narrated Project/main.amx"
animatix video "My Narrated Project/main.amx" --hold 0 --output narrated.mp4
```

Use the paths to the extracted binaries if they are outside `PATH`. On Windows
the binaries have an `.exe` suffix. In the workbench, choose **New project… → Narrated Explainer** to create and open the starter. Choose a new or empty
directory. The starter uses `narration.mp3` and `narration.speech.json` beside
`main.amx`. The scene uses the transcript's cue markers to time the graphics and
displays captions from the same transcript. When moving an edited project, keep
the narration files currently referenced by the source. [Recording and sharing](narration.md#record-in-the-native-workbench)

Run `animatix-gui --doctor` from a terminal if the workbench cannot open a
preview or export. The report checks the renderer, text shaping, and the linked
FFmpeg AAC and H.264 encoders. The GUI report also checks preview audio
output separately; a missing playback device does not prevent export audio.
Animatix mixes and muxes audio in the export process. The Release build verifies the extracted macOS and Windows ZIPs
before uploading them, including the packaged workbench preview and an
11-second template export with audio. [Native video proof](../tools/release/native/video-proof.sh)

The native proof pins the vcpkg ports tree and its FFmpeg 8.x dependency set.
It builds against those headers, links the macOS libraries statically, and
stages the Windows DLLs beside both executables. Both stages carry the same
FFmpeg tools used to inspect the output. They include the codec source
archives, vcpkg recipes and licenses. The companion Linux source archive includes
the project's source and Cargo dependencies for the same release. The Windows
package statically links the Visual C++ runtime. On GitHub's
Windows runner, the acceptance process loads a matched DXC and DXIL pair from
the runner SDK to exercise its software adapter. The SDK DLLs remain in the
acceptance environment. Update the FFmpeg pin as a unit and verify both packages.

### Video export

CLI and GUI builds do **not** enable video by default. Enable it explicitly:

```sh
cargo build -p animatix-cli -p animatix-gui --features video
cargo run -p animatix-cli --features video -- video web/content/scenes/first-move.amx --fps 30 --output demo.mp4
cargo run -p animatix-cli --features video -- gif web/content/scenes/first-move.amx --fps 15 --output demo.gif
```

Run these inside `nix develop`: its `bindgenHook` generates bindings from the
matching FFmpeg headers. PNG/WebP export does not require `video`;
MP4/WebM/MOV/GIF does. See the CLI's `--help` for individual export options.

### Incremental chapter export

For a multi-scene composition, run the chapter export tool from the source
checkout with Python 3.10+, FFmpeg, FFprobe, and a video-enabled Animatix CLI.

```sh
python3 tools/content/chapter-export.py path/to/lesson.amx \
  --animatix target/debug/animatix --output path/to/lesson.mp4
```

Run the same command after editing the lesson. The tool reuses verified video
parts under `.animatix-chapters/` beside the source. A chapter starts on the
first full-export frame at or after its scene's scheduled start and includes
its incoming transition. Editing a scene or referenced media rebuilds the
parts that draw that scene, including neighboring transitions and carried
actors. Shared source and effective encoder settings can invalidate every part.

The tool assembles H.264 video in composition order and copies in one
continuous AAC mix from Animatix, preserving narration across chapter
boundaries. The resulting MP4 has navigation markers named after the scenes.
A failed export keeps the prior output and the verified parts
remain available on retry. Use `--cache-dir` to share a cache between runs or
put it on another disk. Use `--dependency PATH` for external render inputs
such as a font file installed on the host. Animatix resolves imported source,
images, audio and transcripts as part of its normal build.

## Repository files

Keep source, lockfiles, test inputs and reviewed visual baselines in Git.
`web/tools/fixtures/` contains shared benchmark and text-parity inputs. The retired
Transformer screenshots are no longer a regression suite; the README illustration
and its historical demo are preserved in [the illustration archive](assets/transformer-overview/README.md).
The general `animatix baseline` command still stores user-created baselines under
`snapshots/`; review them before committing or updating them.
Put temporary reports and render experiments under `target/` or in harness-managed temporary directories.
Local agent plans, environment files and generated SDK/site output are ignored.
Adding an ignore rule does not remove an already tracked file from Git.

## Headless Linux rendering

Use the same `animatix image`, `animatix storyboard` and `animatix video`
commands in a terminal or a container. Linux rendering prefers an available
hardware GPU and can use Mesa lavapipe on the CPU when there is no usable GPU.
The Nix development shell adds its lavapipe manifest to driver discovery,
rather than replacing explicitly configured or system drivers.

If you already have a compatible CLI, the small Nix runtime shell avoids
pulling in Rust or desktop tooling:

```sh
nix develop .#headless --command ./target/debug/animatix image scene.amx -o preview.png
```

For a portable Linux package, use the host driver and see the
[Linux distribution guide](../tools/release/native/linux/README.md).
`animatix check` validates the project without drawing it; for a cloud
rendering preflight, use `animatix doctor` or export a real frame.
Use the `GPU preview and rendering` row to diagnose still and preview
rendering; the encoder rows apply separately to video-enabled builds.

## Validate a change

`tools/ci/ci.sh` defines the project gates and generates CI's job matrix. Add new
checks there, rather than adding independent commands to workflow YAML.

```sh
tools/ci/ci.sh gates                       # gates and their required shells
tools/ci/ci.sh shell web-build             # shell for a particular gate
tools/ci/ci.sh gate fmt lint check test    # inside the default dev shell
bash tools/docs/check-docs.sh                # standalone documentation checks
```

Run formatting and the gates affected by your change before committing. A
single-crate check is not evidence that the GUI, analyzer, LSP or wasm build
passes. Public API doc comments are required by the affected crates' lint rules.

The `feature-contracts` gate stays in **Check & Lint**. It checks that the
minimal engine compiles without optional features, that its normal dependency
graph excludes rich text and GPU renderers, and that the default CLI and GUI
graphs exclude `rsmpeg`. Rich text enters through `animatix-text/rich-text` only.

The separate `default-native-build` gate links the default CLI and GUI binaries.
Selective CI runs it for native entry points, build scripts, linker inputs and
the export/video boundary. Manifests, lockfiles, packaging/toolchain changes,
uncertain plans and reusable release validation still run the full matrix.
Ordinary editor changes still run workspace checks and tests. Neither check
claims compilation in an environment with FFmpeg uninstalled.

| Change | Additional validation |
| --- | --- |
| Syntax or highlighting | Use the lossless tokenizer for lexical roles and the parser for contextual identifier and time-header ranges. Test the shared ranges, GUI, web UTF-16 offsets and LSP multiline tokens together. |
| Runtime, layout or rendering | Render representative frames and review a storyboard; run the relevant render and example gates. |
| Demo or site scene | Review composition, clipping, cue timing and the final held frame. Use `check`, `storyboard`, `audit` and regression baselines; see [scene audit](audit.md). |
| Site playback or fallback UI | Run `site-build` and `web-site-smoke` in their listed shells. Use `SMOKE_DEPTH=deep` for changes shared across work, lesson and technique pages. |
| Hot path | Record `tools/quality/perf-bench.sh compare` evidence. This is a manual measurement, not a PR gate; see [performance evaluation](performance_evaluation.md). |
| Documentation | The `doc-links` gate runs `tools/docs/check-docs.sh` for links, source paths and stale roadmap wording. The separate `doc` gate validates Rustdoc and generated property tables. |

The source editor colors an active draft from lexical tokens while semantic highlighting is
pending. The GUI and Playground coalesce ordinary typing for 120 ms; explicit native
completion queries still use the current draft. Playground cell selection follows the
compiled document's authored marker lines and clears while its displayed draft differs.
The native editor uses the analyzer’s header roles for cells and token colors. While analysis is pending, unchanged header lines retain their cells when the lexer still sees a source header. Search backgrounds and diagnostic underlines share the same projected GUI source ranges.

Public examples live in `web/content/scenes/` and have a Playground entry. Use temporary harness artifacts for
unfinished projects and human review; keep focused regression fixtures beside
their tests and actionable gaps in `docs/roadmap.md`. Historical experiments
remain available in Git. For embedded looping scenes, the held final frame
must remain a useful composition rather than an empty outro.

See [browser smoke contracts](web-smoke.md) for SDK fixtures, assembled-site
journeys, evidence and retry policy.

### CI selection and build experiments

`tools/ci/ci.sh` owns gates, stable job IDs and matrices. Pull-request selection
uses workspace path dependencies conservatively, including optional, build and
dev dependencies. Unknown inputs or unavailable base commits select every job;
release/reusable callers always use the full matrix. Main pushes may use the
same dependency selection only after a read-only Actions API proof establishes
that a first-parent ancestor passed the complete matrix in this same CI
workflow. The previous tip is tried first. If its run was selective, pending
or otherwise incomplete, the planner searches a bounded recent history for a
fully validated ancestor under the same CI policy. It then classifies the
entire net diff from that ancestor to the current head, not just the last push.
It does not infer coverage from a chain of selectively green runs. Force pushes,
policy changes, missing proof, unknown paths and API failures still fall back
to full coverage. Main always retains Web Build, producing a fresh same-run
SDK; `publish_web` only permits the existing main-push signer, never PR or
release/reusable runs. No cross-run SDK substitution is introduced.

The extension benchmark measures VM/property lookup and timeline evaluation.
Selective CI blocks on it for engine, shared engine input and extension package
changes; a post-evaluation renderer-only change still runs correctness and
render checks, not this timing threshold. Full/release validation still runs the
benchmark. The independent **Extension Benchmarks** workflow also records the
same health check on a schedule and by manual dispatch, without publishing a
package or blocking the native Nightly. Workspace correctness tests remain
part of ordinary code CI.

PR selection starts in
selective mode after the twelve representative shadow cases recorded in
`docs/history/maintenance-ci-shadow.json`. `target/ci/plan.json` records the
candidate/effective plans and intentional omissions; `target/ci/main-baseline.json`
records the proof or fallback reason. The job summary exposes both. Set the repository variable
`ANIMATIX_CI_SELECTION=shadow` to execute the complete matrix while reviewing
selection evidence. Keep `CI Required` as
the stable required check when migrating branch protection; it rejects failed,
cancelled or missing selected jobs.

Run `tools/ci/ci.sh plan --event pull_request --base <sha> --head <sha>` to inspect
a plan. Gates write exit-preserving measurements to `target/ci/timings.jsonl`;
`ANIMATIX_GATE_TIMINGS` can select another evidence path. For phase boundaries and
controlled local comparisons, see [CI measurement](ci-measurement.md). `doc-links` checks
handwritten documentation without compiling Rust. Its job keeps Nix, but skips
the large runner disk cleanup and Rust cache restore/save. Other preparation
remains unchanged. `doc` checks Rustdoc and the
generated property reference. The workspace test gate includes the native
`site_scenes` target; its standalone gate remains available for manual site builds.

The `test` gate builds the workspace once with Cargo's existing feature union,
then executes the resulting test harnesses. Engine and renderer harnesses run
serially; other harnesses use at most two CPU workers, with one libtest thread
per process. Every active test ID must appear in the results, and renderer
checks require an adapter through `ANIMATIX_REQUIRE_GPU=1`. Workspace doctests
run separately after the harnesses. The no-run build retains Rust example
compilation; the `examples` gate still checks the `.amx` catalogue. Cargo build
and doctest commands inherit the caller's incremental setting. Raw inventories,
plans and per-harness logs are kept in unique directories under
`target/ci/workspace-tests/`.

The one-off linker, runner, font-profile, contract-layout, property-reference
and CI-topology experiments are complete. Their dedicated scripts, workflow,
`.#experiments` shell, and Nextest configuration have been retired; the measured
decisions and evidence remain in [the maintenance report](history/maintenance-ci.md).
To rerun a preserved local harness, use a detached worktree at the final
implementation cut and the commands from that revision:

```sh
git worktree add --detach /tmp/animatix-maintenance-ci bd34463df14fc94b68a411d84285c0d7e0774f5d
cd /tmp/animatix-maintenance-ci
nix develop .#experiments --command tools/ci/ci.sh gate runner-experiment
```

The hosted CI-topology trials used source
`1152395a68715739d87df667051557f5ad53e5da`; at that revision, dispatch
`nightly.yml` with `ci-baseline`, `ci-grouped`, or `ci-shared-cli`. The archived
report records the source, run IDs, measurements and remaining reproduction
details. The production test gate uses Cargo/libtest and Cargo doctests; it does
not depend on Nextest. Engine and renderer harnesses run serially; remaining
harnesses use at most two CPU workers. Unique temporary paths do not isolate
process-global caches: engine tests using the text dependency see its production
cache even though the text crate's own unit tests use thread-local caches.

The engine's public integration suites remain independent modules inside the
`engine_contracts` target, sharing one process-wide font database. For example,
`cargo test -p animatix --test engine_contracts ir_tests::test_name` selects a
case from the former `ir_tests` target. Add new contract modules to
`tests/engine_contracts.rs`; a separate executable requires an explicit
`[[test]]` entry because this package sets `autotests = false`. Keep these tests
serial until their shared process-state assumptions have been checked. Source-size reports can be
reproduced with `python3 tools/quality/maintenance-inventory.py`; source attributes are
not substitutes for executable test-ID inventories.

### Property metadata and generated reference

Core property descriptors declare `AuthoredTypeHint` only where the authoring
type cannot be derived from `PropertyValueKind`. Syntax maps those hints to its
own types; the engine continues to own runtime bindings. Adding a property does
not require another name-based syntax type branch. Serialized `PropertyId` rows
remain append-only.

`PropertyDescriptor::new(name, applicable, value_kind)` is unchanged. External
struct literals must supply `authored_type: AuthoredTypeHint::Default`, or migrate
to the constructor and its const `with_authored_type` builder. This source-level
API migration does not change accepted DSL types or serialized IDs.

Regenerate the reference with
`cargo run -p animatix --example property_reference -- --write`, then verify with
`--check`. The docs-owned grouping array contains presentation categories only;
input types, applicability and runtime flags come from the existing schemas.
Keep explanations and alias rows outside generated markers. An authoring `Any`
is not narrowed by the separately displayed runtime representation.

### Interface catalogs

The GUI embeds Fluent catalogs from `crates/animatix-i18n/locales`. Astro reads
its interface catalogs at build time. The site and standalone player use
`web/embed/src/locales` for player controls and graphics messages that have
identical meanings in both. The site sends formatted labels to browser scripts
and includes Fluent patterns for messages that take values available only in
the browser.

Use stable IDs for messages and attributes, and pass authored names, timecodes
and source positions as values. Keep the canonical text on compiler diagnostics
for build reports and protocol consumers. Mark a new localized explanation at
the producer so distinct messages can still share a diagnostic code.
`npm --prefix web/app test` checks catalog syntax, duplicates, keys, argument
and attribute parity, and English fallback for both frontends. The same suite
runs in the `site-build` gate.

Use this coverage map when reviewing a locale change.

| Surface | Translate at display | Preserve as source or detail |
| --- | --- | --- |
| Parser diagnostics | Expected/found wording, grammar labels, rule explanations | AMX tokens, authored names, coordinates and the canonical CLI/LSP diagnostic |
| File and export | Action messages, format controls, progress and failure summaries | Paths, format names, numeric timecodes, backend and OS error details |
| Narration | Recording and history controls, alignment, binding instructions, worker summaries | Script text, voice IDs, historical issue text, subprocess stderr and provider responses |
| Site and stand-alone player | Navigation, controls and playback errors | AMX examples, URLs, SDK event data, unclassified legacy error details |

`python3 tools/quality/localization-audit.py` scans known display calls for
literal copy and lists its explicit exclusions. Review generated values and
diagnostic producers separately when adding a display path.

### Environment boundaries

Enter each shell from a clean terminal, not from another dev shell or an active
direnv environment. Nested shells can keep the wrong Rust toolchain first on
`PATH`; the gate runner checks shell identity but does not enter shells for you.

| Environment | Purpose |
| --- | --- |
| `nix develop` | Native Linux gates and site assembly |
| `nix develop .#web-build` | wasm checks |
| `nix develop .#web-ci` | Web SDK builds and browser tests |
| `nix develop .#dist-driver` | Orchestrate the Docker Linux distribution build |
| Native macOS/Windows toolchains | OS-specific eparts and distribution gates |

The distribution shell requires a running Docker daemon; Nix does not supply
one. Linux release binaries use the pinned Ubuntu build container, not the Nix
runtime libraries. macOS/Windows use native SDKs and a floating rustup stable
channel. Cargo/npm downloads, advisory data and documented container inputs are
separate reproducibility boundaries; these shells do not make every build hermetic.

### Test fixtures and structure

Reuse `crates/animatix/src/timeline/tests/support.rs` rather than constructing
parallel fixtures. `build` returns the report, `build_output` returns its output
without judging diagnostics, `build_timeline` rejects errors, and `build_clean`
allows only `never-revealed` hints. The module also provides AST builders,
property readers and render-path assertions. GUI tests use
`crate::app::test_support` store constructors.

The `structure` gate enforces the [normative ECS structure contract](ecs.md#8-structure-gate).
Its limits and function/file allowlists are documented there. Treat allowlists
as ratchets: split oversized code and remove its entry, rather than raising limits.

## Find the right code

See [Architecture](architecture.md) for crate boundaries and [ECS](ecs.md) for
runtime storage invariants. Common starting points:

| Area | Source |
| --- | --- |
| CLI | `crates/animatix-cli/src/main.rs` |
| Parser, AST and typing | `crates/animatix-syntax/src/` |
| Build and evaluation | `crates/animatix/src/timeline/` |
| Rendering and encoders | `crates/animatix-render/src/` |
| GUI shell and panels | `crates/animatix-gui/src/app/` |
| Language intelligence | `crates/animatix-analyzer/src/` |
| LSP adapter | `crates/animatix-lsp/src/` |

### GUI contract

The [GUI workbench](gui-workbench.md) is code-first. Source edits rebuild the
scene; preview selection, timeline review and the Inspector do not write scene
properties back to source. Read-only panels can navigate to a direct,
unambiguous source location. Failed builds retain the last successful preview
with a stale/error state; stale export is blocked.

Keep source, derived build state and workspace view state separate. The unused
semantic AST-edit API has been retired. Any future timing-writeback design must
have revision checks and a visible, undoable source patch.

### Analyzer and LSP

`animatix-syntax` provides the parser and AST. `animatix-analyzer` supplies shared
language intelligence to the GUI and LSP. Keep core analysis independent of I/O.
`parse_source_complete` is the canonical tokenization/parse pipeline; reuse its
returned token stream rather than tokenizing again for positions.

Build the stdio language server with:

```sh
cargo build -p animatix-lsp
```

Configure your editor's LSP client to launch the absolute path to
`target/debug/animatix-lsp`, associate it with `.amx` files, and use the project
root as its workspace. An editor needs an LSP client or extension; this repository
does not supply a universal VS Code setting or a built-in nvim-lspconfig entry.

## Web distribution

The web player and on-demand capabilities ship together as a generated SDK in
`web/runtime/`. That directory is absent from a clean checkout.

```sh
# From a clean terminal, not another dev shell:
nix develop .#web-ci --command tools/ci/ci.sh gate web-build web-sdk-integrity web-transactions
```

Staging, restore, site assembly and packaging verify the manifest against the
source identity and workspace version. Do not copy only the player script or one
capability file. Pages uses the verified CI SDK artifact when available, with a
source-build fallback. See [Web development](../web/README.md) for the SDK and site
workflows.

## Commits and releases

Use ordinary `git commit` after staging only the intended changes.
[Conventional Commits](https://www.conventionalcommits.org/) are recommended,
not required; CI does not reject a change for its commit-message format.

```sh
git add <files>
git commit -m "feat(gui): add timeline scrubbing"
```

### Prepare a versioned release

Choose the version and tag explicitly. Never infer a bump from commit messages.
Start from clean `main`; the first release may keep the current workspace version
if that is the chosen version. The following uses `0.1.0` as an example, not a
request to release it:

```sh
# Optional draft, in nix develop. Add --base-tag v<previous-version> if needed.
python3 tools/release/release.py draft 0.1.0 --output /tmp/animatix-v0.1.0-draft.md
# Review and rewrite the draft, then save the final notes as docs/releases/v0.1.0.md.
# Only if the chosen version differs from the workspace version:
tools/release/bump-version.sh 0.1.0
# Run the relevant validation gates, including meta-version, before committing.
git add Cargo.toml Cargo.lock docs/releases/v0.1.0.md
git commit -m "Prepare release v0.1.0"
```

The optional draft command uses `git-cliff`. Its current template is a flat,
oldest-first list, not automatic grouping by change type; ordinary non-Conventional
commit messages are retained. It does not commit, tag, push or
publish. Without `--base-tag`, it starts after the nearest reachable SemVer tag,
including prereleases; select the previous stable tag explicitly when preparing
a stable release from release candidates. With no such tag it includes all history.
Nightly and Cloudflare deployment tags are never release boundaries. Final notes in `docs/releases/` files named `vX.Y.Z.md` are human-editable and are the
canonical release description. An optional `CHANGELOG.md` may summarize releases,
but is not required and is not the publication source. Explain user-facing changes,
compatibility and limitations instead of dumping a commit log.

Review the exact release commit and obtain authorization before tagging/publishing:

```sh
git tag -a v0.1.0 <exact-reviewed-commit> -F docs/releases/v0.1.0.md
python3 tools/release/release.py resolve v0.1.0
# Only when publishing this commit and tag is authorized:
git push origin main
git push origin v0.1.0
```

The resolver checks the committed version and release notes for the selected tag.
A tag push starts the versioned release workflow. Manual workflow dispatch builds
an existing, explicitly selected tag; publication is off by default and requires
`publish: true`. Preparation, tagging and publication are separate decisions:
never auto-bump, auto-push or publish merely because a draft or build succeeded.
Publication uses `gh release create --verify-tag`: the remote tag must already
exist. An existing release causes a failure, not an edit or asset overwrite.
Investigate that failure rather than silently replacing a published release.

The scheduled/manual nightly workflow builds a Linux video-enabled distribution
and replaces the rolling `nightly` prerelease. Scheduled runs skip unchanged
`main`; the workflow's force input can request a rebuild. It does not create a
new versioned release each day.

Release runs the complete Windows and macOS video acceptance as part of the
distribution gates. The **Native Video Proof** workflow provides a manual
rehearsal of the native archives and Linux Nightly packaging.

To warm the Windows and macOS vcpkg binary caches before tagging, run
**Native Video Proof** on the default branch with `ffmpeg_only` enabled.
Release tags can restore that branch's cache. vcpkg checks each package's ABI
and builds missing packages from the pinned ports. The cache holds the binary
package archives and the codec source archives that go into the release ZIP.
The build verifies the source checksums and downloads missing archives from
the pinned URLs. To check the warm path on a fresh runner,
enable `require_ffmpeg_cache`. Leave `ffmpeg_only` disabled for a warm-path run
that also builds Animatix and validates the extracted native ZIPs.

Detailed explanations, measurements and post-mortems belong in
[history](history/README.md); [roadmap](roadmap.md) contains unfinished work only.
For Linux binary/source archive identity and rebuild instructions, see the
[packaging guide](../tools/release/native/linux/README.md).

## Engine change checklists

### Adding a new expression semantic

Every new expression form (operator, literal shape, call convention) must be
implemented in **all** of the following places. The compiler's non-exhaustive
match errors catch the typed arms, but the *semantic* ones (recursion in the
two visitor methods, lowering parity) are silent if missed:

1. **AST variant** — `crates/animatix-syntax/src/ast/` (`Expr` enum);
   extend `references_ident` in the same file (plot dynamic-gating and
   frame-env injection depend on it).
2. **Parser** — `crates/animatix-syntax/src/parser/expr.rs`.
3. **IR lowering** — `crates/animatix/src/timeline/modifier_runtime/ir/lower.rs`
   (`compile_expr`).
4. **IR types** — `.../ir/types.rs`: a `CompiledExpr` variant plus **both**
   visitors for a new IR variant: `collect_prop_refs` and `references_ident`.
   A literal such as `hex_color("fff1d6")` can lower to an existing
   `CompiledExpr::Const`.
5. **IR evaluation** — `.../ir/eval.rs` (`evaluate_compiled_expr`) and
   `.../ir/display.rs` (debug rendering) for new IR variants.
6. **Unified build-time evaluation** — `crates/animatix/src/timeline/utils/`
   (`evaluate_expr_inner`) automatically delegates to `compile_expr` and
   `evaluate_compiled_expr`, sharing the same IR VM.
7. **Cache key** — if the form reads mutable environment state, check whether
   `utils::evaluate_expr`'s cache key (`expr_hash`, `stamp`, `let_epoch`)
   needs a new component.
8. **Round-trip & tests** — `crates/animatix/src/timeline/value_parser.rs`
   (`value_to_expr`) and test cases in `crates/animatix/tests/ir_tests.rs`.

Precedents to copy: `LetChain` (block bodies, 13 touch points),
`PropRef` (references, 9), `MakeList` (list/tuple split, 7).

### Adding an ECS component

The timeline stores per-entity data as components indexed by `EntityId`, not as
fields on a monolithic track. `docs/ecs.md` is the normative design. The
touch-point checklist is:

1. **Component type** — `crates/animatix/src/timeline/components/<domain>.rs`
   (`Clone`, `Debug`; `Serialize/Deserialize` behind `feature = "serde"` when
   it belongs in a `CarryBag` snapshot).
2. **Store** — add a `RequiredStore<C>`/`ComponentStore<C>` to `World`, and
   populate/resize it in `World::spawn`. Required means every entity has it.
3. **Accessor** — add `EntityRef::<domain>()` and, when writable,
   `EntityMut::<domain>_mut()`. Do not let call sites touch the store directly.
4. **Delete the old field/table** in the same change, and route every call site
   through the accessor. `rustc` is the checklist; a dual write is a bug.
5. **Snapshot** — follow [the ECS snapshot checklist](ecs.md#7-adding-a-component)
   for the per-tier snapshot helper, carry injection, and round-trip test.
6. **Tests** — component presence, snapshot round-trip, and frame-cache
   invalidation on writes through `Timeline`/`EntityMut`.
7. **Docs** — update `docs/ecs.md` §2 and `docs/architecture.md` §2. A
   label-keyed resource instead of a component needs a one-line reason in
   `docs/ecs.md` §2.3.
