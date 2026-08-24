# Phase 2 Handoff — Demo Gallery

## Status

- **Branch**: `feat/demo-gallery-p2` at `/home/xiayuxuan/Documents/animatix-phase2`,
  9 commits ahead of `main` (fast-forward merge ready; main has not moved).
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

## Remaining known issues (candidates for Phase 3)

1. **`Mask` clip semantics** — the engine-fixed Mask clips children to the
   Mask's own `size` rect at the Mask's position (the clip layer used to be
   pushed at the scene origin, hiding every child of any mask not at the
   top-left corner — fixed). A `clip_shape` child is still just a rendered
   child, not the clip geometry: implementing "clip takes the clip_shape
   child's shape/size (and hides it)" is the remaining piece.
2. **Hosted-plot size convention** — `{graph}_size` is stored as half-size but
   consumed as full-size by bars/curves/`.map()`, so a plot hosted in a Graph
   occupies only the central half of the axis box. Needs a convention decision
   (touches several call sites + GUI inspector).
3. **Silent fallback on failed property expressions** — a name that fails to
   resolve in a property expression (e.g. `theme.text_md` when the module was
   imported without `as theme`) falls back to defaults with **no diagnostic**.
   There should be a warning per the never-silently-drop rule.
4. **Invalid easing names fall back silently** — `ease: bounce-out` (the
   canonical names have no directional suffix) is accepted without a warning.
5. **LSP/GUI don't call `Analyzer::merge_import_symbols`** — the CLI check/lint
   paths now resolve imported symbols for diagnostics; wiring the same call
   into the LSP/GUI analyzers would fix the editor experience too.
6. **`gap` not registered for BarChart** in the runtime property registry
   (the builder parses it, so charts work; the GUI inspector just won't list
   it). The registry is keyed by property name and `gap` is owned by the
   `ContainerLayoutGroup` schema — exposing it for BarChart needs either a
   per-actor schema variant or group-handler support, not a one-line change.
   Invalid easing names DO warn now (the parser no longer consumes
   unresolvable `ease:` modifiers), and the CLI check/lsp/gui analyzers
   resolve imported symbols.

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

- All pre-commit gates pass (fmt, `cargo check --workspace`, syntax 213,
  animatix lib 706, serially).
- No generated PNGs are committed; smoke outputs are disposable.
- `cog commit` cannot open a linked worktree's `.git` file — commits on this
  branch used `git commit -m "type(scope): ..."` per AGENTS.md fallback (each
  message says so).
- Keep using `nix develop` for workspace checks and renders (software Vulkan
  via lavapipe; a bare GPU adapter is unavailable outside the shell).
- Merge `feat/demo-gallery-p2` back to `main` when ready (fast-forward).
