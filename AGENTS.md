# Agent Guide for Animatix

Animatix is a Rust workspace for a layout-first animation DSL (`.amx`). Pipeline: parse `.amx` → build `Timeline` → evaluate per-frame → render via Vello/WGPU.

## Map

- `crates/animatix-core`: shared capability and effect contracts, the built-in **property descriptor table**, icon glyphs, `RenderError`, and the built-in catalog under `animatix_core::builtins`. The crate has no engine or parser dependencies.
- `crates/animatix-syntax`: parser, AST, module system, diagnostics, formatter, and property/type layer. Contract tables derive from `animatix_core::builtins`.
- `crates/animatix-text`: plain text shaping and optional Typst compilation behind `rich-text`. Native builds embed fonts via `bundled-fonts`, and web packages load the `web/fonts` pack.
- `crates/animatix`: engine — timeline ECS world/stores, build/evaluate, `BehaviorDef` family systems, composition, frame vocabulary.
- `crates/animatix-render`: GPU presentation above the engine — Vello/wgpu core, filter backend, offscreen frames, transitions, export encoders. One-way: public engine API only.
- `crates/animatix-cli`, `crates/animatix-gui`: CLI and eframe/egui workbench (source editor, preview, read-only inspector).
- `crates/animatix-analyzer`, `crates/animatix-lsp`: language intelligence and its LSP wrapper; update the analyzer when syntax changes.
- `crates/animatix-web`: wasm32 playback and source preparation with `SourcesOnly` module reuse. The SDK uses Vello GPU and prefers WebGPU with WebGL2 fallback. The site provides a source editor at `/playground/`. See `web/README.md` for SDK and site entry points.
- `crates/animatix-plugin-api`, `-demo`, `-tooling`: native plugin C ABI, example, tooling. `crates/eparts`: themed egui widgets.
- `crates/animatix-syntax/src/token/`: the single lossless tokenizer (parser input, LSP tokens, GUI highlighting).
- `docs`: `roadmap.md` is remaining work only, `docs/history/` is the archive (`history.md` is a pointer), `docs/ecs.md` is normative ECS + structure contract. `examples/` has supported demos; focused regression fixtures belong beside their tests. Human review and temporary experiments run through the harness.

## Key Docs

- `docs/spec.md` — language surface; `docs/primitives.md` and `docs/properties.md` are the quick reference tables.
- `docs/architecture.md` — pipeline and crate layering; `docs/ecs.md` — timeline storage, component checklist, structure gate.
- `docs/effects.md` — effect chain/ABI; `docs/extension_authoring.md` — registering primitives, actions, functions, plugins.
- `docs/contributing.md` — workflows, project structure, LSP, error model, release details; `docs/performance_evaluation.md` — measurement method.
- `docs/roadmap.md` — what is left; `docs/history/` — evidence for what landed and why.

## Workflow

0. **Background tasks need no polling.** The harness notifies on completion; a `while pgrep -f '<pattern>'` loop self-matches and never exits. If a bound is truly needed, exclude self (`pgrep -f 'cargo bench' | grep -vx "$$"`).
1. Read the relevant docs before changing (`spec.md`, `architecture.md`, `ecs.md`, etc.).
2. Keep tests green: `tools/ci/ci.sh gate test` (or `cargo test -p animatix --lib -- --test-threads=1` plus `-p animatix-gui` for one crate).
3. **Run the gates before committing.** `tools/ci/ci.sh` is the single definition of green; every check is a named gate there, CI's matrix comes from it, and `release.yml` uses the same list. Add a gate, never a cargo command in workflow YAML (`workflows` fails that). `tools/ci/ci.sh gates` lists every gate **and its required shell**; run it in that shell (`env -i` if a dev shell is already loaded, because devshells prepend to PATH). Always run `tools/ci/ci.sh gate fmt`; never commit with a failing gate. Rationale and shell/PATH traps: `docs/contributing.md`.
4. **An embedded scene owes a resting composition.** `autoplay loop` cycles are `duration` + `hold`, so an outro may animate parameters but may not erase the cast: the held frame is what the page shows. `--policy site` and resting composition are still checked by `web/tools/site/site-rest-check.py`. Author scenes with visual review as the primary judgment:
   - `animatix storyboard <file>`: primary visual check for cues, overlaps/clipping, and resting frame.
   - `animatix audit <file> --evidence <dir>`: provides structural/pixel facts, candidates, and per-finding cropped/annotated evidence images.
   - `animatix audit --delta <baseline_json>`: focuses review on new changes.
   - `animatix baseline <file>`: guards regression.
   - `animatix check <file>`: assists iteration.
   - Budget: `tools/quality/audit-budget.json` manages accumulated debt.
