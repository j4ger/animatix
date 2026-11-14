# Contributing to Animatix

## Quick Start

```bash
cargo build
cargo test
```

If you use Nix, `nix develop` sets up Rust, FFmpeg, Tree-sitter, graphics
dependencies, and **Cocogitto** (`cog`, used for conventional commits). Run
`cog commit`/`cog check` inside the dev shell; outside it `cog` is not on `PATH`.

Linux video-enabled release builds use `scripts/dist-linux.sh` and the pinned
Ubuntu container. A release includes a separate corresponding-source archive
with the exact allowlisted dirty source snapshot, `Cargo.lock`, vendored Rust
dependencies, offline Cargo configuration, pinned media source archives, and
the rebuild recipe. See [the Linux packaging guide](../packaging/linux/README.md)
for its identity, validation, and rebuild steps.

The source archive records a content identity for the allowlisted project files,
including tracked edits and non-ignored untracked files. Its vendored Cargo tree
and generated offline Cargo configuration have separate digests, checked before
metadata/build and again when verifying the archive. The archive includes a
`REBUILD.md` command and the pinned media source archives; use the documented
container recipe when checking a delivered binary rather than rebuilding from
the checkout that produced it.

---

## Development Workflows

### CLI Commands

```bash
# Inspect parsed AST
cargo run --bin animatix -- ast examples/projects/fft_explain.amx
cargo run --bin animatix -- ast examples/projects/fft_explain.amx --compact

# Live preview
cargo run --bin animatix -- render examples/projects/fft_explain.amx
cargo run --bin animatix -- render examples/projects/fft_explain.amx --loop

# Frame export
cargo run --bin animatix -- image examples/projects/fft_explain.amx --time 1.5 --output frame.png

# Video/GIF export (requires the `video` feature, see AGENTS.md)
cargo run -p animatix-cli --features video -- video examples/projects/fft_explain.amx --fps 30 --duration 5 --output demo.mp4
cargo run -p animatix-cli --features video -- gif examples/projects/fft_explain.amx --fps 15 --duration 5 --output out.gif

# GUI
cargo run --bin animatix-gui -- examples/projects/fft_explain.amx
```

### CI selection and build experiments

`scripts/ci.sh` owns gates, stable job IDs and matrices. Pull-request selection
uses workspace path dependencies conservatively, including optional, build and
dev dependencies. Unknown inputs or unavailable base commits select every job;
main pushes and release callers always use the full matrix. Selection starts in
selective mode after the twelve representative shadow cases recorded in
`docs/history/maintenance-ci-shadow.json`. `target/ci/plan.json` records the
candidate and effective plans. Set the repository variable
`ANIMATIX_CI_SELECTION=shadow` to execute the complete matrix while reviewing
selection evidence. Keep `CI Required` as
the stable required check when migrating branch protection; it rejects failed,
cancelled or missing selected jobs.

Run `scripts/ci.sh plan --event pull_request --base <sha> --head <sha>` to inspect
a plan. Gates write exit-preserving measurements to `target/ci/timings.jsonl`;
`ANIMATIX_GATE_TIMINGS` can select another evidence path. `doc-links` checks
handwritten documentation without compiling Rust; `doc` checks Rustdoc and the
generated property reference. The workspace test gate includes the native
`site_scenes` target; its standalone gate remains available for manual site builds.

Build experiments are manual and never part of default PR/release/all runs:

```bash
nix develop .#experiments --command scripts/ci.sh gate linker-experiment
nix develop .#experiments --command scripts/ci.sh gate runner-experiment
scripts/ci.sh --matrix experiment
```

The experiment shell supplies mold, LLD and nextest pinned by `flake.lock` without
changing normal linker/runner defaults. Candidates use owned build directories
and record their source, tools, commands, failures, wall time and process-tree
memory. Run experiments serially on fixed source; do not edit source or run
competing benchmarks during a measurement. Cold builds and warm execution are
separate populations. Five paired warm rounds and three cold rounds are the
default; failed candidates remain failed evidence, never a silent fallback.

