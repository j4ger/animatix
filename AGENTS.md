# Agent Guide for Animatix

## Project

Animatix is a Rust workspace: a domain-specific language (`.amx` files) for layout-first animation, with a CLI, GUI (eframe/egui), analyzer, and LSP.

**Pipeline:** parse `.amx` → build `Timeline` → evaluate per-frame → render via Vello/WGPU.

## Crates && Paths

| Crate | Role | What agents touch |
|-------|------|-------------------|
| `animatix` | Core library: parser, AST, timeline compilation, renderer, diagnostics, module system | Most work happens here |
| `animatix-gui` | Desktop IDE: source editor, inspector, preview, scene list | Inspector/preview changes, `SourceEdit` variants |
| `animatix-analyzer` | Shared language intelligence: symbol table, completions | Rarely — add analyzer support for new syntax |
| `animatix-lsp` | LSP server wrapper over analyzer | Rarely |
| `tree-sitter-animatix` | Tree-sitter grammar for highlighting | Only when adding new syntax tokens |

Other paths:
`docs`: all documentation
`examples`: demo scenes that showcase language features

## Workflow

1. **Investigate before changing.** Read relevant docs in `docs/`.
2. **Keep tests green.** Run `cargo test -p animatix` and `cargo test -p animatix-gui` before finishing.
3. **Update docs.** If your change affects user-visible behavior, update `docs/spec.md` or `docs/architecture.md`.
4. **Keep the todo list clean.** In `docs/roadmap.md`, remove completed items rather than marking them done. The file should read as "what's left," not a history log.
5. **Use `cog` for commits.** When the user asks you to commit, prefer `cog commit <type> "<summary>" [scope]` over raw `git commit` so the message is validated against `cog.toml` (example: `cog commit feat "add scrubbing" gui`). Stage files explicitly first, or use `cog commit --add <type> "<summary>" [scope]` only when all unstaged changes belong in the commit. Only fall back to `git commit -m "type(scope): summary"` if `cog` is unavailable or non-interactive use is blocked; mention the fallback in your final note.
6. **Follow conventional commits.** Commit messages must use Conventional Commits with a valid scope from `cog.toml` when scoped (common scopes: `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`). Examples: `feat(gui): add scrubbing`, `fix(parser): handle empty keyframes`, `docs: refresh examples guide`.
7. **Ask if unsure.** If you are unclear or need to decide between critical design choices, ask the user and present with pros and cons. Also inform the user if you spot any design flaw during any work.

### Error handling

- Runtime paths return `Result`. `RenderError` lives in `renderer/error.rs`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`) — not `println!`.
