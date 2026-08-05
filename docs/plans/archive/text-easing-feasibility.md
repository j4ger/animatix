# Feasibility Assessment: Text Property Easing (Per-Character Morphing)

Status: Assessment only — no code changes.
Date: 2026-06-22

## Goal of the requested feature
Smooth per-character morphing for `text` content changes (`Text.text`, `Typst.content`), e.g. "Hello" → "World" letter-by-letter, via character-level diffing and staggered interpolation.

---

## Verdict

**Ship a simpler version.** True per-character content morphing is over-engineered relative to actual demand and to what competitors ship. The higher-value, cheaper work is (1) making the *already-written* Fade cross-fade actually render for content transitions (it is currently shadowed by a frame-time recompile path), and (2) adding a per-character *reveal* entrance action (`type-in`/`write-in`) that directly satisfies the explicit "MISSING" request in `examples/fft_explain.amx` and matches Manim's `Write`. Defer character-diff morphing until a concrete user asks for it.

---

## 1. Necessity

### How common is animated text content change?
Moderate, but the dominant patterns are **entrance reveal** and **value swap**, not letter-by-letter morphing:

- `examples/09_components.amx` — `card1.value_text.text = "$102K" [800ms]` (dashboard value swap). Wants a cross-fade, not a letter morph.
- `examples/06_reactive.amx` — `status.text = "TRACKING — keyframe active"` (state label swap). Same: cross-fade suffices.
- `examples/fft_explain.amx` — explicitly comments `// MISSING: draw-in or type-on effect for text would be nice` on titles. This is a **reveal/typewriter** request, not a content-morph request.
- `examples/05_morph.amx` — morphs *shapes*, not text. Text morph strategies are not exercised by any example.

No example asks for "Hello" → "World" letter-by-letter. The two real asks are cross-fade for value swaps and type-on for titles.

### What do competitors do?
- **Manim**: `Transform` morphs **point arrays** (vectorized), not strings. `TransformMatchingTex`/`TransformMatchingShapes` match by TeX substring / sub-shape and morph point arrays. `Write` is a **stroke-progress reveal**. Manim does **not** diff strings into per-character interpolation. Animatix already ships the point-array equivalent (`Auto`/`Match`/`Nearest` in `timeline/morph.rs`).
- **After Effects**: text animators do per-character/word/line **transform** animation (opacity, position, scale, rotation, color) via range selectors — the gold standard for "kinetic typography." AE does **not** morph glyph outlines per character for content changes; content changes are cuts or separate layers.
- **Motion Canvas / Remotion**: per-character effects are done **manually** by mapping over characters with staggered delays. No built-in content morph.

**Per-character content morphing (string diff → staggered glyph interpolation) is essentially not what competitors do.** The common features are (a) point-array morph (already shipped) and (b) per-character reveal/transform (not shipped).

### Is the current workaround sufficient?
The roadmap's workaround ("multiple overlapping actors with staggered fade-in/out") works but is verbose for value swaps and doesn't help with type-on titles. More importantly, the roadmap's premise that "text path arrays support Fade morph strategy (cross-fade via opacity)" is **not actually true at render time** — see §2.

---

## 2. Practicality — what the code actually does today

### Pipeline (traced)
1. `Text`/`Typst`/`Code` actors carry two relevant tracks: `text.text_content: PropertyTrack<String>` and `text.text_paths: PropertyTrack<Vec<TextPath>>` (`timeline/animation_track.rs:250`, `timeline/dispatch.rs:81`).
2. On `actor.text = "new" [duration]`, `recompile_text_at_assignment` (`timeline/assignments.rs:556`) writes:
   - `text_content` keyframes: `t_start` → old string, `t_end` → new string.
   - `text_paths` keyframes: `t_start` → `evaluate_text_paths(t_start)` (old compiled glyphs), `t_end` → newly compiled glyphs.
   - `morph_options` keyframe at `t_end` (if supported).
3. At frame time, `scene_eval.rs:451` always constructs a `TextCompileCtx` and calls `Primitive::evaluate`, which for text calls `primitives::mod::evaluate_text_paths` (`primitives/mod.rs:55`).
4. That function reads `content = text_content.get(time_ms)` and, **if non-empty, recompiles the string from scratch** via `text_ctx.text_compiler.compile(...)` and returns those fresh paths directly. It only falls back to the cached `text_paths` track + morph when `content` is empty.

### Critical finding: the Fade strategy is shadowed at render time
Two facts combine to defeat content cross-fading today:

- **`String` interpolates by stepping at `t < 0.5`** (`timeline/property_track.rs:115`): old string until the midpoint, new string from the midpoint onward.
- **The frame-time recompile path always fires** because `text_content` is always populated for Text/Typst/Code (set at declaration in `declarations_text.rs:401` and at reassignment in `assignments.rs:568`). Since content is never empty, `primitives::mod::evaluate_text_paths` takes the `compile(&content, …)` branch and returns freshly compiled glyphs — **bypassing `text_paths` and its `MorphOptions` entirely**.

