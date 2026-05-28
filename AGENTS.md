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
5. **Follow conventional commits.** All commits must follow the Conventional Commits spec (e.g., `feat(gui): add scrubbing`, `fix(parser): handle empty keyframes`). Use `cog commit` if unsure.
6. **Ask if unsure.** If you are unclear or need to decide between critical design choices, ask the user and present with pros and cons. Also inform the user if you spot any design flaw during any work.

### Error handling

- Runtime paths return `Result`. `RenderError` lives in `renderer/error.rs`.
- Test code may use `.unwrap()` / `.expect()`.
- Use `tracing` (`info!`, `debug!`, `warn!`, `error!`) — not `println!`.
