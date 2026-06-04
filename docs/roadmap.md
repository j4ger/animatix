# Animatix Roadmap

> Forward-looking view of planned features, grouped by user value and technical priority.
> For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specs, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Priority Order

1. **Phase 5** — Source Editor Foundation (lossless parsing, formatting, linting)
2. **Phase 6** — Web Export & Media (WASM, audio, remaining formats)

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

> Expanding output targets and preview fidelity. These are the features that let users ship to more platforms and iterate faster.

| # | Feature | Description | Effort | Blocker |
|---|---------|-------------|--------|---------|
| 1 | **Web canvas / WASM export** | Render to HTML5 Canvas or WebGPU for browser-based playback. Standalone `animatix-web` crate. | 1–2 months | Renderer abstraction |
| 2 | **Audio playback in preview** | Play audio segments during GUI preview (currently only muxed on video export). | 1 week | Audio backend (rodio/cpal) |
| 3 | **APNG export** | Animated PNG output for lossless web animations. Requires an APNG encoder backend. | 3 days | APNG encoder |

---

## Icebox (no committed timeline)

> These are real features but are either blocked on external dependencies, solve niche problems, or lack user demand. They are not on any committed schedule.

| Feature | Blocker / Reason to defer |
|---------|---------------------------|
| **Export performance: pre-compiled plot closures** | Only matters for scenes with dozens of plot actors. `always` block easing covers simpler cases. |
| **Asset usage tracking** | Show which actors reference an asset. Requires cross-referencing `AssetCache` with AST; low user demand. |
| **Variable track UI** | GUI for `let` variable tracks inside keyframes. Very niche; `always` blocks cover most use cases. |
| **Module dependency graph** | Visual graph of `.amx` imports. Internal tooling; no user stories yet. |
| **Scene primitive (PiP)** | Actor that renders another scene's timeline inside itself. Premature: existing components + Stack cover reuse cases; parallel playback needs composition-level design, not an actor. Revisit after transition blending (Phase 7). |
| **Zero-readback GPU filters** | Eliminate CPU readback in filter pipeline. Blocked on Vello GPU filter support ([#1296](https://github.com/linebender/vello/issues/1296)). Phase 8.6a (GPU compute + readback) is shipped and sufficient for now. |

---

## Removed (superseded or internal-only)

| Item | Reason |
|------|--------|
| `let` variable animation | Superseded by easing functions in `always` blocks. |
| Unify duplicate `PropertyValue` types | Internal refactor with no user-facing impact. |
| Replace `node_local_bounds` with trait-based bounds | Internal refactor with no user-facing impact. |
| Validate `CreateActor` props | Already handled at runtime via diagnostics and timeline build errors. |

---

## Vello Dependency Tracking

> Upstream Vello features that affect Animatix's roadmap.

### Filter Effects (Issue [#1296](https://github.com/linebender/vello/issues/1296))

**Status:** CPU implementation merged (PR [#1286](https://github.com/linebender/vello/pull/1286), Nov 2025). GPU implementation planned.

**What Vello ships now:**
- `push_filter_layer(filter)` / `set_filter_effect(filter)` API
- GaussianBlur, DropShadow, Flood primitives
- RenderGraph DAG scheduler for nested filters
- LayerManager for buffer allocation + reuse
- Blur decimation (auto-downsample for large radii)

**What's planned:**
- GPU implementation ("naturally extends to vello_hybrid")
- ColorMatrix, Brightness, Contrast, Saturate, HueRotate, Grayscale, Sepia
- Blend, Composite, Morphology, ConvolveMatrix

**Impact on Animatix:**
- When Vello GPU filters ship, we can replace our `GpuFilterBackend` with Vello's native filter API
- This eliminates the CPU readback in our Phase 8.6a pipeline
- Our current approach (render-to-texture → GPU filter → readback → composite) matches Vello's architecture
- No action needed until Vello GPU filters are stable

**Current Animatix status:**
- Phase 8.6a shipped: GPU compute shaders for blur + color matrix, one readback per filter
- Phase 8.6b (zero-readback) deferred: would conflict with Vello's direction