Net effect: an animated text content change today renders the **old text at full opacity until the midpoint, then hard-cuts to the new text**. The `Fade`/`Auto`/`Match`/`Nearest` strategies in `morph::interpolate_text_paths` only influence (a) the seed value `start_val = track.evaluate_text_paths(t_start)` used to keyframe `text_paths`, and (b) the no-compiler fallback. They do **not** affect rendered output for content transitions. The roadmap's premise ("text path arrays support Fade morph strategy (cross-fade via opacity)") holds in the unit-test sense (`morph.rs` tests pass) but not in the rendered scene.

### Why the recompile path exists (and why this is a real tension)
The recompile path is **correct and necessary for typography animation**: animating `font_size`, `font_family`, `font_weight`, `letter_spacing`, `color`, etc. cannot be expressed by morphing cached glyph paths — the glyphs must be re-laid-out per frame. So the fix cannot be "just delete the recompile path." It must distinguish **content is changing** (use cached `text_paths` + morph) from **typography is changing** (recompile). That distinction is the actual core of "text property easing," and it is smaller than per-character diffing.

### What per-character content morphing would require
- **Character identity on glyphs**: `TextPath` (`renderer/types.rs`) currently has only `{ path, color, opacity }` — no source-character index. `extract_glyphs_with_metrics` (`renderer/text.rs:675`) and the fast path (`compile_text_fast`, `renderer/text.rs:983`) emit flat glyph lists. You'd need to enrich emitted glyphs with a char index/range, or re-derive alignment at compile time.
- **A diff algorithm** (LCS/Levenshtein) between old and new strings, run at build time in `recompile_text_at_assignment`, producing per-character insert/delete/match operations.
- **Staggered per-character interpolation**: each character needs its own sub-interval within `[t_start, t_end]`; matched chars morph their glyph Bezier paths, deletions fade out, insertions fade in. This is a new branch in `interpolate_text_paths` (`timeline/morph.rs`) and a new `MorphStrategy` (e.g. `Chars`).
- **Does not generalize to Typst**: compiled Typst output (math equations, ligatures, combining marks, multi-glyph characters) has no well-defined character→glyph mapping. Per-char morphing would be **plain `Text` only**, not `Typst`/`Math`/`Code` — yet the roadmap names `Typst.content` explicitly.
- **Parser/AST/analyzer/grammar/inspector surface**: a new `strategy: chars` keyword plus stagger params (`stagger`, `char_delay`) touches the parser, AST, `animatix-analyzer`, `tree-sitter-animatix`, and the GUI inspector — the standard 4-crate + grammar blast radius.

### Font/size/color changes during transitions
Already handled by separate tracks (`font_size`, `color`, etc.) and the recompile path. Per-char morphing would have to compose with these, adding another interaction edge.

---

## 3. Complexity estimate

### Full per-character content morphing (the roadmap item)
- **Files**: ~8–10 across 4 crates + grammar.
  - `renderer/types.rs` (TextPath char metadata), `renderer/text.rs` (emit per-char grouping; already have `extract_glyphs_grouped` as a partial precedent).
  - `timeline/morph.rs` (new `Chars` strategy + diff + staggered interpolation), `timeline/property_track.rs` or `animation_track.rs` (stagger config), `timeline/assignments.rs` + `timeline/declarations_text.rs` (diff at build time; stop shadowing morph for content changes).
  - `primitives/mod.rs` (precedence fix), `timeline/dispatch.rs`.
  - Parser/AST, `crates/animatix-analyzer`, `tree-sitter-animatix`, GUI inspector.
- **New data structures**: per-glyph char-index metadata; a char-diff op list (Match/Insert/Delete); staggered per-char timing model.
- **Parser/AST**: yes — new keyword + params.
- **Interaction with `MorphOptions`**: extends the existing `MorphStrategy` enum; reuses `MorphOptions` plumbing (`evaluate_paths_with_options`).
- **Performance**: build-time diff is O(n·m), negligible. Per-frame cost is O(n_chars) path morphs — fine for titles/labels, not for paragraphs. Acceptable for the target use case but another reason to scope to short strings.
- **Correctness risk**: high for Typst/math (ill-defined char mapping) and for Unicode (grapheme clusters, combining marks, ligatures).

### Simpler alternatives (see §4)
- Phase 1 (cross-fade actually renders): ~3–4 files, no parser changes.
- Phase 2 (`type-in` reveal action): ~4–6 files, mostly within `timeline/actions/` (precedent: `draw-in`/`reveal-in` in `actions/reveal.rs`).

---

## 4. Alternatives

