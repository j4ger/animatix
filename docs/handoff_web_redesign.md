# Handoff — Web Demo Redesign ("the instrument")

> **STATUS: COMPLETE (2026-10-02).** Every item in "Remaining work" landed:
> the engine fix shipped with the perf-bench guard (`a4ce11f7`), the serial
> review loop took all scene groups to measured PASS (tour `fa345ecd`,
> transformer `b5654c2e`, gradient/matrix/hash `3328b581`, epicycles/sorting
> `efd5071d` — that group turned out never to have registered the ink scheme),
> the page-level pass fixed the nav-assembly regression, assets and docs are
> updated, and the full-workspace checks ran green. The story is told in
> `docs/history.md`, "The instrument redesign: web/ as a living timeline".
> This file is kept as the design-direction reference (locked palette, chrome
> spec, review pipeline).

*2026-10-02 · state of the working tree at `9f449f5` + staged polish (see
"Commits" below). This document is the pickup point for finishing the
redesign; the design direction was approved by the repo owner.*

## Objective

Rebuild the Animatix web demo (`web/`) away from the safe "light editorial"
skin into the approved direction: **the page itself is an Animatix timeline**
— dark "ink" ground shared with the scenes, amber `#f5b942` brand accent,
timeline furniture everywhere (scroll playhead, keyframe stamps), live scenes
instead of posters, and a production-polish pass over every scene's keyframes
reviewed by subagents serially (concurrency 1).

## Design system (locked)

- **Palette** (`web/demos/lib/theme.amx`, scheme `ink`, extends
  `editorial-dark`): scene background `#0b0e14` == site `--bg` (plates melt
  into the page), text.primary bone `#ece7db` == site `--ink`,
  accent.primary amber `#f5b942` (brand lead), accent.warning coral `#ff8666`
  (drama voice, one-beat accents only), accent.success sage `#8fc7a3` and
  accent.info steel `#8ab4f8` rationed to genuine diagram semantics.
- **Site skin** (`web/site.css`): dark instrument look, serif display voice
  kept (Charter stack). Contrast checked: ink 13.9:1, dim 7.3:1, faint
  8a8578 5.2:1, amber 10.9:1 on bg.
- **Timeline chrome** (`web/site-chrome.js`): nav ruler (scroll = playhead,
  scaleX fill + diamond head + one tick per section), `#12.4s` timecode chip,
  per-section keyframe stamps (`@ #8.0s`, BEAT = 4s/section), ledger rows
  stamp in via IntersectionObserver, `amx-player[data-hoverplay]` cards play
  on hover/focus, `.duality` figure scrubs with scroll (code lines carry
  `data-t="start,end"` and light up per beat), gallery/theater via
  Fullscreen API.

## Player API additions (`web/embed/src/amx-player.js`, minified bundle rebuilt)

- `sealed` attribute — page-driven mode: no center play button, no canvas
  gestures; with `autoplay` it resumes automatically on re-entering the
  viewport.
- `fit="cover"` — stage unlocks from its aspect box, canvas crops to fill
  (full-bleed hero). Default `contain` unchanged.
- `amxready` event (bubbles) + `duration` / `time` getters.
- `play()` from the poster/finished position restarts at 0 (loops begin with
  the build-up, not the hold).

## Engine fix (uncommitted — see "In flight")

**Bug**: a same-label re-declaration that changes actor type (the morph
syntax, e.g. `box: Rect` → `box: Ellipse [strategy: match]`) called
`set_identity` on the track, and `actor_type` is the frame path's primitive
registry key — so the target shape owned the *whole* timeline: the Rect drew
as a circle from birth. This falsified every cross-type morph on the site.

**Fix** (crates/animatix/src/timeline/dispatch.rs +
timeline/scene_eval.rs): `AnimationTrack::render_type_name(time_ms)`
resolves the primitive per frame from the `shape_type` track *inside the
vector-shape family only* (Rect/Ellipse/Line/Arrow/Polygon/Path) and only
when that track genuinely changes value; scene_eval's four render-path call
sites use the new `track_primitive_at(track, time_ms)`. Regression test
`rect_to_ellipse_redeclaration_keeps_rect_geometry_before_the_morph` in
timeline/tests/build.rs. All 762 lib tests green (serial run).

**Guard**: this touches the per-frame render path, so the commit must ship
with `scripts/perf-bench.sh compare` output. Baseline was saved on the
pre-fix tree (`target/perf/baseline`); the compare run was still executing at
handoff — see "In flight" for the exact finish steps.

## Scene review pipeline (how to continue the loop)

```bash
# 1. render keyframes (stamps + 0.45s, plus end frame) — cleans its own dir
bash /tmp/render_keyframes.sh web/<path>/scene.amx /tmp/frames   # script may need re-creating after reboot; see git history of this doc's source or rewrite: extracts '#N.Ns' stamps, renders via target/release/animatix image
# 2. contact sheet per scene (ffmpeg hstack/vstack, labels = frame times)
bash /tmp/contact.sh /tmp/frames/<name> /tmp/sheets/<name>.png
# 3. dispatch ONE general-purpose subagent per group (serial, concurrency 1)
#    prompt template: design-language recap + sheet paths + .amx paths +
#    "per-keyframe verdicts + per-scene PRODUCTION READY / NEEDS WORK +
#    prioritized .amx-level fixes with evidence"
# 4. apply fixes, re-render, re-dispatch verification until PASS
```

Review verdicts so far (first pass → second pass):

