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
| Namespace access depth | Open; aliased imports expose one level (`alias.export_name` only). |
| Colorscheme dotted token parser | Open; runtime/API accepts `scene.background`-style overrides, source parser does not yet. |

---

## Audit: 2026-08-05

Findings and issues from the roadmap/doc audit. Items marked `Resolved` were
addressed by the cleanup that produced this version of the roadmap.

| ID | Finding / Issue | Status |
|----|-----------------|--------|
| ROADMAP-001 | Committed docs referenced the gitignored `.picopi/plans/` directory. | Resolved: plans moved to `docs/plans/` and references updated. |
| ROADMAP-002 | The committed roadmap omitted the active eparts framework track and GUI follow-ups. | Resolved: added `Active Work` above. |
| ROADMAP-003 | `docs/spec.md` marked GUI scene list/composition timeline pending even though the GUI and architecture docs describe it as shipped. | Resolved: spec status updated. |
| ROADMAP-004 | `docs/primitives.md` said `Image.url`/`Svg.url` assignment requires re-declaration; `Image.url` already keyframes. | Resolved: planned section updated to `Svg.url` only. |
| ROADMAP-005 | `docs/spec.md` said text/Typst/Code assignment does not recompile paths; assignment-time recompilation is implemented. | Resolved: spec gap updated. |
| ROADMAP-006 | `clippy.toml` referenced roadmap `P1.3`/`P1.4`, which no longer existed. | Resolved: config comments updated. |
| ROADMAP-007 | `docs/gui_design_language.md` listed Danger button and toolbar registry wiring as remaining after both shipped. | Resolved: remaining-work section updated. |
| ROADMAP-008 | `docs/architecture.md` still described aliased module import as future. | Resolved: wording updated. |
| ROADMAP-009 | Local planning docs were stale and some still said plans were not merged after implementation shipped. | Resolved: completed plans archived with a README explaining archive semantics. |
| ROADMAP-010 | Actor anchor points, runtime-indexed `always` targets, `match`, and `list_swap`/`list_set` shipped without spec coverage. | Resolved: spec sections added. |
| ROADMAP-011 | The icebox `Auto-arrow layout` item was partially addressed by actor anchor-point endpoint refs. | Resolved: icebox reason rewritten. |
| ROADMAP-012 | The spec media known-gap still said text/Typst/Code source assignment required re-declaration. | Resolved: known-gap wording updated. |
| ROADMAP-013 | GUI design doc §6.2 still said `ButtonVariant::Danger` was not exposed. | Resolved: button API and policy note updated. |
| ROADMAP-014 | The spec still referenced the removed `docs/roadmap.md §4.1`. | Resolved: cross-reference replaced with current roadmap notes. |
| ROADMAP-015 | Archived plans still contained `.picopi` path references. | Resolved: archive README explains historical semantics; direct references updated where practical. |
| ROADMAP-016 | No automated check keeps roadmap/doc references and status in sync. | Open: add a docs consistency check or link checker. |
| ROADMAP-017 | `Theme::light()` contrast is unverified. | Open: see GUI follow-ups. |
| ROADMAP-018 | `Svg.url` remains static/immediate on assignment. | Open: see language/runtime gaps. |
| ROADMAP-019 | Runtime object field writes are unsupported. | Open: see language/runtime gaps. |
| ROADMAP-020 | Precise shape/path/text bounds remain approximate. | Open: see language/runtime gaps. |
| ROADMAP-021 | The shortcut cheat sheet still hardcoded key labels instead of reading `ShortcutRegistry`. | Resolved: cheat sheet now renders registry-backed, platform-aware keys; gesture rows stay static. |
| ROADMAP-022 | The shortcut registry had duplicate tool bindings and used `V` for both Move and Vertex. | Resolved: tool shortcuts are unambiguous; `V` is Vertex, `M` is Move, `Shift+S` is Scale, `R` is Rotate, `P` is Pivot. |
| ROADMAP-023 | `docs/primitives.md` said `tick_labels` was not implemented, but Graph axis label rendering is shipped. | Resolved: property docs updated to the accepted string values. |
| ROADMAP-024 | Legend scene scanning and other known limitations were not tracked in the roadmap. | Open: added Legend auto-entry extraction, multi-target custom actions, namespace depth, and Colorscheme dotted parser to `Active Work`. |

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
