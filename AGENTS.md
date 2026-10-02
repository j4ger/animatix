# Agent Guide for Animatix

Animatix is a Rust workspace for a layout-first animation DSL (`.amx`). Pipeline: parse `.amx` → build `Timeline` → evaluate per-frame → render via Vello/WGPU.

## Map

- `crates/animatix-core`: shared language vocabulary (capability enums, effect contract types, the built-in **property descriptor table**, icon glyphs, `RenderError`); zero engine/parser deps.
- `crates/animatix-std`: built-in catalog — effect definitions (WGSL/pack/support) and primitive identity cards; the single source for built-in metadata.
- `crates/animatix-syntax`: parser, AST, module system, diagnostics, formatter, property/type layer; derives its contract tables from `animatix-std`.
- `crates/animatix-text`: the typst/fontdb text compiler behind the engine's `text` feature.
- `crates/animatix`: runtime engine — timeline, build/evaluate, primitive behaviour, composition, and the frame vocabulary the renderer consumes.
- `crates/animatix-render`: GPU presentation above the engine — Vello/wgpu renderer core, filter backend, offscreen frames, transitions, and export encoders. One-way: it uses only the engine's public API.
- `crates/animatix-gui`: eframe/egui IDE, preview, inspector, `SourceEdit`.
- `crates/animatix-analyzer`: shared language intelligence; update for new syntax.
- `crates/animatix-lsp`: LSP wrapper over analyzer.
- `crates/animatix-web`: wasm32 driver for browser playback (no in-browser
  editor — that is the desktop app). Parse/build reuses the module system in
  `SourcesOnly` mode; frames render into an offscreen vello target that is
  blitted to the WebGPU canvas surface. Embeds resolve their engine profile
  (`slim` default / `full`) per element and share one WebGPU context per
  loaded engine directory. `web/` is the static site: a light-editorial
  homepage (`web/index.html`, hero scene in `web/scenes/`), the language tour
  (`web/tour/`, its scenes in `web/tour/scenes/`, live editors in
  `tour/editor.js` driven by the embed's `applySource`), the scene gallery
  (`web/gallery.html`), the demo hub (`web/demos/index.html` with poster
  cards) and the demo pages (`web/demos/transformer/`), all skinned by
  `web/site.css` with shared nav/footer injected by `web/site-chrome.js`.
  The `<amx-player>` embed component lives in `web/embed/`; build/serve
  instructions, the attribute reference, and the GitHub Pages deployment
  notes are in `web/README.md`.
- `crates/eparts`: themed egui widget framework used by the GUI.
- `crates/animatix-syntax/src/token.rs`: the single lossless tokenizer; drives parser input, LSP semantic tokens, and GUI highlighting.
- `docs`: documentation (`docs/roadmap.md` is remaining work only; completed work is archived in `docs/history.md`). `examples`: runnable `.amx` demos. `dogfood`: in-progress real-content projects and grammar probes.

## Workflow

0. **Background tasks need no polling.** The agent harness notifies when a
   background command finishes — do not add a sentinel loop to "wait for it."
   A `while pgrep -f '<pattern>'` loop self-matches its own command line (the
   pattern string lives in the loop text) and never exits, leaving a zombie
   task. If a wait guard is truly required, bound the iterations and exclude
   self (`pgrep -f 'cargo bench' | grep -vx "$$"`).

1. Read relevant docs before changing (`docs/spec.md`, `docs/architecture.md`, etc.).
2. Keep tests green: run `cargo test -p animatix` and `cargo test -p animatix-gui` before finishing when relevant.
3. **Before committing**: format first, then run these checks and ensure they pass:
   ```bash
   cargo fmt --all                # Format the workspace; commit any resulting changes
   cargo check --workspace --all-targets   # All crates and targets compile
   cargo clippy --workspace --all-targets -- -D warnings  # CI enforces this; run it here too
   cargo test -p animatix-syntax  # Parser tests pass
   cargo test -p animatix --lib -- --test-threads=1   # Core library tests pass (serial avoids WGPU teardown SIGSEGV)
   cargo test --no-fail-fast -- --test-threads=1      # All tests across workspace
   ```
   Do not commit with build errors or test failures.

   > **Why `--workspace --all-targets`?** Ensures all crates (including GUI, analyzer, LSP) and all targets compile. Prevents silent drift between core and tooling crates. **Why clippy here?** `cargo check` does not surface lint-level problems, and the CI `clippy` job fails the build on warnings — running it locally keeps that job green instead of discovering lints after the push. **Run the `--workspace` commands inside `nix develop`** — the GUI's audio stack builds against system ALSA headers the dev shell provides (see Common Pitfalls).
4. Update docs for user-visible behavior; keep `docs/roadmap.md` as only remaining work (remove completed items).
5. Ask on unclear design choices and call out design flaws you notice.
6. When committing, use `cog commit <type> "<summary>" [scope]` after staging files (example: `cog commit feat "add scrubbing" gui`). `cog` is provided by the flake dev shell (cocogitto), so **run the commit while inside `nix develop`**; outside the shell it's not on `PATH`. Use `cog commit --add ...` only if every unstaged change belongs in the commit. Fall back to `git commit -m "type(scope): summary"` only if `cog` is genuinely unavailable/blocked, and mention it.
7. Conventional commit scopes come from `cog.toml`: `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`.

## Common Pitfalls

- **GUI/tooling drift**: `cargo check -p animatix` does not compile the GUI, analyzer, or LSP. Use `cargo check --workspace --all-targets` before committing so their errors cannot accumulate silently.
- **Single tokenizer**: Syntax tokens are defined once in `crates/animatix-syntax/src/token.rs`. The parser consumes that token stream, the analyzer and LSP use it for positions, and the GUI uses it for syntax highlighting. Do not add a second lexer or grammar.
- **Evaluation paths**: Build-time expressions use the AST tree-walker (`evaluate_expr`); frame-time modifier code and plot closures are lowered to IR and interpreted by the single IR executor (`execute_modifier_ir` / `evaluate_compiled_expr`). Leaf operators and builtins are shared through `eval_shared`, so new operators/builtins are added there to keep both paths in sync. When adding a new expression *semantic* (not just a builtin), follow the touch-point checklist in `docs/contributing.md` ("Adding a New Expression Semantic to the IR") — there are 8 implementation sites and two of them are compiler-invisible.
- **`cog` is inside the dev shell**: `nix develop` provides `cog` (cocogitto). If `cog` isn't found, you're outside the shell — re-enter `nix develop` before committing. Do not preemptively fall back to `git commit -m`; use it only when cog is genuinely blocked (e.g., the linked-worktree `.git` quirk for cross-worktree commits).
- **Workspace-wide cargo commands need the dev shell (ALSA)**: `animatix-gui` pulls `rodio` → `cpal` → `alsa-sys`, whose build script probes the system for ALSA through `pkg-config`. Outside `nix develop`, any `--workspace` cargo command dies inside that dependency — `The system library 'alsa' required by crate 'alsa-sys' was not found … PKG_CONFIG_PATH … alsa.pc` — and `cargo check --workspace --all-targets` reports a build failure before it ever looks at this repo's code. It is a missing system library, not a code fault: enter `nix develop` instead of editing `Cargo.toml`, gating the crate behind a feature, or setting `PKG_CONFIG_PATH` by hand. Note the asymmetry — commands scoped to a crate that does not reach the GUI (e.g. `cargo test -p animatix-web`) build and pass outside the shell, so a green single-crate run is not evidence the workspace checks will pass.
- **"Hidden by default" is position-dependent and invisible at the declaration site**: an actor declared **before the first keyframe** with no explicit `opacity` is seeded `opacity: 0` and stays invisible until an entrance action (`fade-in`, `wipe-in`, `draw-in`, `reveal-in`) or an explicit `opacity` assignment reveals it. One declared *inside* a keyframe is visible immediately. Nothing at the declaration hints at this, so it produces two failure modes: source that looks complete while rendering nothing, and a pixel / `debug_readback` measurement taken before the entrance runs that silently measures an empty stage. Before trusting any render measurement, confirm the actor is actually on screen at the time you sample: run `animatix check <file>` (a `never-revealed` warning means no entrance ever lifts it) and sample at or after the reveal. Related rule, same file: an actor with an *explicit* `opacity: 0` is a seed that any entrance lifts to 1.0, while any other authored opacity is what the entrance settles on — see `docs/history.md` ("Entrance opacity and declared duration").
- **Layering is one-way**: `animatix-render` depends on `animatix`, never the reverse. The seam is the `FilterBackend` trait in `animatix::timeline`; the engine's renderer module holds only frame vocabulary (`error`, `types`, `text`). The engine's `animatix-render` dependency is a **dev-dependency for the export-path examples** — do not use `animatix-render` types inside engine unit tests, because Cargo then builds `animatix` twice (test-mode and normal) and the types stop unifying.

## Optional Features

### Video Export

> **Always build/test the `video` feature inside `nix develop`.** The flake's
> `rustPlatform.bindgenHook` regenerates the `rsmpeg`/`rusty_ffmpeg` bindings
> from the FFmpeg headers provided by the dev shell, so the build works even
> though the pinned `rsmpeg` version's tag (`ffmpeg.8.0`) lags the FFmpeg
> nixpkgs ships (8.1). Running any `--features video` cargo command **outside**
> `nix develop` fails: `pkg-config` can't find FFmpeg and a stale prebuilt
> `binding.rs` is used instead. If you hit an `rsmpeg`/FFmpeg "field vs.
> accessor method" error, you are almost certainly outside the nix shell — not
> looking at a real version-incompatibility bug.

To enable video export, enter the dev shell first, then build with the `video` feature:
```bash
nix develop                          # provides FFmpeg + pkg-config + bindgenHook
cargo build -p animatix-cli --features video
cargo build -p animatix-gui --features video
```

The FFmpeg code lives in `animatix-render`, so both `video` features forward to
`animatix-render/video`; the engine itself has no `video` feature.
(Non-Nix users: install FFmpeg system libraries + `pkg-config`, then build the
same two targets with `--features video`.)

To build the engine without video:
```bash
cargo build -p animatix
```

Rendering, text, and SVG are **not** feature-gated: the engine's build/evaluate
paths and the `FilterBackend` seam need `vello`/`wgpu`/`typst`/`usvg`
unconditionally, so those three features were removed rather than left as
switches no supported build could turn off. The real feature switches are
`perf-tracing` (default-on), `serde`, `plugin-loading`, and `video` on the
CLI/GUI (which forwards to `animatix-render/video`).

Without FFmpeg, the default build includes rendering, text, SVG support, and single-frame raster export (PNG/WebP), but not video/GIF export. Only the FFmpeg-dependent formats (MP4/WebM/MOV/GIF) require the `video` feature.

The GUI crate (`animatix-gui`) does **not** include `video` by default, so `cargo test -p animatix-gui` runs without FFmpeg. To opt into video export:

```bash
# Build/test the GUI without FFmpeg (default — no video/GIF export)
cargo build -p animatix-gui
cargo test -p animatix-gui

# Build/test the GUI with video export (run inside `nix develop`)
cargo build -p animatix-gui --features video
cargo check -p animatix-gui --features video
```

Without the `video` feature, PNG and WebP export work normally. The export dialog will only show a "requires the 'video' feature (FFmpeg)" message when attempting an FFmpeg-dependent format (MP4/WebM/MOV/GIF).

## Adding or Changing a Property

`animatix-core::property::PROPERTY_DESCRIPTORS` is the single declaration of the
built-in property list (name, `Applicable` predicate, value kind). Two
derivations must follow, and tests fail the build if they don't line up:

1. `animatix-syntax::schema::raw_property_types()` — the type-system view, index
   aligned with the descriptor rows (`property_types_line_up_with_descriptors`).
2. `animatix::timeline::property_registry::BINDINGS` — the engine binding
   (plan-slot `ValueType`, flags, storage field, default, read source), joined
   into `PROPERTY_REGISTRY` by name (`every_binding_has_a_descriptor`,
   `value_kinds_agree_with_descriptors`). A descriptor with no binding must be
   listed in `UNBOUND_DESCRIPTORS` with a reason.

`PropertyId` is the descriptor row index and serialized plans store it: **append
rows, never reorder or remove** (`property_id_order_is_pinned` pins the count
and sentinel ids).

## Code Rules

- Every `#[allow(dead_code)]` must have an inline justification comment explaining why the item is intentionally unused (e.g., `// Reserved for future X integration`). `#[allow(dead_code)]` without a comment is not allowed in committed code.
- Remove truly dead code instead of marking it dead, unless there is a concrete forward-looking reason to keep it.
- Never commit with `cargo check --workspace` errors. If a crate has pre-existing errors unrelated to your changes, document them in a comment in your commit message.
- **`#[allow(unreachable_code)]` is banned.** It only ever appears when an early `return` made the rest of a function dead — which means the change disabled a code path rather than adding to it. Remove the early return or delete the dead path. (A committed debug experiment once left the whole multithreaded export loop unreachable behind this attribute; see `docs/history.md`, "Debug scaffolding that shipped as behaviour".)

### Commit-Time Guards Beyond `cargo test`
- **Hot-path changes need a benchmark.** Anything touching the per-frame render, scene-evaluation, or export loops must ship with the result of `scripts/perf-bench.sh compare` in the commit message. A committed investigation probe once added an unconditional full-canvas GPU readback to every filter scope, every frame; no test and no lint could see it, and the perf harness was not part of the checklist.
- **Debug instrumentation is additive or it does not land.** Probes must be env-gated (`ANIMATIX_DUMP_STAGES`, `ANIMATIX_DUMP_FRAMES`, `ANIMATIX_PROBE`) and must never change what the default build renders, encodes, or how many threads it uses. If a probe needs a behavioural change to reach the bug, make that change a separate commit with its own justification.
- **Pinned dependencies carry their reason.** Every git/rev pin in a `Cargo.toml` needs a comment naming what the pin protects against and what would lift it (see the `vello` pin), plus a test at the dependency boundary where one is feasible (`crates/animatix-render/tests/vello_img_probe.rs`).

### Never Silently Drop Values
- All property value drops must be logged with `tracing::warn!` or documented with a comment explaining why the drop is intentional.
- All expression drops must be logged with `tracing::debug!` or documented.
- Use evaluation helpers (`evaluate_expr`, `evaluate_expr_with_lookup_diagnostic`) instead of direct `Expr` matching where possible.
- When a property receives an unrecognized or wrong-type value, log it before skipping:
  ```rust
  match evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject) {
      Some(Value::Vec2([min, max])) => x_domain = [min, max],
      Some(v) => tracing::warn!("{}: 'x_domain' expects a (min, max) tuple, got {:?}", subject, v),
      None => {} // eval error already reported as a diagnostic
  }
  ```
- Intentional catch-all arms (`_ => {}`) must carry a comment, e.g.
  `_ => {} // Non-plot properties are handled by the general actor pipeline.`

## Code Style

- Runtime paths return `Result`; `RenderError` lives in `animatix-core` and is re-exported as `animatix::renderer::error`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`), not `println!`.
