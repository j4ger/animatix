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
3. Update docs for user-visible behavior; keep `docs/roadmap.md` as only remaining work (remove completed items).
4. Ask on unclear design choices and call out design flaws you notice.
5. When committing, use `cog commit <type> "<summary>" [scope]` after staging files (example: `cog commit feat "add scrubbing" gui`). Use `cog commit --add ...` only if every unstaged change belongs in the commit. Fall back to `git commit -m "type(scope): summary"` only if `cog` is unavailable/blocked, and mention it.
6. Conventional commit scopes come from `cog.toml`: `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`.

## Code Style

- Runtime paths return `Result`; `RenderError` lives in `renderer/error.rs`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`), not `println!`.
