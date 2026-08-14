# Contributing to Animatix

## Order of Operations

When making a non-trivial change:

1. **Align the design** — check `docs/` for existing design notes
2. **Update docs** — match the intended behavior before coding
3. **Implement** — clean, readable code matching existing patterns
4. **Add tests** — targeted and readable; focused coverage over snapshots
5. **Add examples** — a focused demo that proves the feature is real
6. **Verify** — build, test, render, and validate

If a change alters the user-facing language surface, docs and examples ship with the code.

## Validation

```bash
cargo build
cargo test
cargo clippy --all-targets -- -D warnings

# Inspect parser output
cargo run --bin animatix -- ast path/to/file.amx --compact

# Export a frame for visual validation
cargo run --bin animatix -- image path/to/file.amx --time 1.5 --output /tmp/frame.png
```

If a change affects `.amx` syntax or highlighting, update the single tokenizer in
`crates/animatix-syntax/src/token.rs` and run the syntax, GUI, and LSP tests.

## Commit Messages

This project uses [Conventional Commits](https://www.conventionalcommits.org/) enforced by [Cocogitto](https://docs.cocogitto.io/).

```bash
cog commit feat "add timeline scrubbing" gui
cog commit fix "handle empty keyframes" parser
```

Scopes: `animatix`, `gui`, `analyzer`, `lsp`, `syntax`, `parser`, `renderer`, `timeline`, `ci`, `docs`.

## Documentation

Keep these in sync when relevant: `README.md`, `docs/spec.md`, `docs/architecture.md`, `docs/primitives.md`, `docs/properties.md`, `docs/roadmap.md`, `examples/README.md`, `dogfood/README.md`.

## Detailed Guide

For project structure, GUI data flow, LSP setup, error model, and full development workflows, see [`docs/contributing.md`](docs/contributing.md).