Hosted manual trials set dev/test debug information to zero so independent
candidate targets fit their disks; macOS application trials keep the `dist`
profile. They record those overrides and are not evidence about the default
debug profile. Local trials can retain the default profile on a larger disk.
For the reference check comparison, pass a pre-generator commit through
`ANIMATIX_REFERENCE_BASE_SHA` and run the named
`property-reference-experiment` gate in the experiment shell.

GPU tests belong in `animatix-render` or must be explicitly included in the
nextest GPU group before enabling parallel execution. Nextest does not replace
Cargo doctests or example compilation. Unique temporary paths do not isolate
process-global caches: engine tests using the text dependency see its production
cache even though the text crate's own unit tests use thread-local caches.

The engine's public integration suites remain independent modules inside the
`engine_contracts` target, sharing one process-wide font database. For example,
`cargo test -p animatix --test engine_contracts ir_tests::test_name` selects a
case from the former `ir_tests` target. Keep these tests serial until their
shared process-state assumptions have been checked. Source-size reports can be
reproduced with `python3 scripts/maintenance-inventory.py`; source attributes are
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

### Tokenizer/Tooling Validation

For syntax or highlighting changes, run the syntax, analyzer, GUI, and LSP tests after updating `crates/animatix-syntax/src/token/`.

### Recommended Validation Loop

For parser/language changes:
```bash
cargo run -- ast path/to/scene.amx
cargo test
```

For runtime/layout/rendering changes:
```bash
cargo run -- image path/to/scene.amx --time 0.0 --output /tmp/frame0.png
cargo run -- image path/to/scene.amx --time 1.5 --output /tmp/frame1.png
# Video export requires the `video` feature (see AGENTS.md)
cargo run -p animatix-cli --features video -- video path/to/scene.amx --output /tmp/check.mp4 --fps 30
cargo test
```

---

## Project Structure

```
crates/
├── animatix-core/         # Shared vocabulary (capability enums, effect contract, icons, RenderError)
├── animatix-std/          # Built-in catalog (effect definitions, primitive identity cards)
├── animatix-syntax/       # Syntax layer (parser, AST, module system, typing)
├── animatix-text/         # Typst/fontdb text compilation service
├── animatix/              # Runtime engine (timeline, build/evaluate, primitives)
├── animatix-render/       # GPU presentation (Vello/wgpu, filters, encoders)
├── animatix-analyzer/     # Shared language intelligence
├── animatix-lsp/          # LSP server (tower-lsp)
├── animatix-gui/          # Desktop GUI (eframe/egui)
└── eparts/                # Themed egui widget framework
```

### Source Areas Worth Knowing

- `crates/animatix-cli/src/main.rs` — CLI entrypoint
- `crates/animatix-syntax/src/parser/` — Chumsky parser (split into submodules)
- `crates/animatix-syntax/src/ast/` — AST types
- `crates/animatix-syntax/src/typing/` — shared `Type`/`TypeEnv` inference
- `crates/animatix-syntax/src/walk.rs` — shared AST traversal primitives
- `crates/animatix/src/timeline/` — keyframed runtime, actions, morphing, plotting
- `crates/animatix-render/src/` — Vello/WGPU rendering backend (renderer core, filter backend, encoders)
- `crates/animatix/src/primitives/` — actor primitive system
- `crates/animatix-gui/src/app/mod.rs` — GUI shell state and event loop
- `crates/animatix-gui/src/app/panels/` — UI panels (inspector, timeline, sidebar, preview, editor)
- `crates/animatix-gui/src/app/commands/mod.rs` — command system (ShellAction / Command / ViewAction)
- `crates/animatix-gui/src/app/design_tokens/mod.rs` — GUI design token system
- `crates/animatix-syntax/src/token/` — single lossless tokenizer
- `crates/animatix-gui/src/fonts.rs` — egui font setup, including system CJK fallback

---

## GUI Data Flow

```
Editor → DocumentSession.set_source_text() → debounce → rebuild → Timeline
Timeline → frame_cache → PreviewSurface.render() → egui texture → Preview pane
Timeline.tracks → Inspector (read properties at current time)
Preview click → hit_regions → selected_actor → Inspector highlights
Preview drag → PropertyEdit → handle_property_edit → source + timeline + invalidate
```

