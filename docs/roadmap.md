# Animatix Roadmap

Canonical source of truth for remaining work. When a segment is fully done,
remove the completed items from this file. Detailed planning documents and
historical archives live in [docs/plans/README.md](plans/README.md).

---

## Active Work

### eparts Framework Expansion (committed, unscheduled)

The `eparts` crate has an active framework-expansion track that is committed but
not yet scheduled. It includes JSON themes, hot-reload, table/chart/webview
surfaces, i18n, accessibility depth, a gallery app, CI parity, and related
framework widgets.

The full itemized list and sequencing guidance are in
[docs/plans/eparts-refinement-roadmap.md](plans/eparts-refinement-roadmap.md)
section `6.X`. First candidates when capacity opens are the gallery app, JSON
themes, CI platform parity, and `StyledExt` helpers.

### GUI Follow-Ups

| Item | Status / Notes |
|------|----------------|
| Opportunistic eparts widget adoption | Partially complete; remaining call sites migrate when the surrounding GUI area is next edited. |

### Structural Refactors

| Item | Status / Notes |
|------|----------------|
| Generic primitive build path | Partially complete; `Callout` now uses the generic actor build path and schema-driven keyframe writing. `Legend` still bypasses the generic pipeline for `at` and entry scanning, but its metadata now uses generic tagged tracks. |
| Migrate remaining bespoke enum-like properties | Open; `ShapeType`, `PlacementMode`, `MorphOptions`, and related typed tracks still use bespoke enums instead of `ValueType::Enum`/`ValueType::Sum`. Needs a migration plan because these types are embedded in dispatch, interpolation, persistence, and GUI code. |
| Unify GUI and core property value models | Open; GUI `PropertyValue` and core `PropertyValue` are separate and require manual conversions. Replace with one schema-driven value model used by commands, validation, inspector, spreadsheet, and gestures. |
| Clean up tree-sitter grammar conflicts | Open; many conflict declarations are reported as unnecessary, but removing them breaks parsing without a deeper grammar redesign. Needs the canonical `::` type-path grammar and conflict cleanup as one workstream. |
| Make `LegendTracks` metadata generic | Partially complete; title, font size, label color, swatch size, gap, and wrapping now live in generic tagged property tracks with cached `LegendTracks` fields retained for GUI/serde compatibility. |
| Merge tree-walker and IR/VM into single execution engine | Needs design spike before scheduling. Batch 6 (#12) extracted shared helpers, but the dual-path-with-fallback is currently a safety feature for closures. |

### Language and Runtime Gaps

| Item | Status / Notes |
|------|----------------|
| Precise shape/path/text bounds | Open; callout geometry and actor anchor points use world-space affine plus available local bounds. Exact text/path bounds remain deferred. |
| Text/Typst/Code frame-time content overrides | Open; timed assignments recompile glyph paths, but changing `text` directly inside `always` is not a supported path. |
| Data-dependent algorithm timelines | Open; no runtime mutable state or branching timeline, so algorithm animations must be hand-unrolled recordings. Confirmed by `dogfood/projects/sorting-visualizer`. |

---

## Audit History

The 2026-08-05 audit trail is archived at
[docs/plans/archive/roadmap-audit-2026-08-05.md](plans/archive/roadmap-audit-2026-08-05.md).
Future sessions should read `Active Work` above for current remaining items and
consult the archive only for prior findings and resolution context.

---

## Icebox

Not strictly needed, ones that require more design, or simply weird thoughts that
came to mind. Should be ignored when planning for implementation, in most cases.
Audit status is from 2026-08-05.

| Task | Reason / Audit Status |
|------|-----------------------|
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. Unchanged. |
| **Export performance: pre-compiled plot closures** | Only matters for many plot actors or heavy sampled fields. Unchanged. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. Unchanged. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. Unchanged. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. Unchanged. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. Unchanged. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. Unchanged. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. Unchanged. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. Unchanged. |
| **Auto-sorted property registry** | Keep manually sorted with `registry_is_sorted` guard; proc-macro adds more maintenance surface than it removes. Unchanged. |
| **Interactive step control (presentational mode)** | Manim-style `wait()` / `next_slide()`. Architecturally incompatible with Animatix's declarative deterministic playback model. GUI scrubbing covers most use cases. Unchanged. |
| **Auto-arrow routing / smart connector layout** | Actor anchor-point endpoint refs (`from: n0.right`, `to: n1.left`) cover manual auto-tracking. Remaining value is automatic edge routing/relayout, still niche. |
| **Per-actor exit before scene transition** | Animate individual actors out before `play SceneName [fade, ...]`. Workaround: `fade-out` actions timed at scene end. Transition blending is already uniform. Unchanged. |
| **Full `typst_shorthand` (`$$...$$`) parser sync** | Known Batch-8 leftover. Requires tree-sitter external scanner (C) changes, not just grammar edits. Highlighting-only impact today (PEG parser handles `$$...$$` correctly). Pull into a batch only after a scanner spike. Unchanged. |