- **A. Fix the shadowing + make Fade cross-fade render for content changes.** Distinguish content-change from typography-change in `primitives::mod::evaluate_text_paths` so that during a content transition the cached `text_paths` + `Fade` strategy is used (cross-fade old/new glyph arrays by opacity), while typography changes still recompile. The Fade code already exists and is tested (`morph.rs::interpolate_text_paths` `Fade` branch). Tradeoffs: + removes the hard-cut that currently surprises users; + tiny surface; − requires a careful precedence rule (content-change vs typography-change) and a decision on what happens when *both* change in one transition. Effort: S–M.
- **B. Typewriter / per-character reveal entrance action (`type-in`/`write-in`).** Entrance effect: characters appear one-by-one (opacity or clip reveal, optionally with stagger). Directly addresses `fft_explain.amx`'s "MISSING: type-on effect for text" and matches Manim `Write`. Reuses per-glyph opacity; **no string diffing**. Tradeoffs: + high demand, clear precedent (`draw-in`/`reveal-in` already exist for vector paths in `actions/reveal.rs`), no parser grammar changes beyond a new action verb; − currently `reveal.rs` *rejects* text targets (`reveal_out_reports_unsupported_text_targets`), so text-path reveal needs its own implementation. Effort: M.
- **C. AE-style per-character transform animator** (staggered fade/slide/scale/rotation per char, independent of content change). The actually-common "kinetic typography" feature. Tradeoffs: + the most generally useful text-animation primitive; − bigger surface (per-char transform model, range selector, parser params); separable from content morphing. Effort: M–L.
- **D. Word/line-level transitions.** Coarser grouping, much simpler than char-level. Tradeoffs: + cheap; − rarely what users want for short labels. Effort: S.
- **E. Full per-character content morphing (the roadmap item).** Tradeoffs: − over-engineered vs. demand and competitors; − doesn't generalize to Typst/math; − largest blast radius; + the only option that produces "Hello"→"World" letter morphs. Effort: L.

### Is per-character morphing over-engineered?
Yes, for the current evidence base. It targets a transition style no competitor implements via string diffing, no example requests, and it can't cover `Typst`/`Math` (which the roadmap explicitly names). The demand it would satisfy is better met by A (cross-fade) + B (type-on reveal) at a fraction of the cost.

---

## 5. Recommendation

**Ship a simpler version**, sequenced:

1. **Phase 1 — Make Fade cross-fade actually render for content transitions (highest ROI, cheapest).**
   Resolve the `primitives::mod::evaluate_text_paths` precedence so a content-only change uses the cached `text_paths` + `Fade` strategy (cross-fade) instead of hard-cutting via the recompile path. Keep the recompile path for typography changes (`font_size`/`font_family`/`color`/spacing). This turns the current surprise hard-cut into a cross-fade using code that already exists and is tested. ~3–4 files, no parser/grammar changes.

2. **Phase 2 — `type-in`/`write-in` per-character reveal entrance action.**
   Add a text-targeting reveal action alongside `draw-in`/`reveal-in` (`timeline/actions/reveal.rs`), using per-glyph opacity/clip with stagger. Directly satisfies `fft_explain.amx`'s explicit "MISSING" note and gives Manim-`Write` parity. ~4–6 files.

3. **Defer — True per-character content morphing (string diff + staggered glyph interpolation).**
   Over-engineered relative to demand and competitor behavior; doesn't generalize to Typst/math; largest blast radius (4 crates + grammar). Revisit only if a concrete user request for letter-by-letter content morphing appears. Until then, the workaround (overlapping actors) plus Phase 1's cross-fade covers the realistic cases.

### Supporting evidence
- `timeline/property_track.rs:115` — `String` steps at `t<0.5` (snap).
- `primitives/mod.rs:55-140` — frame-time recompile from `text_content` shadows `text_paths` morph whenever content is non-empty.
- `timeline/assignments.rs:556-620` and `timeline/declarations_text.rs:401-560` — `text_content` is always populated, and `text_paths` keyframes + `morph_options` are written but shadowed at render.
- `timeline/morph.rs::interpolate_text_paths` — `Fade` cross-fade is implemented and unit-tested, but inert for rendered content transitions per above.
- `renderer/text.rs:675` (`extract_glyphs_with_metrics`), `renderer/types.rs` (`TextPath` has no char identity) — per-char morph needs new metadata.
- `examples/fft_explain.amx` — explicit `// MISSING: draw-in or type-on effect for text would be nice`.
- `examples/09_components.amx`, `examples/06_reactive.amx` — real content-swap use cases wanting cross-fade, not letter morph.
- `timeline/actions/reveal.rs` — `draw-in`/`reveal-in` precedent and the text-target rejection test (`reveal_out_reports_unsupported_text_targets`) showing where Phase 2 plugs in.

### Risks / open design questions (for Phase 1)
- Precedence rule: when *both* content and typography change in one transition, which wins? Likely: typography change forces recompile (no morph); content-only change uses cached-path Fade. Needs an explicit decision and tests.
- `text_content` step-at-midpoint is itself a latent snap; Phase 1 should rely on `text_paths` interpolation, not on `text_content` interpolation, during the transition.
- Must keep `evaluate_text_paths(t_start)` seeding (`start_val`) consistent with the new render path so the cross-fade starts from the actually-rendered old glyphs.
