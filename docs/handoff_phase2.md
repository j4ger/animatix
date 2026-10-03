# Phase 2 Handoff — Demo Gallery

> **Status: closed.** Every item this file left open has landed since it was
> written; nothing here is outstanding. Kept as the record of *why* each one was
> open and what closed it — see [§ Where each remaining item landed](#where-each-remaining-item-landed).
> Two later sessions superseded much of the surrounding state: the branch below
> was merged into `main` long ago, and the engine has since absorbed the
> extension/primitive rewrite, the easing table work, and the silent-drop round
> (`docs/handoff_silent_drops.md` is the current one to read next).

## Status

- **Branch**: `feat/demo-gallery-p2` — merged to `main` (fast-forward); the
  working copy referenced below (`/home/xiayuxuan/Documents/animatix-phase2`)
  no longer exists, but the commit hashes in this file resolve in this repo.
- **`dashboard_story.amx`**: complete, 5 scenes, smoke-rendered.
- **`motion_poster.amx`**: complete, 4 scenes (~28s), smoke-rendered at 2.5s /
  6.5s / 18s / 25s. Per-letter reveal, slogan cross-fade, morph strategy
  comparison (match / path_arc / stretch), ken-burns graded zoom, easing race.
- **Engine limitations from the previous handoff**: all six investigated.
  Four were real bugs and are **fixed on this branch**; two were misdiagnoses
  of documented behavior (details below). Several adjacent bugs were found and
  fixed along the way.

## Engine fixes on this branch (oldest first)

| Commit | Fix |
|---|---|
| `f79beb90` | `unknown-type` false warning for statement-position instances of imported `pub component`s (semantic lint only consulted builtin types; also accepts namespaced `alias.Component`) |
| `ff9fe429` | `draw-in` / `wipe-in` / `reveal-in` never lifted the hidden-by-default opacity seed, so targets stayed invisible forever — the real cause behind the old "Path renders blank" and "Filter with component children shows nothing" reports |
| `8c4d916d` | Static Filter properties (`blur:` etc.) were dropped silently; only assignment-driven values applied. Declaration-time values now seed the filter tracks |
| `3a0be20b` | `BarChart` (and other standalone plots) ignored `size:` — layout box was read from the pre-declaration track snapshot. Also wired `anchor:`/`offset:` through the plot dispatch path (previously silently dropped) |
| `f32b0187` | CLI `image`/`video`/`gif` now default their canvas to the file's `config { resolution: .. }`; root-level `Filter` no longer post-composites on top of later siblings |
| `8e244595` | Stroke-only `Path` (explicit `stroke:`, no `color:`) no longer emits the default scheme fill (vello implicitly closes open paths → dark dome); entrances reveal fill to the authored `fill_opacity` instead of hardcoded 1.0 |
| `646f4238` | Component instances forward `opacity:`/`at:`/`anchor:`/`offset:` onto the expanded root actor (previously dropped; Group wrapper was the workaround) |
| `71af4b0b` | Component-internal `always` / assignments / reactive bindings survive when the component is instantiated inside a container (previously silently dropped) |

## Misdiagnoses in the previous handoff (no code change needed)

1. **"Transparent Rect overlays are not blended"** — alpha blending is exact
   (verified by scene-encoding decode + GPU pixel math: `(0,0,0,0.6)` over
   content dims it to 40%). The "opaque black" frames were the hidden-by-default
   rule: content underneath had no entrance action, so only the overlay's
   contribution was visible over the dark background.
2. **"Component instances have default opacity 0 until an entrance runs"** —
   documented behavior for ALL actors declared before the first keyframe
   (`docs/spec.md` "Pre-Keyframe Actor Declarations"); component instances
   expand to plain actor declarations and follow the same rule. What *was* a
   bug: instance-level `opacity: 1` was dropped instead of making the instance
   visible — fixed in `646f4238`.
3. **"Filter with component children does not produce visible output"** — the
   filter backend is type-agnostic; the output was invisible because of the
   hidden-by-default rule above. The real filter bug was different: static
   `blur:` etc. never applied (fixed in `8c4d916d`).

## Where each remaining item landed

All six of the "candidates for Phase 3" are closed. 1–5 were done by later
sessions; 6 was the last one outstanding and is closed now. Evidence, in each
case a test that fails if the fix is undone rather than a claim:

| # | Item | Closed by | Evidence it is closed |
|---|---|---|---|
| 1 | `Mask` clip_shape defines the clip geometry and does not paint | `24da1f9b` (make `clip_shape` the clip source), `f35ca6eb` (round rect, `Text`/plot fallbacks), `b0e1f22e` + `12297136` (`Primitive::clip_path` capability, plot geometry excluded — ABI 8) | Five GPU tests in `crates/animatix-render/src/offscreen.rs`: `mask_clip_shape_ellipse_defines_clip_region`, `mask_clip_shape_polygon_defines_clip_region`, `mask_clip_shape_without_geometry_warns`, `mask_clip_shape_plot_falls_back_with_warning`, `mask_clips_children_at_mask_position`. All pass inside `nix develop` (software Vulkan). Documented in `docs/primitives.md` §Mask. |
| 2 | Hosted-plot size convention | Behaviour is FULL size everywhere now; stale "half-extent" comments corrected in `2a36a5bf` | `crates/animatix/src/timeline/tests/bar_chart.rs:370` `hosted_bar_chart_spans_graph_axis` measures the baked bar span against the graph's 800 px axis box; `crates/animatix-render/src/offscreen.rs:1457` `hosted_bar_chart_paints_bars_across_the_full_graph_axis` does the same at the pixel level (`3e312998`). Both pass. |
| 3 | Silent fallback on failed property expressions | multi-segment path failures report the full dotted path | `unknown-lookup-path` diagnostic; see `docs/history.md`. |
| 4 | Invalid easing names fall back silently | `4e8a607d` made the parser stop consuming unresolvable `ease:` modifiers; `328f5aef` collapsed the four easing tables into one | The build layer's `Unsupported ease value` warning fires; the GUI offers only names the registry knows. |
| 5 | LSP/GUI don't resolve imported symbols | `4e8a607d` wired `Analyzer::merge_import_symbols` into both; `eea24034` re-resolves on `didSave`, `a573683d` on watched-file changes | Call sites today: `animatix-gui/src/editor.rs:53` (once per opened buffer), `animatix-lsp/src/main.rs:134` (open), `:396` (save), `:431` (watched `.amx` change). The GUI's per-buffer cache intentionally does not re-resolve on every keystroke. |
| 6 | `gap` not registered for BarChart | This session — the row itself stayed as it was because **`Applicable` is already per-actor**: adding `BarChart` to the row was all it took | `crates/animatix/tests/applicability_table_agrees_with_reads.rs::bar_chart_gap_is_applicable_to_bar_chart` fails when `BarChart` is dropped from the row. See below for why the earlier "needs a per-actor schema variant" guess was wrong. |

### Why item 6 was one line after all

The old note predicted the registry could not expose `gap` for BarChart "without
a per-actor schema variant or group-handler support". It can, because the three
surfaces that care are all *derived* from the same `Applicable` value:

- `animatix_syntax::schema::property_specs()` filters each row by
  `applicable.includes(...)` and that single slice drives the GUI inspector's
  property list, plan-slot filtering, and the analyzer's `unknown-property`
  hint — one row, every consumer.
- The runtime binding (`binding!("gap", …ActorField::ContainerLayoutGroup…)`) is
  keyed by property *name*, not by actor type, and `ContainerLayoutGroup` has no
  track storage at all: `dispatch.rs:1025` returns `None` for it and the engine
  only uses it to say "cannot set 'layout' directly". So widening the row adds
  no storage and no new animated/assignable surface — `chart.gap = 10` still
  fails loudly with `unsupported-assignment-property`, exactly like
  `chart.bar_width = 6` always did.
- BarChart itself has never depended on the row: `build/plot.rs:2164` reads the
  prop straight from syntax, which is why charts always worked.

The row now reads `Actors(&["BarChart", "Col", "Grid", "Group", "Legend",
"Mask", "Row", "Stack"])`. Before it did, `animatix check` reported
`unknown-property: Property 'gap' not commonly used on BarChart (may still be
valid)` for a property both documented in `docs/primitives.md` §BarChart and
honoured at build time.

**Follow-up found while closing it (recorded in `docs/roadmap.md`):** the type
row for `gap` is `Type::Num`, so the documented string form `gap: "auto"` trips
`type-mismatch` even though the builder accepts `Expr::Str("auto")` — the bare
`gap: auto` form is clean. `bar_width` and `max_value` have the same mismatch
already, and `show_axis`/`show_labels` are typed `Bool` while their builders
accept `"true"`/`"false"`. Widening `gap`'s type row is not a free correctness
fix: it would also remove the only signal that a `Row` silently drops a
non-numeric `gap` (the container primitives read it with `if let Ok(Value::Num)`
and report nothing), so it is a decision rather than a patch.

## How to verify the current state

```bash
nix develop
cargo fmt --all
cargo check --workspace
cargo test -p animatix-syntax
cargo test -p animatix --lib -- --test-threads=1

cargo run --bin animatix -- check examples/gallery/dashboard_story.amx
cargo run --bin animatix -- check examples/gallery/motion_poster.amx

# Smoke-render a frame from each scene
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx --time 1.5  -o /tmp/dash_kpis.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx --time 4.5  -o /tmp/dash_trend.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx --time 6.5  -o /tmp/dash_ranking.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx --time 9.5  -o /tmp/dash_focus.png
cargo run --bin animatix -- image examples/gallery/dashboard_story.amx --time 12.0 -o /tmp/dash_end.png

cargo run --bin animatix -- image examples/gallery/motion_poster.amx --time 2.5  -o /tmp/mp_title.png
cargo run --bin animatix -- image examples/gallery/motion_poster.amx --time 6.5  -o /tmp/mp_morph.png
cargo run --bin animatix -- image examples/gallery/motion_poster.amx --time 18.0 -o /tmp/mp_kenburns.png
cargo run --bin animatix -- image examples/gallery/motion_poster.amx --time 25.0 -o /tmp/mp_easing.png
```

`image` now defaults to each file's `config { resolution: .. }`; pass
`--width/--height` to override.

Approximate motion_poster scene starts (global, with transitions):

| Scene    | Global start (s) | Suggested smoke time (s) |
|----------|------------------|--------------------------|
| Title    | 0.0              | 2.5                      |
| MorphLab | ~5.5             | 6.5                      |
| KenBurns | ~15              | 18.0                     |
| Easing   | ~22              | 25.0                     |

## Notes for the next session

- Gates from this session: `cargo fmt --all`,
  `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test -p animatix-syntax`, `cargo test -p animatix --lib` serially.
- No generated PNGs are committed; smoke outputs are disposable.
- `cog commit` works inside `nix develop` (cocogitto is on the shell `PATH`). It
  cannot, however, open a *linked worktree's* `.git` file — when committing on a
  branch checked out in another worktree, either commit from the main repo or use
  the `git commit -m "type(scope): ..."` fallback (and note it, per AGENTS.md).
- Keep using `nix develop` for workspace checks and renders (software Vulkan
  via lavapipe; a bare GPU adapter is unavailable outside the shell).
- ~~Merge `feat/demo-gallery-p2` back to `main`~~ done; `main` has moved a long
  way since (easing tables, the primitive/extension rewrite, the instrument
  redesign, the silent-drop round).
- Read `docs/handoff_silent_drops.md` next — it is the open one.
