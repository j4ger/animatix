# Agent Guide for Animatix

Animatix is a Rust workspace for a layout-first animation DSL (`.amx`). Pipeline: parse `.amx` → build `Timeline` → evaluate per-frame → render via Vello/WGPU.

## Map

- `crates/animatix`: core parser, AST, timeline, renderer, diagnostics, modules.
- `crates/animatix-gui`: eframe/egui IDE, preview, inspector, `SourceEdit`.
- `crates/animatix-analyzer`: shared language intelligence; update for new syntax.
- `crates/animatix-lsp`: LSP wrapper over analyzer.
- `tree-sitter-animatix`: highlighting grammar; touch when syntax tokens change.
- `docs`: documentation. `examples`: runnable `.amx` demos.

## Workflow

1. Read relevant docs before changing (`docs/spec.md`, `docs/architecture.md`, etc.).
2. Keep tests green: run `cargo test -p animatix` and `cargo test -p animatix-gui` before finishing when relevant.
3. **Before committing**: run these checks and ensure they pass:
   ```bash
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
- **Tree-sitter grammar**: Changes to `.amx` syntax require updates to **both** the PEG parser (in `crates/animatix`) and the tree-sitter grammar (in `tree-sitter-animatix`). Forgetting one breaks either parsing or highlighting.
- **Evaluation paths**: The engine has two evaluation paths — the tree-walker and the IR/VM. New features (expressions, operators, built-ins) must be added to both paths to stay in sync.

## Optional Features

### Video Export

To enable video export, install FFmpeg system libraries and build with:
```bash
cargo build --features animatix/video
```

To build just the crate without video:
```bash
cargo build -p animatix
```

Without FFmpeg, the default build includes rendering, text, and SVG support, but not video export.

The GUI crate (`animatix-gui`) depends on the `video` feature by default, so GUI builds still require FFmpeg.

## Code Rules

- Every `#[allow(dead_code)]` must have an inline justification comment explaining why the item is intentionally unused (e.g., `// Reserved for future X integration`). `#[allow(dead_code)]` without a comment is not allowed in committed code.
- Remove truly dead code instead of marking it dead, unless there is a concrete forward-looking reason to keep it.
- Never commit with `cargo check --workspace` errors. If a crate has pre-existing errors unrelated to your changes, document them in a comment in your commit message.

## Code Style

- Runtime paths return `Result`; `RenderError` lives in `renderer/error.rs`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`), not `println!`.