| group | first pass | after fixes |
|---|---|---|
| hero.amx + sample.amx | NEEDS WORK (engine morph bug, ball/bracket collision, hard loop cut, carriage desync, small sample) | **PASS / PASS** |
| tour A: syntax / timing / actions / morph | NEEDS WORK ×4 (tiny casts, no labels, palette misuse, hard loop cuts) | syntax **PASS**; timing/actions/morph fixed (labels aligned, orbit cleared, path_arc beats) — **re-verification dispatch pending** |

Fixes applied during tour A that generalize: casts scaled to ≥25% of frame
width, per-beat captions teach the concept on screen, finale colors unify on
amber, every scene gets a ~350ms reverse-order outro so loops wrap from a
quiet plate, coral is a one-beat accent (return to amber after the drama
beat).

## Two engine limitations discovered (worked around, documented)

1. **`strategy: fade` has no cross-fade for state shapes** (Rect, Ellipse,
   points-Polygons — anything without a `vector_paths` track): a fade
   re-declaration renders as a color lerp + one-frame silhouette swap. The
   tour now demos only auto / match / path_arc / stretch; the gap is
   recorded in `docs/roadmap.md` ("Morph: strategy: fade has no cross-fade…").
2. **`always`-block property reads are static**: `pulse.at = actor.at + (…)`
   evaluates `actor.at` at the *declared* position, not the animated one —
   only direct aliasing (`a.at = b.at`) tracks. tour/actions.amx therefore
   uses a fixed orbit centre sized to clear the actor's travel band.

## Commits so far

- `9f449f5` feat(animatix): relaunch the site as an instrument (skin, chrome,
  player additions, ink scheme, hero/sample scenes, homepage)
- `d677e224` feat(animatix): gallery theater mode; live scene posters on the
  demos hub; ink sweep across all scenes
- `b1fa7c21` feat(animatix): polish hero, sample and the four opening tour
  scenes for the ink system

## Remaining work, in order

1. **Finish the engine commit** (in flight — next section).
2. **Serial review loop, remaining groups** (same pipeline; sheets may need
   re-rendering after any scene edit):
   - tour B: effects, plots, reactive, components, scenes (tour/scenes/) —
     textmath.amx is a still (Typst needs the full profile), review as image.
   - transformer (demos/transformer/scenes/, 7 scenes): overview, tokens,
     positional, attention, multihead, feedforward, pipeline. attention.amx
     has careful accent semantics in its header comments — preserve them.
   - gradient (5), matrix (4), hash (4), epicycles (5), sorting (4).
   - Apply fixes → re-render → verification dispatch per group until PASS.
   - Known group-level risks: epicycles/sorting are long multi-scene
     documents (demos/*/scene.amx hold 3–5 scenes each — render sheet covers
     all stamps); `dynamic_layout: true` scenes may reflow differently under
     the new palette's text widths.
3. **Re-verify tour A** (timing/actions/morph) in the same dispatch as the
   first tour-B group if convenient — serial, one agent at a time.
4. **Page-level review**: serve `web/` (`python3 -m http.server` or
   `web/README.md`'s serve instructions) and screenshot pages in a WebGPU
   capable browser (hero, duality scrub, ruler/timecode, ledger stamp-in,
   gallery theater, demos hub hover-play). If headless WebGPU is
   unavailable, verify layout with scenes swapped for static poster images
   and check the no-WebGPU veil path. Also verify the 390px nav fit again
   (the old guard comments are in site.css).
5. **Assets & docs**:
   - regenerate `web/assets/og.png` from the new hero (ffmpeg crop 1200×630
     of a hero frame ~t4.5).
   - `web/README.md`: document `sealed`, `fit`, `amxready`, `duration`/
     `time`, hover-play, theater, the `ink` scheme and how scenes opt in.
   - `docs/history.md` entry summarizing the redesign (completed work moves
     out of roadmap; roadmap gains the fade-morph entry — already added).
6. **Final checks before pushing** (repo rules): `cargo fmt --all`,
   `cargo check --workspace --all-targets`, clippy `-D warnings`,
   `cargo test -p animatix-syntax`, `cargo test -p animatix --lib --
   --test-threads=1`, full workspace tests — all were green on the engine
   fix except the pending bench; re-run after any further engine edits.

## In flight at handoff time — perf compare intentionally skipped (owner's call)

The engine fix is **left uncommitted in the working tree** (3 files:
`crates/animatix/src/timeline/{dispatch.rs,scene_eval.rs,tests/build.rs}`),
because the repo rule requires hot-path commits to ship with a
`scripts/perf-bench.sh compare` result and the owner chose to skip the run
at wrap-up. The pre-fix baseline is already saved at `target/perf/baseline`.
First action next session:

```bash
nix develop -c bash scripts/perf-bench.sh compare   # ~10–25 min (criterion)
# then commit the three engine files as:
# fix(animatix): resolve the render primitive per frame across cross-type morphs
# …with the compare summary in the message body.
```

If compare flags a regression, suspect the `keyframes` scan inside
`render_type_name` (its fast path is two early returns: non-vector identity,
constant shape_type track); cache the "changes over time" verdict on the
track the way `vector_paths_epoch` memoizes path evaluation. The fix itself
is verified: regression test green, 762 lib tests green serially, and the
sample/hero renders show Rect-before / Ellipse-after as authored.

## Known pre-existing issues (not ours, do not fix in this scope)

- `demos/svg-probe/scene*.amx` fail `animatix check` with SVG asset
  load errors when run from a CWD other than their own — pre-existing
  (verified against HEAD); the browser path resolves assets relative to the
  scene and works.
- Probes (`demos/svg-probe`, `perf-probe`, `multi-probe`) got the ink sweep
  but no polish pass; they are engineering pages, deliberately left as-is.
