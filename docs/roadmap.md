# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## Phase 5 — Source Editor Foundation

> Heavy infrastructure work that unlocks code-quality tools. Users experience this as "formatting works," "round-trip editing is safe," and "insertions don't break layout."

| # | Feature | Description | Effort | Blocker |
|---|---------|-------------|--------|---------|
| 1 | **Lossless AST (green tree)** | Immutable syntax tree that preserves whitespace and comments. Enables reliable source editing without formatting loss. | 2–3 months | — |
| 2 | **Source formatter** | `cog fmt` and editor auto-format that preserve the user's style choices. | 2 weeks | 1 |
| 3 | **Lint and diagnostics CLI** | Static analysis for unused actors, missing imports, and type mismatches. Runnable from `animatix-cli`. | 2 weeks | 1 |
| 4 | **Snippet-aware insertion** | Parse palette snippets into AST before inserting (instead of raw text surgery). Prevents malformed insertions and respects surrounding formatting. | 2 days | 1 |

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
