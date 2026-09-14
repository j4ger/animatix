# Animatix Roadmap

Canonical source of truth for **remaining** work. Completed, resolved, and
closed work lives in [`history.md`](history.md) — do not re-add it here.

---

## Backlog & Prioritization

### GUI UX Redesign (active)

Source of truth: `docs/gui_design_language.md` §12 (diagnosis, target
information architecture, per-surface redesign, open decisions). The
2026-09-11 phases shipped directly on `main`; the remaining items below are
small enough to land there too.

Export/settings polish shipped 2026-09-14: the failure tag keeps its short
label but reveals the full encoder message on hover with a copy action, the
dialog gained explicit `Encoder` (codec + libx264 preset) selectors for the
formats that honour them, and a reset control restores the default settings
while keeping the output path. CRF/bitrate controls remain unscheduled — they
need encoder-parameter plumbing in `animatix-render`, not just dialog state.

Diagnostics peek shipped 2026-09-14: the status-bar chip raises a transient
overlay anchored above itself (single click peeks, double click pins the docked
panel; Escape, a click outside, or picking a diagnostic closes it).

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
| Track solo | A `ToggleActorSolo` command + muted state (eye/lock shipped) | Not started |
| Multi-actor curves | The Curves editor edits the first selected actor only | Not started |

## Planned Effects

Wave 1 (`Sharpen`, `Vignette`, `MotionBlur`, `Grain`, `Levels`) shipped
2026-09-13, wave 2 (`Duotone`, `Posterize`, `Edge`, `LensDistortion`,
`DropShadow`) shipped 2026-09-14 — see `docs/history.md` for both.

`Bloom`, soft `DropShadow`, and a generic chain `Mix` are blocked on the
second-input-texture ABI bump (`docs/effects.md` §4.1) and are **not**
scheduled: the ping-pong chain overwrites the original after the first pass,
so an add-back has no source. `Mix` additionally needs named intermediate
chain outputs — a separate chain-model change, not part of that bump.
