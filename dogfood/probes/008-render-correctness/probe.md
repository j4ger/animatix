# Probe 008 — Render Correctness

Per-primitive rendering audit: render each primitive on a neutral (black)
background at a known position/size/color, review the frame, and record whether
the output matches the intended geometry/style. Rendered PNGs are **not**
committed; use `scripts/audit_render.sh` to regenerate and inspect them.

## How to run

```bash
nix develop
scripts/audit_render.sh                    # renders every probe to /tmp/animatix-audit
```

Each demo is revealed with `#0s ... fade-in` and reviewed at `t=1.0`.

## Findings (2026-08-27)

### ✅ Correct
| Group | Result |
|---|---|
| Shapes (Rect/Ellipse/Line/Arrow/Polygon/Path) | All draw correctly: fill, stroke, arrowheads, closed-path fill. |
| Text EN + CJK + `text_max_width` wrap | Correct; explicit `text_max_width` is honored (does not collapse to 2-3 chars). |
| Code, Typst (via `content:`), Typst math `$..$` | Correct. |
| Image (natural size/offset), Svg | Correct. |
| Plots: Graph-hosted curves, BarChart (standalone `size`), NumberPlane, Heatmap | Correct; hosted plots span the full axis (central-half bug fixed). |
| Containers: Row/Col/Grid/Stack/Group | Correct alignment/gap/padding; Stack correctly overlaps & centers children. |
| Mask (clip at position + `clip_shape` geometry), Callout (coordinate + targeted), Legend | Correct. |
| Equation + Fragment (math `$..$` mode) | Correct. |

### 🐛 Fixed (this session)
| Issue | Root cause | Fix |
|---|---|---|
| **VectorField blank** when only `color:` is set | Arrows are drawn via `stroke` (`fill: None`), but `default_stroke_width(VectorField)` was `0.0`, so `stroke` became `None` → invisible. `color:` is used as the arrow stroke color. | Added `VectorField`/`ContourSet` to `default_stroke_width`'s non-zero arm (`crates/animatix/src/timeline/shapes/mod.rs`). Regression test `stroke_drawn_plots_get_a_default_width`. |
| **ContourSet blank** when only `color:` is set | Same root cause as VectorField (contours are stroke-drawn). | Same fix. |

> Both were latent in shipped examples (e.g. `data/07_plots.amx`'s VectorField)
> but never surfaced because `render_smoke.sh` only asserts the whole frame is
> non-blank (other elements carry it).

### ⚠ Notes (verify / human review)
| Issue | Note |
|---|---|
| Typst `text:` property silently renders blank | **Fixed 2026-08-27**: the uniform `text:` content property now feeds the content track for `Text`/`Code`/`Typst` (declarations_text.rs `content_matches`); canonical names (`content`/`code`) remain aliases. |
| Typst math with implicit multi-letter coefficients ("mc^2") | **Resolved 2026-08-28 — correct Typst semantics, error surfaced**: `$mc^2$`/`$E=mc^2$` fail because Typst parses `mc` as a single multi-letter *variable*, not `m*c` (a Typst math gotcha). This is correct behavior, not a bug. **Fix**: the compile error now surfaces Typst's real message ("unknown variable: mc" + hints instead of an opaque "failed to compile Typst document"). For a multi-letter product, write `$m c^2$` (spaces) or `$"mc"$`. |
| CJK `font_weight` bold may fall back to a regular face | Documented limitation (one representative face per family is loaded). |
| First-class `Math` primitive | **Added 2026-08-27**: `Math` renders Typst math without the `$...$` wrapper (compiles via `compile_math`); registered in the primitive registry, analyzer built-in types, and schema. |
| Filter `blur` not visibly applied in `animatix image` export | The GPU filter backend path looks correct (ping-pong WGSL blur passes), but `blur: 10` on a high-contrast source stays sharp. Smoke tests only assert `is_ok()` + size, never that content is actually blurred. Could be a software-Vulkan (lavapipe) compute limitation or a latent shader/pipeline bug. **See `dogfood/probes/009-filter-gpu-deferred` (open, human review).** Also note the Filter scene-eval branch silently falls back to unfiltered rendering when the backend is unavailable. |
| Equation fragment content with spaces around an operator (`" = "`) drops the operator | `#box()[ = ]` loses the `=`. Use `"="` without surrounding spaces, or trim content. |
| Stack `gap` is ignored | Documented (Stack is an overlap container); a `tracing::warn!` fires. |
| Legend auto-includes every color-bearing actor | Expected auto-scan; can be noisy on multi-actor scenes. |

## Open follow-ups
- Filter blur/fallback on software GPU (verify + decide whether the silent
  fallback should surface a diagnostic) — see probe 009.
- Equation fragment whitespace handling around operators.
- **Hosted BarChart bars overhang ~49px below the Graph's bottom axis**
  (2026-08-31): a render-level pixel scan shows the bars span the full axis
  width (fine), but their baseline paints at screen y≈399 while the Graph's
  bottom axis is at y≈350 — every bar sticks out below the chart box. Likely in
  the graph-hosted baseline math (`build_bar_chart_paths` uses the full height
  where the axis uses the half-height convention). Not fixed yet; a focused fix
  + pixel regression are needed.

### Resolved (2026-08-28)
- Default bundled "Open Sans" was a single-weight mock (no emphasis). **Replaced
  with four real static Open Sans faces** (Regular/Bold/Italic/BoldItalic,
  Apache-2.0, vendored in `crates/animatix/assets/fonts/` with SHA-256 provenance
  + `scripts/refresh-fonts.sh` integrity check). `DEFAULT_FONT_FAMILY` stays
  "Open Sans"; bold/italic/font_weight now work with the default family
  (pixel-verified: bold ink 0.385 vs regular 0.332; regression test
  `bundled_default_font_covers_bold_and_italic`). Static faces are kept rather
  than upstream variable fonts because they are smaller (≈850KB vs ≈1.1MB) and
  simpler; typst was upgraded to 0.15 which does support variable axes.
- **Render-level pixel-assertion tests** (2026-08-31): added
  `hosted_bar_chart_paints_bars_across_the_full_graph_axis`
  (`renderer/offscreen.rs`) — a rasterized-frame check that hosted BarChart
  bars span the full Graph axis (left third → right third, guarding the old
  central-half regression). Mask clip pixel tests already exist in the same
  module; the geometry-level `hosted_bar_chart_spans_graph_axis` remains a
  complement.