Special-case edits (e.g. `child_order` on containers) bypass per-track dispatch and are handled directly in `handle_property_edit` / `handle_keyframe_edit` before the generic pipeline:
```
PropertyEdit { actor: container, property: "child_order", value: StringList }
  → resolve labels to EntityIds, update ContainerMetadata.child_order + rebuild layout_children
  → AST mutation: ReorderContainerChildren → reorder children block in source
```

### Key State Objects

- **DocumentSession**: file path, source text, dirty state, compiled AST, timeline, duration, scene dimensions
- **PreviewPaneState**: current time, playback state, dimensions, status/error
- **GuiShell**: document, editor, preview, dock state, hot reloader, selected actor, hit regions

---

## Analyzer Architecture

Language intelligence is shared between the GUI and LSP via `animatix-analyzer`:

```
animatix (core parser/AST)
    ↓
animatix-analyzer (pure computation, no I/O)
    ↓
animatix-gui (direct calls)    animatix-lsp (tower-lsp, JSON-RPC)
```

### Design Principles

1. **No I/O** — all functions take `&str` or `&[Stmt]`, return data
2. **Position-based API** — `(line, col)` matching LSP's `Position`
3. **Incremental** — `Analyzer::update()` re-parses only when source changes
4. **Canonical parser**: `parse_source_complete` is the single tokenize+parse pipeline used by module loading and the analyzer; `parse_source` and `parse_source_with_occurrences` are the narrower views of the same parse. The analyzer keeps the returned token stream plus AST for positions. Do not tokenize a second time in callers.

### LSP Capabilities

| Feature | LSP Method | Status |
|---------|------------|--------|
| Completions | `textDocument/completion` | ✅ |
| Hover | `textDocument/hover` | ✅ |
| Go-to-definition | `textDocument/definition` | ✅ |
| Document symbols | `textDocument/documentSymbol` | ✅ |
| Diagnostics | Published on change | ✅ |

### Editor Configuration

**VS Code** (`.vscode/settings.json`):
```json
{
  "amx.languageServer": {
    "command": "animatix-lsp",
    "args": []
  }
}
```

**Neovim** (nvim-lspconfig):
```lua
require('lspconfig').animatix.setup {
  cmd = { 'animatix-lsp' },
  filetypes = { 'amx' },
  root_dir = require('lspconfig').util.root_pattern('.git', 'Cargo.toml'),
}
```

---

## Error Model

| Class | Example | UI Treatment |
|-------|---------|--------------|
| File load | Missing file | Red banner in preview |
| Parse | Syntax error | Red squiggles in editor |
| Semantic | Unknown action/label | Yellow squiggles |
| Info | Unknown property for type | Blue squiggles |
| Build | Timeline build failure | Amber banner |
| Render | GPU error | Red overlay on preview |

### Diagnostic Sources

1. **Tree-sitter** — syntax errors (ERROR/MISSING nodes)
2. **Chumsky** — parse errors (detailed messages)
3. **Semantic checks** — unknown actions, undefined labels, unknown types/properties

---

## Testing

```bash
scripts/ci.sh gate fmt lint check test doc doc-links   # inside nix develop
scripts/ci.sh gates                      # every gate and the shell each needs
```

`scripts/ci.sh` is the single definition of "is this green": every check is a named
gate there, PR CI generates its job matrix from that file, and `release.yml` runs
the same gates as its release blocker. Adding a check means adding a gate — not
editing a workflow. `AGENTS.md` (Workflow 3) records the gate discipline; run
`scripts/ci.sh gates` to see the shell each gate needs, and see that script's note on why
running one in the wrong shell looks like a compile error.

Performance comparison is deliberately **not** one of those gates:
`scripts/perf-bench.sh compare` is manual evidence for hot-path changes, not a
PR check, and it exits 2 on a loaded machine instead of reporting a verdict. See
`docs/performance_evaluation.md` §4.1 for the baseline and interpretation
procedure.

- **animatix**: Core timeline, parser, and rendering tests
- **animatix-analyzer**: 48+ unit tests (symbol extraction, type inference, completions, diagnostics)
- **animatix-lsp**: Compiles, manual testing with editors
- **animatix-gui**: Integrated with workspace tests

