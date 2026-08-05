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
| Runtime-theme migration for custom GUI surfaces | Done; all custom GUI panels now read `eparts::theme(ui)` or theme-aware egui visuals instead of static generic token constants. App-specific palette roles (`category`, `timeline`, `canvas`, `curve`) remain intentionally fixed. |
| Replace raw font-size bypasses | Done; the last raw `FontId::new` in highlighting now uses `TextRole::Mono`, and numeric `.size()` bypasses are gone from GUI code. |
| Restore dev screenshot/visual regression harness | Done; `dev-screenshots` feature, `widget-screenshot` binary, and `scripts/gui-screenshots.sh` render bounded theme-aware PNGs. |
| Clean up dead timeline/panel scaffolding | Done; filter/layout property lane variants, unused `property_group_name`, timeline actor label/keyframe caches, and unwired view/panel command variants were removed. Remaining `#[allow(dead_code)]` entries have concrete forward-looking justifications. |
| Opportunistic eparts widget adoption | Open, not scheduled; migrate when the surrounding GUI area is next edited. |
| Verify `Theme::light()` contrast matrix | Open; needs WCAG AA verification for all text/background pairs. |

### Language and Runtime Gaps

| Item | Status / Notes |
|------|----------------|
| `Svg.url` source assignment | Open; assignment replaces SVG paths immediately/static. `Image.url` already animates with keyframes. |
| Runtime object field writes | Open; build-time variable-track writes like `let p = Point { x: 10 }; p.x = 30` are implemented, but `always` object field writes are not. |
| Precise shape/path/text bounds | Open; callout geometry and actor anchor points use world-space affine plus available local bounds. Exact text/path bounds remain deferred. |
| Text/Typst/Code frame-time content overrides | Open; timed assignments recompile glyph paths, but changing `text` directly inside `always` is not a supported path. |
| Legend automatic entry extraction | Open; legend uses placeholder entries, scene scanning for color-bearing actors is not implemented. |
| Custom action multi-target invocation | Open; custom component actions are single-target (`pulse btn, icon` is not supported). |
| Data-dependent algorithm timelines | Open; no runtime mutable state or branching timeline, so algorithm animations must be hand-unrolled recordings. Confirmed by `dogfood/projects/sorting-visualizer`. |
| Namespace access depth | Open; aliased imports expose one level (`alias.export_name` only). |
| Colorscheme dotted token parser | Open; runtime/API accepts `scene.background`-style overrides, source parser does not yet. |

### Process and Maintenance

| Item | Status / Notes |
|------|----------------|
| Roadmap/doc reference and status consistency check | Open; add a script or link checker so future audits do not rely on manual scans. |
| Dogfood scaffolding | Done; `dogfood/` contains project/probe templates and READMEs. First project (`sorting-visualizer`) completed; three findings fixed and remaining algorithm-timeline gap tracked above. |

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
| **Merge tree-walker and IR/VM into single execution engine** | Long-term, high-risk unification. Batch 6 (#12) extracted shared helpers so duplication is bounded. The dual-path-with-fallback is currently a *safety feature* (it makes closures non-critical). Needs a design spike before scheduling. Unchanged. |
| **Full `typst_shorthand` (`$$...$$`) parser sync** | Known Batch-8 leftover. Requires tree-sitter external scanner (C) changes, not just grammar edits. Highlighting-only impact today (PEG parser handles `$$...$$` correctly). Pull into a batch only after a scanner spike. Unchanged. |
