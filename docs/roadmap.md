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

Track solo shipped 2026-09-14 as an authored property rather than an
ephemeral view toggle: `solo: true` hides every other subtree recursively in
preview and export alike (see `docs/spec.md`), and the timeline track header
gained a solo button that edits the source through the normal property-edit
pipeline, so it is undoable and persists in the `.amx`.

Multi-actor curves shipped 2026-09-14: the Curves panel overlays every selected
actor, qualifying labels as `actor · property.X` so legend keys and widget ids
stay unique, shading later actors' canonical channel colours deterministically,
and committing each drag against the track that owns the keyframe. The value
window already derives from the visible curves, so it fits whatever the legend
leaves on. The Inspector's read-only graph editor still shows one actor.

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

## Planned Features

| Feature | Notes | Status |
|---|---|---|
| Build-time warning for unknown declaration properties | A typo'd property (`colour:`) is surfaced only by `animatix check` as an info diagnostic; the build and render paths say nothing and the actor simply draws without it. The engine can tell the cases apart (registry hit vs extension property vs plot parameter), so this wants a real build-time warning alongside the analyzer/LSP work. | Not started |
| `Code` custom highlighting theme | Token colours now follow the active colorscheme via a role→named-token palette (`ResolvedColorscheme::highlight_palette`, resolved at build). A fully custom palette (arbitrary per-role colours, or a user-supplied `.tmTheme`) is not exposed as an authoring surface; deferred until a real content need. | Not started |

## Dependency & Verification Constraints

| Item | What it is | Status |
|---|---|---|
| Vello pin lift | The workspace pins Vello to `d8686d52` because upstream #1558 (image-atlas residency) makes an image-bearing render draw nothing when a non-image render runs between two image renders — our multi-scope filter shape hits it on every frame. Lift the pin when upstream ships a fix, then re-run the dependency-boundary matrix (`crates/animatix-render/tests/vello_img_probe.rs`) and `animatix video dogfood/projects/effects-wave1/entry.amx`, confirming the backdrop survives every frame. The invariant this protects is documented on `RendererCore`. | Pinned, guard in place |
| Fewer Vello renders per frame | A frame currently renders the canvas once per `Filter` scope plus once for the main scene. Identity chains already skip the offscreen round-trip; further reduction means merging scopes that carry no effects. | **Not scheduled** — small payoff, medium risk (the multi-render sequence is exactly what the pin above protects) |
| GUI panel-level screenshot regression | The headless screenshot driver (`ANIMATIX_SCREENSHOT`, `ANIMATIX_SCREENSHOT_SIZE`) reviews a single, non-interactive app state. The diagnostics peek, the export dialog, and multi-actor curves were verified by compile + unit tests only, because `--demo-script` can only play/pause/scrub. Extending its grammar to switch tabs / open dialogs would make those surfaces screenshot-testable. | Not started |

## Planned Effects

Wave 1 (`Sharpen`, `Vignette`, `MotionBlur`, `Grain`, `Levels`) shipped
2026-09-13, wave 2 (`Duotone`, `Posterize`, `Edge`, `LensDistortion`,
`DropShadow`) shipped 2026-09-14 — see `docs/history.md` for both.

`Bloom`, soft `DropShadow`, and a generic chain `Mix` are blocked on the
second-input-texture ABI bump (`docs/effects.md` §4.1) and are **not**
scheduled: the ping-pong chain overwrites the original after the first pass,
so an add-back has no source. `Mix` additionally needs named intermediate
chain outputs — a separate chain-model change, not part of that bump.
