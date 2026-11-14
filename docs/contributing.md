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
Tree-sitter and Cocogitto (`cog`). Workspace builds need ALSA on Linux even when
you only intend to test the engine, because the GUI depends on audio playback.
A missing ALSA or FFmpeg `pkg-config` result usually means the development shell
is not active; changing Cargo dependencies is not the fix.

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
in the [packaging guide](../packaging/linux/README.md). On a host without those
paths/libraries, use the source-build instructions above instead.

The current packaging scripts build a different, relocatable distribution. A
successful packaging check does not establish that an older uploaded release was
built with those scripts. Recheck this caveat when replacing the nightly artifact.

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

## Validate a change

`scripts/ci.sh` defines the project gates and generates CI's job matrix. Add new
checks there, rather than adding independent commands to workflow YAML.

```sh
scripts/ci.sh gates                       # gates and their required shells
scripts/ci.sh shell web-build             # shell for a particular gate
scripts/ci.sh gate fmt lint check test    # inside the default dev shell
bash scripts/check-docs.sh                # standalone documentation checks
```

Run formatting and the gates affected by your change before committing. A
single-crate check is not evidence that the GUI, analyzer, LSP or wasm build
passes. Public API doc comments are required by the affected crates' lint rules.

| Change | Additional validation |
| --- | --- |
| Syntax or highlighting | Update the lossless tokenizer in `crates/animatix-syntax/src/token/`; test syntax, analyzer, GUI and LSP together. |
| Runtime, layout or rendering | Render representative frames and review a storyboard; run the relevant render and example gates. |
| Demo or site scene | Review composition, clipping, cue timing and the final held frame. Use `check`, `storyboard`, `audit` and regression baselines; see [scene audit](audit.md). |
| Hot path | Record `scripts/perf-bench.sh compare` evidence. This is a manual measurement, not a PR gate; see [performance evaluation](performance_evaluation.md). |
| Documentation | Run `scripts/check-docs.sh`: relative links, source paths, reference consistency and stale roadmap wording. The `doc` gate includes documentation validation. |

Runnable examples belong in `examples/`. Keep unfinished projects and probes in
`dogfood/`, not in the supported example set. For embedded looping scenes, the
held final frame must remain a useful composition rather than an empty outro.

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

The `structure` gate enforces the [ECS structure contract](ecs.md).
Its thresholds and allowlists live in `scripts/structure.sh`. Treat allowlists
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

Keep source, derived build state and workspace view state separate. Existing
`SourceEdit` machinery is not permission to reintroduce visual actor creation,
canvas transforms or Inspector editing. Any future timing-writeback design must
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
nix develop .#web-ci --command scripts/ci.sh gate web-build web-sdk-integrity web-transactions
```

Staging, restore, site assembly and packaging verify the manifest against the
source identity and workspace version. Do not copy only the player script or one
capability file. Pages uses the verified CI SDK artifact when available, with a
source-build fallback. See [Web development](../web/README.md) for the SDK and site
workflows.

## Commits and releases

Use [Conventional Commits](https://www.conventionalcommits.org/). Stage only the
intended changes, then run Cocogitto inside `nix develop`:

```sh
cog commit feat "add timeline scrubbing" gui
cog commit docs "clarify video export setup" docs
cog check
```

Choose a scope from `cog.toml` (for example `gui`, `syntax`, `web` or `docs`).
CI validates commit messages.

For an authorized versioned release, start from clean `main`:

```sh
cog bump --auto --skip-ci
# Review the generated version, changelog, commit and tag before publishing.
git push origin main
git push origin v<version>
```

The pre-bump hook runs `scripts/bump-version.sh`, updates the workspace version
and lockfile, and checks `meta-version`. Pushing the tag triggers the release
workflow: PR gates first, then site, SDK and native distribution artifacts.
Publishing remains a separate decision from preparing the local tag.

The scheduled/manual nightly workflow builds a Linux video-enabled distribution
and replaces the rolling `nightly` prerelease. Scheduled runs skip unchanged
`main`; the workflow's force input can request a rebuild. It does not create a
new versioned release each day.

`CHANGELOG.md` is generated by Cocogitto; do not edit it by hand. Explanations,
measurements and post-mortems belong in [history](history/README.md), while
[roadmap](roadmap.md) contains unfinished work only. For the Linux binary/source
archive identity and rebuild recipe, use the [packaging guide](../packaging/linux/README.md).

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
5. **Snapshot** — extend `snapshot_entity_at` / `restore_entity` and the
   `CarryBag` round-trip test.
6. **Tests** — component presence, snapshot round-trip, and
   `frame_cache_invalidates_on_component_write`.
7. **Docs** — update `docs/ecs.md` §2 and `docs/architecture.md` §2. A
   label-keyed resource instead of a component needs a one-line reason in
   `docs/ecs.md` §2.3.
