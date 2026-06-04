# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## Phase 5 — Source Editor Foundation

> Heavy infrastructure work that unlocks code-quality tools. Users experience this as "formatting works," "round-trip editing is safe," and "insertions don't break layout."

### 5.1 Lossless AST (green tree)

Immutable syntax tree that preserves whitespace and comments. Enables reliable source editing without formatting loss.

**Current state:** AST has `Span`/`ByteSpan` for positions, `Property.trailing_comment`, `Stmt::Comment`. Parser (Chumsky) discards whitespace/comments. `to_source` normalizes formatting on re-serialize.

| # | Task | Description | Effort | Blocker |
|---|------|-------------|--------|---------|
| 5.1.1 | **Trivia data structure** | Define `Trivia` type (leading whitespace, trailing whitespace, comments). Add to AST nodes. | 1 week | — |
| 5.1.2 | **Parser trivia capture** | Modify Chumsky parser to capture whitespace/comments as trivia instead of discarding. | 2–3 weeks | 5.1.1 |
| 5.1.3 | **to_source trivia emission** | Update `ToSource` impl to emit trivia when present, fall back to normalized formatting when absent. | 1 week | 5.1.2 |
| 5.1.4 | **Source edit trivia preservation** | Update `source_edit` module to preserve trivia during AST mutations (property edits, keyframe inserts, etc.). | 1–2 weeks | 5.1.3 |
| 5.1.5 | **Round-trip tests** | Parse → serialize → parse → serialize produces identical output for all example files. | 1 week | 5.1.4 |

### 5.2 Source Formatter

`cog fmt` and editor auto-format that preserve the user's style choices.

**Current state:** `to_source.rs` has hardcoded formatting rules (2-space indent, blank lines between top-level). No configuration. No CLI command.

| # | Task | Description | Effort | Blocker |
|---|------|-------------|--------|---------|
| 5.2.1 | **Extract formatter module** | Pull formatting logic from `to_source` into standalone `formatter.rs` in `animatix-syntax`. | 3 days | — |
| 5.2.2 | **Formatter configuration** | Define `FormatConfig` (indent size, blank line rules, trailing comma style). Serialize to `.amx.toml` or similar. | 2 days | 5.2.1 |
| 5.2.3 | **CLI `cog fmt`** | Add `fmt` subcommand to `animatix-cli`. Formats files in-place or checks (`--check`). | 2 days | 5.2.1 |
| 5.2.4 | **Editor auto-format** | Wire formatter into LSP `textDocument/formatting` and GUI save-on-format. | 3 days | 5.2.1 |
| 5.2.5 | **Idempotency tests** | Formatting already-formatted code produces byte-identical output. | 1 day | 5.2.1 |

### 5.3 Lint and Diagnostics CLI

Static analysis for unused actors, missing imports, and type mismatches. Runnable from `animatix-cli`.

**Current state:** `animatix-analyzer` has basic diagnostics (duplicate labels, unknown types/properties, undefined labels). LSP server exists. No CLI linter.

| # | Task | Description | Effort | Blocker |
|---|------|-------------|--------|---------|
| 5.3.1 | **Unused actor detection** | Warn on actors declared but never referenced in actions/assignments. | 3 days | — |
| 5.3.2 | **Missing import detection** | Error on `import "path"` where file doesn't exist. | 1 day | — |
| 5.3.3 | **Type mismatch warnings** | Warn on property assignments with wrong types (e.g., `size: "hello"`). Uses type annotations when present. | 1 week | — |
| 5.3.4 | **CLI `animatix lint`** | Add `lint` subcommand. Runs diagnostics on one or more `.amx` files. Exit code 1 on errors. | 2 days | 5.3.1–5.3.3 |
| 5.3.5 | **Lint configuration** | `.amx.toml` or inline `// lint-disable: unused-actor` to suppress specific warnings. | 2 days | 5.3.4 |

### 5.4 Snippet-aware Insertion

Parse palette snippets into AST before inserting (instead of raw text surgery). Prevents malformed insertions and respects surrounding formatting.

**Current state:** GUI insertion palette inserts raw text. `source_edit` does AST mutation for property/keyframe edits but not for snippet insertion.

| # | Task | Description | Effort | Blocker |
|---|------|-------------|--------|---------|
| 5.4.1 | **Snippet parser** | Parse snippet text into `Vec<Stmt>` fragment. Handle partial snippets (e.g., just an actor decl without keyframe). | 1 day | — |
| 5.4.2 | **AST fragment merge** | Insert parsed fragment into existing AST at cursor position (inside keyframe, inside container, top-level). | 1 day | 5.4.1 |
| 5.4.3 | **Format-aware insertion** | Re-merge with surrounding trivia so blank lines/indentation are correct. | 1 day | 5.4.1, 5.2.1 |

---

## Phase 6 — Web Export & Media

> Expanding output targets and preview fidelity.

| # | Feature | Description | Effort | Blocker |
|---|---------|-------------|--------|---------|
| 1 | **Web canvas / WASM export** | Render to HTML5 Canvas or WebGPU for browser-based playback. Standalone `animatix-web` crate. | 1–2 months | Renderer abstraction |
| 2 | **Audio playback in preview** | Play audio segments during GUI preview (currently only muxed on video export). | 1 week | Audio backend (rodio/cpal) |
| 3 | **APNG export** | Animated PNG output for lossless web animations. Requires an APNG encoder backend. | 3 days | APNG encoder |

---

## Icebox

> Blocked on external dependencies, solve niche problems, or lack user demand. No committed timeline.

| Feature | Blocker / Reason |
|---------|------------------|
| **Zero-readback GPU filters** | Blocked on Vello GPU filter support ([#1296](https://github.com/linebender/vello/issues/1296)). Phase 8.6a (GPU compute + readback) ships the perf win; wait for Vello to eliminate the readback. |
| **Scene primitive (PiP)** | Existing components + Stack cover reuse. Needs composition-level design for parallel playback. Revisit after transition blending. |
| **Export performance: pre-compiled plot closures** | Only matters for dozens of plot actors. `always` block easing covers simpler cases. |
| **Asset usage tracking** | Show which actors reference an asset. Low user demand. |
| **Variable track UI** | GUI for `let` variable tracks. `always` blocks cover most cases. |
| **Module dependency graph** | Visual graph of `.amx` imports. Internal tooling, no user stories. |