**Use the shared fixtures; do not re-derive them in the test module.**
`crates/animatix/src/timeline/tests/support.rs` is the single home for timeline
fixture construction and reads (`parse`, `build`, `build_output`, `build_ast`,
`build_timeline`, `build_clean`, `build_ignoring_warnings`, `assert_clean`,
`render_paths`, `assert_finite_samples`, `baked_paths`, `path_bounds`,
`text_at`, the `property_value`/`f32_property`/`vec2_property`/
`property_or_default` readers, and the `actor`/`keyframe`/`prop`/`vec2` AST
builders); `timeline/tests/build/support.rs` re-exports them for that
submodule. Pick the helper whose assertion matches what the test checks:
`build` hands back the `BuildReport` (read `.output` for the timeline,
`.diagnostics` for the diagnostics), `build_output` returns it unjudged,
`build_timeline` asserts no error-level diagnostics, `build_clean` asserts
nothing beyond `never-revealed` hints. GUI test modules use
`crate::app::test_support` (`document_store_empty`, `make_document_store`,
`document_store_loaded`, `preview_store`, `ui_store`, `workspace_store`)
instead of hand-rolling store prologues.

Doc comments on public items are load-bearing: `animatix`, `animatix-syntax`
and `animatix-analyzer` set `#![warn(missing_docs)]` and `gate lint` runs with
`-D warnings`, so deleting one fails the build rather than merely losing
information.

`scripts/ci.sh gate structure` enforces the structure contract.
The thresholds and allowlists live in [`scripts/structure.sh`](../scripts/structure.sh)
(with the contract in `docs/ecs.md` §8 and the short rule in `AGENTS.md`); they
are not restated here so the two cannot drift. Treat both allowlists as
ratchets: split the function or file, then delete its row; never raise a row or
add one for new code without justifying it.

For demo work: keep runnable demos under `examples/`, verify with both `ast` and `image`/`video`. Use `dogfood/` for in-progress real-content projects and grammar probes; do not move known-broken probes into `examples/`.

---

## Releasing

One command on `main`, with a clean tree:

```bash
cog bump --auto --skip-ci
git push && git push origin v<version>
```

`cog bump` collects the commits since the last tag into `CHANGELOG.md`, commits
that, and tags the commit. The version itself comes from
`scripts/bump-version.sh`, wired as cog's `pre_bump_hook`: it rewrites the single
`[workspace.package]` line, lets `cargo metadata` refresh `Cargo.lock`, and runs the
`meta-version` gate, so a release cannot half-bump. `--skip-ci` keeps the bump
commit from deploying Pages.

Pushing the tag is what publishes. `release.yml` runs the whole PR gate matrix
first, then builds the site, the web SDK, and the per-OS `dist` binaries
in parallel, and attaches them to one GitHub Release whose notes are
`cog changelog --at v<version>`. It stays that way because the alternative — a
`post_bump_hook` that pushes — publishes a release from a command run at a desk.

Two files record a release and they are not interchangeable. `CHANGELOG.md` is
generated, per-version, never hand-edited. `docs/history/` is the hand-written
evidence archive: the decision, the measurement, the post-mortem. A paragraph
explaining *why* belongs in the history archive; the commit line belongs to cog.

Nightly builds are `nightly.yml` (`workflow_dispatch`), which tags
`nightly-YYMMDD-HHMM` and names its files after that rather than pretending to be
a version. `nix develop` provides cog at whatever version `flake.lock`'s nixpkgs
revision pins, and CI runs the same shell for the `commit` gate — so the tool that
checks a commit message locally is the tool that checks it in CI.

---

## Documentation Validation

```bash
bash scripts/check-docs.sh
```

This checks relative Markdown links under `docs/`, rejects completed-status rows still listed as active roadmap work, and fails on stale known-gap wording for completed roadmap items. CI runs it in the `doc` job.

## Web distribution changes

