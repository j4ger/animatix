# Roadmap Audit: 2026-08-05

Archived audit context for future sessions. The current remaining work lives in
`docs/roadmap.md` under `Active Work`; this file preserves the full audit trail,
including items that were resolved during the roadmap cleanup.

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
