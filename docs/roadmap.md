# Animatix Roadmap

Canonical source of truth for **remaining** work. Completed, resolved, and
closed work lives in [`history.md`](history.md) — do not re-add it here.

---

## Backlog & Prioritization

### Demo Gallery Redesign (active)

Source of truth: `docs/demo_gallery_plan.md`. Work happens on a short-lived git
worktree off `main` (e.g. `feat/demo-gallery`) and is merged back when a phase
lands.

| Phase | Deliverable | Status | Acceptance | Known Blockers / Notes |
|---|---|---|---|---|
| 1 | Shared `lib/` design system + `theme_studio.amx` | **Done** | clean `check`; PNG render smoke | Engine workarounds documented in plan: wrap positioned components in `Group`; wrap Text in `Group` inside Col |
| 2 | `motion_poster.amx` + `dashboard_story.amx` | **Done** 2026-08-24 (merged) | clean `check`; PNG smoke of every scene | Engine fixes landed with it — see `docs/handoff_phase2.md` |
| 3 | `epicycles.amx` + `sorting_theatre.amx` | **Done** 2026-08-25 (merged) | clean `check`; 3-frame PNG smoke | Epicycles wave-reveal polish noted but merged; `sorting_theatre` uses `dynamic_layout` + build-time sort precomputation + `swap` actions |
| 4 | `brand_reel/` capstone | **Done** 2026-08-25 (merged) | all six `play` transitions ≥1×; `persist`; Audio; cross-file scenes | Multi-scene zero-duration bug fixed; cross-file slot fills / component-instance positioning workarounds landing with it |
| 5 | Tutorial refurbishment + README matrix + `scripts/check_examples.sh` smoke | **Done** 2026-08-25 | script green; render smoke covers all examples | Reuses new `lib/`; `animation/16_showcase.amx` and `composition/20_feature_reel.amx` are superseded by the gallery |

### GUI UX Redesign (active)

Source of truth: `docs/gui_design_language.md` §12 (diagnosis, target
information architecture, per-surface redesign, open decisions). Work
happens on the short-lived `feat/gui-redesign` worktree off `main`.

Phase 0 shipped 2026-09-11: visible auto-key (default off), caret-anchored
completion, click-latched Delete scope, layout-preserving Inspector toggle,
explicit keyframe-diamond model, labeled scene inspector, platform-aware
shortcut display, action-identified Layers menu, remapped tool keys
(`V/A/R/S/G/P`), and real group scale/rotate.

Layout phase shipped 2026-09-11 (screenshot-verified): Inspector and Code
share one right-hand tab group; region sizes are `clamp(ratio × available,
min, max)` with a per-frame pixel-bound pass and a 120px tile floor; Animate /
Code / Inspect / Focus presets plus Reset layout; sidebar merged 6 → 3 labeled
tabs (Project / Outline / Library) with the editor promoted into the detail
region; `pill_tab_bar` degrades label-first.

Bottom tab group shipped 2026-09-11: Timeline and Curves share one bottom tab
group (Timeline active by default). The Curves tab is an interactive F-curve
editor over the selected actor — horizontal drag retimes through the batched
`MoveKeyframes`, vertical drag rewrites a keyframe value through the new exact
`SetKeyframeValue` command, right-click sets easing, click/Shift+click selects
(the selection is shared with the timeline), and a ruler scrubs the playhead.
Reachable from the toolbar, command palette, and `ViewAction::ShowCurves` /
`ShowTimeline`.

Later phases shipped 2026-09-11: timeline keyframe model (property-granular
`KeyframeId` selection, multi-keyframe drag with a single undo step via batched
`MoveKeyframes`, empty property lanes from the animatable-property registry,
actor track header eye/lock); autosave to a `<file>.amx.autosave` sidecar with
a Recover/Discard prompt on startup; in-editor find-match highlighting; Library
drag-to-place onto the canvas; narrow-window compact mode (48px sidebar icon
rail + overlay drawers). The `eparts` library got a batch of fixes (scrolling
virtualized `List`/`Tree`, themed `Select` with keyboard nav and a focus ring,
overlay-aware `Dialog`, dead slots wired, new per-component slot groups, a
macro-generated partial theme layer, and `Theme` grouped into
`palette`/`components`/`elevation`).

Remaining:

| Item | Scope | Status |
|---|---|---|
| Diagnostics peek | A transient overlay from the status-bar chip (today the chip toggles the existing bottom panel) | Not started |
| Track solo | A `ToggleActorSolo` command + muted state (eye/lock shipped) | Not started |
| Multi-actor curves | The Curves editor edits the first selected actor only | Not started |
| Export/settings polish | Detailed error text, codec/quality controls, restore-defaults | Not started |
| main rebase | `main` moved to `9cbb2dfa` (`actor_type` is now `String`); rebase + adapt before merging | Not started |

---

## Planned Effects

Wave 1 (`Sharpen`, `Vignette`, `MotionBlur`, `Grain`, `Levels`) shipped
2026-09-13 — see `docs/history.md` ("Built-in effects, wave 1"). Remaining
follow-ups, cheapest first:

| Effect | Notes |
|---|---|
| `Duotone` | Two-colour map on luma; zero-neighborhood, single pass |
| `DropShadow` (hard) | Callout/panel elevation; needs a second pass over alpha only |
| `Edge` | Sobel magnitude; zero extra inputs, useful for sketch styles |
| `Posterize` | Level quantisation; trivial single pass |
| `LensDistortion` | Barrel/pincushion UV warp through the linear sampler |

`Bloom` / soft `DropShadow` / a generic chain `Mix` wait on the
second-input-texture ABI bump (`docs/effects.md` §4.1).
`Edge`, `Posterize`, `LensDistortion`. `Bloom` / soft `DropShadow` / a generic
chain `Mix` wait on the second-input-texture ABI bump (`docs/effects.md` §4.1).