The web player and its optional capabilities ship as one SDK in the generated
`runtime/` directory under `web/`; that output is absent from a clean checkout.
Use `scripts/ci.sh gate web-build web-sdk-integrity` in the
`.#web-ci` shell for a fresh build, then `scripts/ci.sh gate
web-transactions` to exercise the complete SDK and site archives after
unpacking. SDK staging, restore, site assembly, and release packaging each
verify the manifest against the source identity and workspace version. Do not
copy only the player script or an individual capability file into a site.
Pages consumes the verified CI SDK artifact when available; the source-build
fallback runs the web build and its browser gates.

---

## Adding a New Expression Semantic to the IR

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

## Adding a Component to the ECS World

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

## GUI Actor Creation

Users can create actors from the GUI via toolbar `+`, inspector CTA, or right-click canvas. New actors are inserted into the current keyframe block (or wrapped in `#0s` if none exist).

Supported types: Rect, Ellipse, Line, Polygon, Path, Text, Row, Col. Code-only types (Math, Image, Svg, plots) are not creatable from GUI.

To add GUI creation support for a new primitive:
1. Ensure it implements `Primitive::default_props()` in `primitives/<name>.rs`.
2. Add the `SourceEdit::InsertActor` handling in `source_edit/actor_edits.rs` if new behavior is needed.
3. Wire the palette entry in `app/shell/toolbar.rs`.

### Effects

Effects are not primitives, so they do not go through actor creation. The
insertion palette's "Effects" tab is populated from the runtime `EFFECTS` array,
and selecting one inserts `label: Type` at the head of the selected `Filter`
scope's body (`SourceEdit::InsertEffect`, handled by
`source_edit::actor_edits::insert_effect`). Adding a built-in effect therefore
needs no GUI change — it appears in the palette and the Inspector's "Effects"
group as soon as it is in `EFFECTS`. The end-to-end checklist is in
`docs/effects.md` §8.

---

## Commit Messages

This project uses [Conventional Commits](https://www.conventionalcommits.org/) enforced by [Cocogitto](https://docs.cocogitto.io/).

### Quick Reference

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

**Scopes (common):** `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`

### Examples

```bash
# Feature in the GUI
cog commit feat "add timeline scrubbing" gui

# Bug fix in the parser
cog commit fix "handle empty keyframes" parser

# Breaking change
cog commit fix -B "drop support for old scene format" animatix

# Simple docs update
cog commit docs "document timeline evaluation order"
```

### Validation

```bash
# Check all commits since the last tag
cog check

# Check a specific range
cog check --from <commit-sha>
```

CI enforces conventional commits on every pull request.

### CI environment boundary

Linux project gates use the tool versions locked by `flake.lock`: the default
Nix shell for native checks and site assembly, `.#web-build` for wasm checks,
and `.#web-ci` for browser tests. Enter each from a clean shell, not from another
devshell. `scripts/ci.sh gate` checks the shell identity as well as Nix presence;
`scripts/ci.sh shell eparts` resolves the current operating system rather than
the first registry row. All project checks still belong in the gate registry.

`.#dist-driver` is intentionally different: it supplies only orchestration tools
for the existing Docker Linux distribution pipeline. The Ubuntu 22.04 build ABI,
pinned container Rust, FFmpeg closure and clean Ubuntu runtime acceptance remain
unchanged; no Nix libraries are injected into the shipped application. A running
Docker daemon is required outside the shell. Nix does not provide that daemon.

Windows/macOS eparts and distribution gates remain native to exercise the actual
OS toolchain and executable formats. Their rustup stable channel remains a known
floating-version exception, distinct from the flake-locked Rust and Docker's
fixed Rust baseline; this migration does not upgrade either release toolchain.
GitHub checkout, matrix planning, artifact transfer and publishing are workflow
control-plane/bootstrap steps, not project gates, and remain outside devshells.
This standardizes tool inputs, not fully hermetic builds: Cargo/npm downloads,
security advisory data and Docker's documented apt inputs still have their own
network and reproducibility boundaries.

The documentation path checker recognizes only the generated `web/runtime/`
root when it is absent in a fresh checkout. Paths below that root and other
missing web paths are still checked; this is not a general generated-path bypass.

---

*For the language specification, see [`spec.md`](spec.md). For the system architecture, see [`architecture.md`](architecture.md). For work items, see [`roadmap.md`](roadmap.md).
