# Phase 2 Handoff — Demo Gallery

## Status

- **Bug fix (multi-scene zero-duration clamping)**: completed and merged to `main`.
- **Phase 2 worktree**: `feat/demo-gallery-p2` at `/home/xiayuxuan/Documents/animatix-phase2`.
- **`dashboard_story.amx`**: implemented and `check`-clean. Five scenes render in the
  PNG smoke tests (see sample command below).
- **`motion_poster.amx`**: not started.

## What landed in this worktree

- `examples/gallery/dashboard_story.amx` — a 5-scene data-story demo:
  1. `KPIs` — `MetricCard` row with staggered fade-in and count-up text override.
  2. `Trend` — weekly bar chart built from `Rect`s inside a `Row`, with a
     coordinate `Callout` on the peak bar.
  3. `Ranking Shift` — 5 bars with `swap` and `reorder` actions.
  4. `Focus` — KPI row + highlighted insight card + takeaway `TitleCard`.
  5. `End` — closing title card.
- `examples/lib/charts.amx` — removed the unused `LineChart` component that was
  added experimentally; `ChartPanel` + `LegendItem` remain unchanged.
- Regression test `test_zero_duration_scene_does_not_collapse_composition` in
  `crates/animatix/src/composition/tests.rs` (already on `main`).

## Engine limitations discovered during Phase 2

These are real blockers for the originally planned visuals. They are **not**
regressions introduced by this work; they are pre-existing image-export/renderer
behaviors:

1. **`Path` actors do not render in `animatix image` export.**
   - Repro: any `Path` with `stroke:` and `stroke_width:` produces a blank frame.
   - Impact: cannot use `draw-in` on a hand-drawn line chart.
   - Workaround in dashboard: built the chart from `Rect` bars instead.

2. **`BarChart` `size:` / `at:` properties are ignored in `animatix image`
   export.**
   - Repro: `examples/data/26_data_math.amx` renders the bars as a tiny cluster.
   - Impact: cannot use the built-in `BarChart` for a large chart.
   - Workaround in dashboard: built the chart from `Rect` bars inside a `Row`.

3. **Transparent `Rect` overlays are not blended.**
   - Repro: a full-screen `Rect` with `color: (0,0,0,0.6)` renders as opaque
     black.
   - Impact: cannot dim the background to focus one card.
   - Workaround in dashboard: omitted the dim/blur effect; the focus card is
     simply layered over the KPI row.

4. **`Filter` actor with component children does not produce visible output**
   in the image-export path (even though `examples/animation/08_effects.amx`
   works with an `Image` child).
   - Impact: cannot blur the background behind a focus card.
   - Workaround: same as #3 — avoid `Filter` for this demo.

5. **Component instances have default opacity `0` until an entrance action runs.**
   - Any reused component (e.g. `MetricCard`) that appears in a later scene must
     be explicitly `fade-in`’d, even if it represents a carried/persistent
     background element.

6. **Top-level component instances produce `unknown-type` checker warnings**
   unless wrapped in a `Group`. Wrapping is already the documented workaround
   for `anchor`/`offset` on component instances.

## How to verify the current state

```bash
nix develop
cargo fmt --all
cargo check --workspace
cargo test -p animatix-syntax
cargo test -p animatix --lib -- --test-threads=1

cargo run --bin animatix -- check examples/gallery/dashboard_story.amx

# Smoke-render a frame from each scene
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx \
  --time 1.5 -o /tmp/dash_kpis.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx \
  --time 4.5 -o /tmp/dash_trend.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx \
  --time 6.5 -o /tmp/dash_ranking.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx \
  --time 9.5 -o /tmp/dash_focus.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx \
  --time 12.0 -o /tmp/dash_end.png
```

Approximate scene timings (with transitions):

| Scene | Global start (s) | Suggested smoke time (s) |
|-------|------------------|--------------------------|
| KPIs  | 0.0              | 1.5                      |
| Trend | ~2.5             | 4.5                      |
| Ranking Shift | ~5.6       | 6.5                      |
| Focus | ~8.7             | 9.5                      |
| End   | ~11.2            | 12.0                     |

## Remaining work for Phase 2

1. **Implement `motion_poster.amx`** per `docs/demo_gallery_plan.md` §G4:
   - per-character staggered entrance,
   - slogan morph / timed text cross-fade,
   - Path morph strategy comparison (blocked by `Path` rendering bug — may need
     to use `Polygon` or pre-rendered shapes),
   - background Image ken-burns inside a `Mask`,
   - easing family showcase.
   - **Risk**: the `Path` rendering bug means the morph/path strategy comparison
     will need a workaround (e.g. `Polygon` with `stroke`, or a different
     visual treatment).

2. **Polish `dashboard_story.amx`** if desired:
   - Replace the manual `Rect` bar chart with a real `PlotCurve`/`BarChart` once
     the renderer/export path supports it.
   - Re-add the background dim/blur focus effect once transparent overlays or
     `Filter` with component children work.

3. **Merge `feat/demo-gallery-p2` back to `main`** after `motion_poster.amx` is
   done (or merge now if you prefer smaller commits).

## Notes for the next session

- All pre-commit gates pass.
- No generated PNGs are committed; smoke outputs are disposable.
- Keep using `nix develop` for full workspace checks to avoid the `alsa-sys`
  pkg-config failure outside the shell.
