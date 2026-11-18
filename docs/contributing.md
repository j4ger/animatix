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
cargo run --bin animatix-gui -- examples/basics/00_hello.amx
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
cargo run --bin animatix -- ast examples/basics/00_hello.amx --compact
cargo run --bin animatix -- check examples/basics/00_hello.amx
cargo run --bin animatix -- render examples/basics/00_hello.amx --loop
cargo run --bin animatix -- image examples/basics/00_hello.amx --time 1.5 --output frame.png
```

### Downloaded builds

[Releases](https://github.com/j4ger/animatix/releases) list the available artifacts.
Do not infer portability from an archive name: the inspected November 12, 2026
`nightly` archive contains bare CLI/GUI binaries that reference a Nix store loader
and external FFmpeg libraries. It is not the self-contained Linux package described
in the [packaging guide](../tools/release/native/linux/README.md). On a host without those
paths/libraries, use the source-build instructions above instead.

The current packaging scripts build a different, relocatable distribution. A
successful packaging check does not establish that an older uploaded release was
built with those scripts. Recheck this caveat when replacing the nightly artifact.

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
the binaries have an `.exe` suffix. In the workbench, **New narrated project…**
in the file menu also creates and opens the starter. Choose a new or empty
directory. Keep `narration.mp3` and `narration.speech.json` beside `main.amx`
when moving the project. The scene uses the transcript's cue markers to time
the graphics and displays captions from the same transcript.

Run `animatix-gui --doctor` from a terminal if the workbench cannot open a
preview or export. The report checks the renderer, text shaping, and the linked
FFmpeg AAC and H.264 encoders. Animatix mixes and muxes audio in the export
process. The Release build verifies the extracted macOS and Windows ZIPs
before uploading them, including the packaged workbench preview and a
30-second export with audio. [Native video proof](../tools/release/native/video-proof.sh)

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
cargo run -p animatix-cli --features video -- video examples/basics/00_hello.amx --fps 30 --output demo.mp4
cargo run -p animatix-cli --features video -- gif examples/basics/00_hello.amx --fps 15 --output demo.gif
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

The `no-video` gate also checks the minimal engine's normal dependency graph:
disabling all optional features must exclude Typst and syntax-highlighting
dependencies. Rich text enters through `animatix-text/rich-text` only.

| Change | Additional validation |
| --- | --- |
| Syntax or highlighting | Update the lossless tokenizer in `crates/animatix-syntax/src/token/`; test syntax, analyzer, GUI and LSP together. |
| Runtime, layout or rendering | Render representative frames and review a storyboard; run the relevant render and example gates. |
| Demo or site scene | Review composition, clipping, cue timing and the final held frame. Use `check`, `storyboard`, `audit` and regression baselines; see [scene audit](audit.md). |
| Hot path | Record `tools/quality/perf-bench.sh compare` evidence. This is a manual measurement, not a PR gate; see [performance evaluation](performance_evaluation.md). |
| Documentation | The `doc-links` gate runs `tools/docs/check-docs.sh` for links, source paths and stale roadmap wording. The separate `doc` gate validates Rustdoc and generated property tables. |

Runnable examples belong in `examples/`. Use temporary harness artifacts for
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
that the previous tip passed the complete matrix in this same CI workflow.
The previous tip must be an ancestor, and CI policy files must be unchanged.
Force pushes, missing/incomplete/failed/cancelled evidence, unknown paths and
API failures fall back to full coverage. A selective previous run is not used
as a full baseline: this first phase deliberately does not infer a transitive
chain of coverage. Main always retains Web Build, producing a fresh same-run
SDK; `publish_web` only permits the existing main-push signer, never PR or
release/reusable runs. No cross-run SDK substitution is introduced.

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
   visitors: `collect_prop_refs` and `references_ident`.
5. **IR evaluation** — `.../ir/eval.rs` (`evaluate_compiled_expr`) and
   `.../ir/display.rs` (debug rendering).
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