5. Update docs for user-visible behavior. `docs/roadmap.md` is remaining work only; `docs/releases/` files named `vX.Y.Z.md` contain human-edited, finalized release notes; `git-cliff` may produce a draft for review. `CHANGELOG.md` is an optional overview. Detailed rationale/post-mortems go in `docs/history/`.
6. Ask on unclear design choices and call out design flaws you notice.
7. Stage only the intended changes and use ordinary `git commit`. Conventional Commits are recommended, not required; choose a useful scope when appropriate.
8. Review and edit release notes for readers, rather than copying the commit log verbatim.
9. Release only with an explicitly chosen version and tag on clean `main`. Preparing files does not authorize committing, tagging or publishing; never infer a version bump from commit messages. Details: `docs/contributing.md`.

## Common Pitfalls

- **GUI/tooling drift**: `cargo check -p animatix` does not compile the GUI, analyzer, or LSP; use `cargo check --workspace --all-targets`.
- **Timeline data is ECS data**: per-entity state belongs in a `World` component store keyed by `EntityId` — never a new label-keyed `Timeline` map or aggregate per-actor struct. `ActorCaps`/`BehaviorId` are stored components and dispatch goes through `BehaviorDef` family systems. `docs/ecs.md` is normative: follow its "Adding a Component" checklist, update snapshot helpers in the same change, route writes through `Timeline`/`EntityMut` so the frame cache is invalidated, and never reintroduce a string-keyed frame-path fallback.
- **Follow the structure gate**: `tools/ci/ci.sh gate structure` enforces `docs/ecs.md` §8: production files <= 1000 lines, tests <= 2000, production functions <= 200 lines, tests <= 300. Over-limit functions and implementation-heavy `mod.rs` files are frozen in `tools/quality/function-size-allowlist.tsv` and `tools/quality/mod-rs-allowlist.tsv`; split the function/file, delete its row, and never add or raise one.
- **Evaluation paths**: build-time expressions (`evaluate_expr`) and frame-time modifier/plot closures both use the unified IR VM (`compile_expr` → `evaluate_compiled_expr`, leaves shared via `eval_shared`). A new expression *semantic* follows the checklist in `docs/contributing.md`.
- **Workspace-wide cargo needs the dev shell (ALSA)**: `animatix-gui` → `rodio` → `cpal` → `alsa-sys` probes ALSA via `pkg-config`; outside `nix develop` any `--workspace` command dies there. It is a missing system library, not a code fault — do not edit `Cargo.toml` or set `PKG_CONFIG_PATH`. A GUI-free single-crate run passing is not workspace evidence.
- **"Hidden by default" is position-dependent**: an actor declared **before the first keyframe** with no explicit `opacity` is seeded `opacity: 0` until an entrance (`fade_in`, `wipe_in`, `draw_in`, `reveal_in`) or explicit `opacity` reveals it; one declared *inside* a keyframe is visible at once. A pixel/`debug_readback` sample taken before the entrance measures an empty stage: run `animatix check` (a `never-revealed` warning means nothing lifts it) and sample at/after the reveal. An explicit `opacity: 0` is a seed any entrance lifts to 1.0; other authored opacities are where it settles. See `docs/history/2026-09.md` ("Entrance opacity and declared duration").
- **Layering is one-way**: `animatix-render` depends on `animatix`, never the reverse. The seam is `FilterBackend` in `animatix::timeline`; the engine's renderer module holds only frame vocabulary. `animatix-render` is a **dev-dependency** for export-path examples — do not use its types in engine unit tests, or Cargo builds the engine twice and the types stop unifying.

## Optional Features: Video Export

> **Build/test `video` inside `nix develop`.** The flake's `bindgenHook` regenerates the `rsmpeg`/`rusty_ffmpeg` bindings from the dev shell's FFmpeg headers. Outside it, `pkg-config` cannot find FFmpeg and a stale prebuilt `binding.rs` is used — an `rsmpeg` "field vs. accessor" error usually means you are outside the shell, not a version bug.

