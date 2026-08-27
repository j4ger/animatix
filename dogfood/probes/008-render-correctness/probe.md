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
| Typst `text:` property silently renders blank | `text:` is accepted by the property guard but does not feed the content track; use `content:`. Footgun worth flagging. |
| Typst markup emphasis `*bold*`/`_italic_` not applied | Content renders at regular weight with delimiters stripped. |
| CJK `font_weight` bold may fall back to a regular face | Documented limitation (one representative face per family is loaded). |
| Filter `blur` not visibly applied in `animatix image` export | Likely the GPU filter backend falling back to unfiltered rendering on software Vulkan (`scene_eval` has a silent fallback). Verify whether the filter works on real GPUs / whether the fallback should warn louder. |
| Equation fragment content with spaces around an operator (`" = "`) drops the operator | `#box()[ = ]` loses the `=`. Use `"="` without surrounding spaces, or trim content. |
| Stack `gap` is ignored | Documented (Stack is an overlap container); a `tracing::warn!` fires. |
| Legend auto-includes every color-bearing actor | Expected auto-scan; can be noisy on multi-actor scenes. |

## Open follow-ups
- Filter-blink/fallback on software GPU (verify + decide whether the silent
  fallback should surface a diagnostic).
- Typst `text:` alias should either work or be rejected with a clear error.
- Equation fragment whitespace handling around operators.
- Add a render-level pixel-assertion test for Mask clip / BarChart axis span /
  hosted-plot full-axis (currently verified visually; deterministic enough to
  assert programmatically).
