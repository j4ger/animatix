# Agent Guide for Animatix

Animatix is a Rust workspace for a layout-first animation DSL (`.amx`). Pipeline: parse `.amx` → build `Timeline` → evaluate per-frame → render via Vello/WGPU.

## Map

- `crates/animatix-syntax`: parser, AST, module system, diagnostics, formatter, shared type system.
- `crates/animatix`: runtime engine, timeline, renderer, primitives, composition.
- `crates/animatix-gui`: eframe/egui IDE, preview, inspector, `SourceEdit`.
- `crates/animatix-analyzer`: shared language intelligence; update for new syntax.
- `crates/animatix-lsp`: LSP wrapper over analyzer.
- `crates/eparts`: themed egui widget framework used by the GUI.
- `tree-sitter-animatix`: highlighting grammar; touch when syntax tokens change.
- `docs`: documentation. `examples`: runnable `.amx` demos. `dogfood`: in-progress real-content projects and grammar probes.

## Workflow

1. Read relevant docs before changing (`docs/spec.md`, `docs/architecture.md`, etc.).
2. Keep tests green: run `cargo test -p animatix` and `cargo test -p animatix-gui` before finishing when relevant.
3. **Before committing**: format first, then run these checks and ensure they pass:
   ```bash
   cargo fmt --all                # Format the workspace; commit any resulting changes
   cargo check --workspace        # All crates compile
   cargo test -p animatix-syntax  # Parser tests pass
   cargo test -p animatix --lib   # Core library tests pass
   cargo test --no-fail-fast      # All tests across workspace
   ```
   Do not commit with build errors or test failures.

   > **Why `--workspace`?** Ensures all crates (including GUI, analyzer, LSP) compile. Prevents silent drift between core and tooling crates.
4. Update docs for user-visible behavior; keep `docs/roadmap.md` as only remaining work (remove completed items).
5. Ask on unclear design choices and call out design flaws you notice.
6. When committing, use `cog commit <type> "<summary>" [scope]` after staging files (example: `cog commit feat "add scrubbing" gui`). Use `cog commit --add ...` only if every unstaged change belongs in the commit. Fall back to `git commit -m "type(scope): summary"` only if `cog` is unavailable/blocked, and mention it.
7. Conventional commit scopes come from `cog.toml`: `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`.

## Common Pitfalls

- **GUI drift**: The GUI crate is excluded from `cargo check` (no `-p` flag), so errors can accumulate silently. Always run `cargo check --workspace` before committing to catch GUI, analyzer, and LSP compilation issues.
- **Tree-sitter grammar**: Changes to `.amx` syntax require updates to **both** the PEG parser (in `crates/animatix-syntax/src/parser/`) and the tree-sitter grammar (`tree-sitter-animatix/grammar.js`). Forgetting one breaks either parsing or syntax highlighting. After editing `grammar.js`, always regenerate the parser:
  ```bash
  cd tree-sitter-animatix && tree-sitter generate
  ```
  Then run the full sync check:
  ```bash
  bash scripts/check-parser-sync.sh
  ```
  The script runs `cargo test -p animatix-syntax`, the tree-sitter corpus tests, and parses every `.amx` under `examples/` recursively with tree-sitter, reporting failures.

  Known constructs that must be kept in sync (verified as of this writing):
  | PEG construct | Tree-sitter rule |
  |---|---|
  | `pub label: Type` | `actor_declaration` (with `optional('pub')`) |
  | `label[n]: Type` | `actor_declaration` / `inline_actor_declaration` (with `array_index` field) |
  | `action name(params) {}` | `action_definition` (with `optional(parameter_list)`) |
  | `for item, i in list {}` | `for_block` / `inline_for_loop` (with `index_variable` field) |
  | `for (a, b) in list {}` | `for_block` / `inline_for_loop` (tuple variable) |
  | `x => expr` single-ident closure | `closure_expression` (no-parens form) |
  | `play module.Scene` dotted path | `play_statement` (accepts `path_expression`) |
  | `verb actor.child [mods]` | `target_list` (accepts `path_expression`) |
  | `verb actor[0] [mods]` | `target_list` (accepts `index_expression`) |
  | `bars[i].color = red` | `property_assignment` (target `indexed_target_path`) |
  | `bars[i].color := red` | `reactive_binding` (target `indexed_target_path`) |
  | `value: Bool | Str` | `type_annotation` (union of `_type_annotation`) |
  | `type LegendMode = Bool | Str` | `type_alias` |
  | trailing comma in `config {}` | `property_list` (with `optional(',')`) |
  | newline-separated inline items | `inline_items` (optional comma separator) |
  | `label: $$ content $$` shorthand | `typst_shorthand` (with `token(/[^$]*/)` content) |
- **Evaluation paths**: Build-time expressions use the AST tree-walker (`evaluate_expr`); frame-time modifier code and plot closures are lowered to IR and interpreted by the single IR executor (`execute_modifier_ir` / `evaluate_compiled_expr`). Leaf operators and builtins are shared through `eval_shared`, so new operators/builtins are added there to keep both paths in sync.

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
cargo build --features animatix/video
```

(Non-Nix users: install FFmpeg system libraries + `pkg-config`, then
`cargo build --features animatix/video`.)

To build just the crate without video:
```bash
cargo build -p animatix
```

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

## Code Rules

- Every `#[allow(dead_code)]` must have an inline justification comment explaining why the item is intentionally unused (e.g., `// Reserved for future X integration`). `#[allow(dead_code)]` without a comment is not allowed in committed code.
- Remove truly dead code instead of marking it dead, unless there is a concrete forward-looking reason to keep it.
- Never commit with `cargo check --workspace` errors. If a crate has pre-existing errors unrelated to your changes, document them in a comment in your commit message.

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

- Runtime paths return `Result`; `RenderError` lives in `renderer/error.rs`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`), not `println!`.