```bash
nix develop                       # FFmpeg + pkg-config + bindgenHook
cargo build -p animatix-cli --features video
cargo build -p animatix-gui --features video   # GUI has no video by default
```

Both features forward to `animatix-render/video`; the engine has none. Native CLI/GUI keep rendering, text, and SVG enabled by default; `animatix-web` ships one SDK with on-demand rich text, image/SVG, and raster encoding capability blocks. Other real switches are `perf-tracing` (default-on), `bundled-fonts` (native default; off in web packages), `serde`, `plugin-loading`, and `video` on CLI/GUI. Without FFmpeg, PNG/WebP still export; MP4/WebM/MOV/GIF require `video`.

## Adding or Changing a Property

`animatix-core::property::PROPERTY_DESCRIPTORS` is the single declaration (name, `Applicable`, value kind, and exceptional `AuthoredTypeHint`). Three things must line up, or tests fail:

1. `animatix-syntax::schema::raw_property_types()` — the derived type-system view, index-aligned (`property_types_line_up_with_descriptors`).
2. `animatix::timeline::property_registry::BINDINGS` — engine binding (plan-slot `ValueType`, flags, storage field, default, read source), joined into `PROPERTY_REGISTRY` by name (`every_binding_has_a_descriptor`, `value_kinds_agree_with_descriptors`); an unbound descriptor must appear in `UNBOUND_DESCRIPTORS` with a reason.
3. The frame-time read path and its `ANIMATED` flag (`frame_consumed_properties_are_flagged_animated`).

`PropertyId` is the descriptor row index and serialized plans store it: **append rows, never reorder or remove** (`property_id_order_is_pinned`).

## Code Rules

- Every `#[allow(dead_code)]` needs an inline justification; remove truly dead code unless a concrete forward-looking reason exists.
- Never commit with `cargo check --workspace` errors; document pre-existing unrelated errors in the commit message.
- **`#[allow(unreachable_code)]` is banned.** It only appears when an early `return` made the rest dead — remove the return or delete the dead path. (A debug experiment once left the export loop unreachable; see `docs/history/2026-09.md`, "Debug scaffolding that shipped as behaviour".)
- **Hot-path changes need a benchmark.** Per-frame render, scene-evaluation, or export-loop changes ship with `tools/quality/perf-bench.sh compare` output in the commit message. That compare is manual evidence, not a PR gate; when the 1-minute load exceeds `PERF_MAX_LOAD_RATIO × nproc` it exits 2 — fix the environment and rerun rather than loosening the threshold.
- **Debug instrumentation is additive or it does not land.** Probes must be env-gated (`ANIMATIX_DUMP_FRAMES`, `ANIMATIX_PROBE`) and must never change what the default build renders/encodes or how many threads it uses. Gating output is not enough: `std::time::Instant::now()` traps on `wasm32-unknown-unknown`, so an unconditional timer panics the web player — use `probe::timing_probe()` and treat a new `Instant::now()` in a wasm-reachable crate as a bug.
- **Pinned dependencies carry their reason.** Every git/rev pin names what it protects against and what would lift it and includes a boundary test where feasible. The Vello GPU image cache has coverage in `crates/animatix-render/tests/vello_img_probe.rs`.

### Never Silently Drop Values

- Property value drops log `tracing::warn!` or a comment; expression drops log `tracing::debug!` or a comment.
- Prefer `evaluate_expr` / `evaluate_expr_with_lookup_diagnostic` over matching `Expr` directly.
- Log an unrecognized or wrong-type value before skipping it, then keep the diagnostic path intact:

  ```rust
  match evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject) {
      Some(Value::Vec2([min, max])) => x_domain = [min, max],
      Some(v) => tracing::warn!("{}: 'x_domain' expects a (min, max) tuple, got {:?}", subject, v),
      None => {} // eval error already reported as a diagnostic
  }
  ```

- Intentional catch-all arms carry a comment, e.g. `_ => {} // Non-plot properties are handled by the general actor pipeline.`

## Code Style

- Runtime paths return `Result`; `RenderError` lives in `animatix-core`, re-exported as `animatix::renderer::error`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`), not `println!`.
