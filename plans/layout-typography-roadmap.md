# Layout & Typography Improvement Roadmap

> Status: **Draft for review**. Derived from the layout/typography audit and a read of the current implementation (`primitives/`, `timeline/layout.rs`, `timeline/taffy_layout.rs`, `timeline/scene_eval.rs`, `timeline/build/`, `renderer/text.rs`, `property_registry.rs`, `track.rs`).
>
> Scope: the "next stage" of layout + typography work. Not a multi-year plan. Seven phases, each independently shippable and testable.

---

## 1. Audit → Phase Mapping

| Audit finding | Phase |
|---|---|
| Container `build()` are stubs (Phase 10b.3 incomplete) | **Phase 2** |
| Container logic in `scene_eval.rs` / `build/actor.rs` not in primitives | **Phase 2** (layout containers) + optional follow-up (Filter/Mask/Equation render) |
| Uniform gap/padding only (no per-axis/per-side) | **Phase 3** |
| Stack ignores gap/align | **Phase 3** |
| Only `font_family`/`font_size`/`color` supported | **Phase 1** |
| Missing `font_weight`/`font_style`/`line_height`/`letter_spacing`/`word_spacing` | **Phase 1** |
| Font weight/style queries not exposed despite `fontdb` support | **Phase 1** |
| `text_align` analyzer-only (no runtime) | **Phase 5** |
| All text goes through Typst compilation | **Phase 4** |
| No automatic text wrapping / no width constraints to Typst | **Phase 5** |
| No overflow handling | **Phase 5** |
| No baseline alignment (`center_text_paths` discards metrics) | **Phase 6** |
| No percentage-based child sizing | **Phase 7** |
| No intrinsic/min/max content sizing | **Phase 7** |
| Documentation gaps (text_align, weight/style, per-side padding, wrapping) | Each phase + §10 |

---

## 2. Architectural Decisions

### AD-1: Complete Phase 10b.3 (move container logic to primitives) first?

**Yes — as Phase 2, after the Phase 1 typography quick win.**

- The layout-container `build()` stubs mean real build logic (`register_container_metadata_and_apply_layout` in `timeline/build/entry.rs`, called from `timeline/build/actor.rs::process_inline_actor_decl` after `process_inline_actor_decl`) lives outside the primitive. New container features (per-axis gap, per-side padding, percentage sizing) are cleaner to add *inside* the primitive's property handling than by extending the legacy post-dispatch hook.
- It is **not a hard blocker** for typography work — Phase 1 (font weight/style) ships entirely through the Typst/registry path and can run first as a quick win.
- It **is a soft prerequisite** for Phases 3 and 7 (container data-model + sizing changes), so it should precede them.
- Scope discipline: Phase 2 migrates only the **layout containers** (Row/Col/Grid/Stack) build path. Migrating the **Filter/Mask/Equation render special-cases** out of `scene_eval.rs::render_node_children` is a separate, larger effort (needs `RenderCommand` to express clip layers and sub-scene compositing) and is listed as an optional follow-up (§9), not part of core Phase 2.

### AD-2: How to handle the Typst compilation bottleneck?

**Add a plain-text fast path that bypasses Typst for simple `Text` (Phase 4).**

- Today every `Text` actor — even a plain word — goes through `typst::compile` (`renderer/text.rs::compile_text` builds a `Source`, a `TypstWorld`, and runs the full document pipeline). The `TextCompiler` cache helps repeated strings but every distinct string pays full Typst cost.
- The fast path uses the **existing `fontdb` + `ttf_parser` `PathBuilder`/`OutlineBuilder` infrastructure** (already in `renderer/text.rs` for glyph extraction) to lay out glyphs directly: resolve face via `FontContext`, iterate glyphs, sum advances from `ttf_parser`, build outline paths. No `typst::compile`.
- **Typst is retained** for `Typst`, `Code`, `Equation`, and any `Text` containing markup-special characters (`* _ $ \ #` etc.) — detected by a "is this plain?" predicate.
- This is also the foundation for performant wrapping (Phase 5) and metric-preserving baseline alignment (Phase 6), because we own the layout and can read advances/ascent/descent directly.
- Cache key (`TextCacheKey`) gains `font_weight`/`font_style` (and later `width`/`text_align`) so the fast path and Typst path share the cache.

### AD-3: Text wrapping via Typst or a separate layout pass?

**Hybrid, gated by a `width`/`max_width` property; ship Typst-path wrapping first, fast-path wrapping second.**

- **Typst path**: pass width via `#block(width: Xpt)[...]` (or `#set page(width:)` for block layout) in `compile_text`/`compile_typst`. Typst is already a full layout engine; this is low-effort and immediately gives wrapping + `text_align` for Typst-routed actors.
- **Fast path**: greedy word-wrap using measured glyph advances (`ttf_parser` advances), breaking on whitespace, honoring `text_align` for line justification/alignment. Same `width` property.
- **Containers pass available width** to text children: layout containers (Row/Col/Grid) compute the content-box width available for a child and seed/override the child's `max_width`. This is wired in `timeline/layout.rs` / `scene_eval.rs` child evaluation.
- **Overflow**: `overflow: clip | ellipsis | visible` (default `visible`) — clip via existing `Mask` clip-layer mechanism or a per-actor clip rect; ellipsis via fast-path truncation. Scoped within Phase 5.
- Rationale: shipping the Typst path first decouples wrapping from the Phase 4 fast path; the fast path then makes wrapped text performant.

### AD-4: How to add font weight/style without breaking the existing API?

**New optional properties with defaults equal to current behavior; no existing `.amx` changes.**

- Add `font_weight` (Num 100–900, default 400 = regular; accept named `"bold"`/`"normal"` aliases) and `font_style` (`"normal"` | `"italic"`, default `"normal"`) to `PROPERTY_REGISTRY`, applicable to `Text`/`Typst`/`Code`.
- Add `font_weight: Option<PropertyTrack<...>>` and `font_style: Option<PropertyTrack<String>>` to `AnimationTrack` (alongside `font_family`/`font_size`), plus `ActorField` variants and accessor arms in `track.rs`.
- Map to Typst via `#set text(weight: ..., style: ...)` in the compile functions.
- Map to `fontdb` via `FontContext::load_face(family, weight, style)` — extend `FontContext::load_font` (which today queries `fontdb::Query` with default weight/style/stretch) to query with the requested weight/style. Crucially, **`build_world` must load all available faces** (regular/bold/italic/bold-italic) for a requested family into the `FontBook`, so Typst's font selection can match inline `*bold*` markup in `Typst` actors.
- Defaults reproduce today's output exactly (regular weight, upright style, one face). Existing `.amx` files are unchanged.
- `font_weight`/`font_style` are `ASSIGNABLE` (animatable is *not* requested by the audit and would cause re-compilation storms; keep them non-animated initially, like `font_family`).

---

## 3. Phase Overview

| # | Phase | Complexity | Depends on | Parallelizable with |
|---|---|---|---|---|
| 1 | Typography properties (weight, style, line-height, tracking, spacing) | Small–Medium | — | Phase 2 |
| 2 | Complete Phase 10b.3 — container primitive migration | Medium | — | Phase 1 |
| 3 | Rich container spacing + Stack alignment | Medium | Phase 2 | Phase 4 |
| 4 | Plain-text fast path (bypass Typst) | Large | Phase 1 (for weight/style in fontdb query) | Phase 3 |
| 5 | Text wrapping, width constraints & `text_align` | Large | Phase 4 (for fast wrap); Typst-only wrap needs only Phase 1 | Phase 6, 7 |
| 6 | Baseline alignment & text metrics | Small–Medium | Phase 4 (ideally); Typst-frame baseline extraction possible without | Phase 7 |
| 7 | Percentage & intrinsic content sizing | Large | Phase 2, 3 | Phase 5, 6 |

**Recommended sequencing:** 1 → 2 → (3 ∥ 4) → 5 → 6, with 7 after 3. Phases 1 and 2 can proceed in parallel; the typography track (1 → 4 → 5 → 6) and the layout track (2 → 3 → 7) are largely independent.

---

## 4. Phase 1 — Typography Properties

**Goal:** Expose `font_weight`, `font_style`, `line_height`, `letter_spacing`, `word_spacing` for `Text`/`Typst`/`Code`, wired through Typst `#set text(...)` and `fontdb` face selection. No behavior change for existing files.

**Deliverables:**
- New registry schemas in `timeline/property_registry.rs` for the five properties (defaults: weight 400, style "normal", line_height 1.0, letter_spacing 0, word_spacing 0).
- New `AnimationTrack` fields + `ActorField` variants + accessor arms in `timeline/track.rs`.
- Parse the properties in `timeline/declarations_text.rs::process_text_actor_decl` and thread them into the compile calls.
- Extend `renderer/text.rs`:
  - `FontContext::load_face(family, weight, style)` (query `fontdb::Query { weight, style, .. }`); keep `load_font` as the regular-weight shortcut.
  - `build_world` loads **all** faces for each requested family into the `FontBook` (iterate `fontdb` faces for the family) so Typst can select bold/italic.
  - `compile_text`/`compile_typst`/`compile_code`/`compile_math` accept weight/style/line_height/tracking/spacing and emit the corresponding `#set text(...)` rules (`weight`, `style`, `par(leading:)` / `#set par(leading:)`, `tracking`, `spacing`).
  - `TextCacheKey` extended with the new properties.
- Route the new properties through `primitives/mod.rs::evaluate_text_paths` (and the per-primitive `evaluate` in `text.rs`/`typst.rs`/`code.rs`) so runtime recompile (via `always`) honors them.
- Analyzer: add the properties to `animatix-analyzer/src/symbol_table.rs` and `completer.rs` hover docs.
- Docs: `docs/spec.md` §9 (text properties table) and §14; update the LLM generation checklist.

**Dependencies:** None. (Phase 4 will later make the fast path honor `font_weight`/`font_style` via the new `load_face`.)

**Complexity:** Small–Medium. The Typst `#set text` mapping is mechanical; the main care is loading multiple faces per family into the `FontBook`.

**Files/components:**
- `crates/animatix/src/timeline/property_registry.rs`
- `crates/animatix/src/timeline/track.rs`
- `crates/animatix/src/timeline/declarations_text.rs`
- `crates/animatix/src/renderer/text.rs`
- `crates/animatix/src/primitives/mod.rs` (`evaluate_text_paths`), `primitives/text.rs`, `primitives/typst.rs`, `primitives/code.rs`
- `crates/animatix-analyzer/src/symbol_table.rs`, `completer.rs`
- `docs/spec.md`

**Success criteria:**
- `title: Text, text: "Hi", font_weight: 700` renders bold; `font_style: "italic"` renders italic; `Typst` with `*bold*` markup renders bold when the family has a bold face.
- `line_height: 1.5`, `letter_spacing: 2`, `word_spacing: 4` visibly affect multi-word text.
- Existing `.amx` files render byte-identically (golden frame test).
- `cargo test -p animatix` green; new unit tests for face selection and `#set text` emission.

**Risks:**
- A requested family may lack a bold/italic face → `fontdb` returns nearest match silently. Acceptable (standard font fallback), but document it. Consider a build-time warning if the exact weight is unavailable and a non-regular weight was requested.
- Loading all faces per family increases `FontBook` size / build-world cost slightly; mitigate by loading faces lazily per requested family (already the `extra_fonts` pattern).

---

## 5. Phase 2 — Complete Phase 10b.3: Container Primitive Migration

**Goal:** Remove the `Row`/`Col`/`Grid`/`Stack` `build()` stubs. Move container metadata registration + layout application *into* the primitive `build()` methods, eliminating the "legacy dispatch" hook in `build/actor.rs`.

**Deliverables:**
- Make `Timeline::register_container_metadata_and_apply_layout` (in `timeline/build/entry.rs`) `pub(crate)` so primitives can call it via `BuildCtx::timeline`.
- Implement `build()` in `primitives/row.rs`, `col.rs`, `grid.rs`, `stack.rs` to:
  1. Call `ctx.timeline.process_inline_actor_decl(...)` (the existing shared actor-decl processing) to register the actor track + children.
  2. Read `gap`/`padding`/`align`/`cols` from `props` (via the `ContainerLayout` group handler / registry helpers).
  3. Call `ctx.timeline.register_container_metadata_and_apply_layout(...)` to seed `ContainerMetadata` and apply layout positions.
- Remove the `if primitive.is_layout_container() { register_container_metadata_and_apply_layout(...) }` post-step in `timeline/build/actor.rs` (two call sites: ~line 632 and ~842).
- Keep `LayoutEngine` (`timeline/layout.rs`), `ContainerMetadata` (`timeline/mod.rs`), and `taffy_layout.rs` as the shared computation backend — the migration is about *who calls them*, not rewriting them.
- Leave the `evaluate()` methods returning empty (containers have no self-visual); `scene_eval.rs::render_node_children` still recurses for layout containers. That recursion is correct and stays.

**Dependencies:** None (can run parallel to Phase 1).

**Complexity:** Medium. Mechanically straightforward but touches the build hot path; risk of subtle ordering regressions (metadata must be registered after the track + children exist, which today is guaranteed by the post-`process_inline_actor_decl` timing — preserve that ordering).

**Files/components:**
- `crates/animatix/src/primitives/row.rs`, `col.rs`, `grid.rs`, `stack.rs`
- `crates/animatix/src/timeline/build/actor.rs` (remove post-hook)
- `crates/animatix/src/timeline/build/entry.rs` (visibility)
- `crates/animatix/src/timeline/layout.rs`, `mod.rs` (unchanged behavior; confirm `ContainerMetadata` construction sites still compile)

**Success criteria:**
- All existing layout tests pass unchanged (`timeline/tests.rs`, `taffy_layout.rs` tests, `examples/02_layout.amx`, `12_reorder.amx` golden frames).
- No `// Build handled by legacy dispatch` comments remain in the four container primitives.
- `grep -rn "is_layout_container" crates/animatix/src/timeline/build/actor.rs` shows the post-hook removed.
- `cargo test --no-fail-fast` green.

**Risks:**
- Build-ordering: `register_container_metadata_and_apply_layout` reads `track.children` and `layout_size`; it must run *after* children are processed. The current post-hook timing guarantees this. In the primitive `build()`, the primitive is built *before* its children (children are processed by the body walker after the parent decl). Verify the primitive `build()` defers layout application to a point where children are known, or keep layout application in the existing post-children hook but invoke it from the primitive via a `finalize_children()` trait method. **Open question Q1.**
- `actions/mod.rs` constructs `ContainerMetadata` literals directly (6+ sites for reorder/swap). These stay valid; just ensure the field set remains compatible with Phase 3 changes.

---

## 6. Phase 3 — Rich Container Spacing & Stack Alignment

**Goal:** Support per-axis `gap` and per-side `padding`, and make `Stack` honor cross-axis `align`.

**Deliverables:**
- Extend `ContainerMetadata` (`timeline/mod.rs`):
  - `gap: f32` → `gap: [f32; 2]` (main-axis, cross-axis) or keep `gap: f32` + add `gap_cross: f32`. **Recommendation:** `[f32; 2]` to match Taffy's `Size { width, height }`.
  - `padding: f32` → `padding: [f32; 4]` (left, top, right, bottom) to match Taffy's `Rect`.
  - Provide helpers `gap_uniform(v)`, `padding_uniform(v)` and a resolution function from authored value → arrays.
- Accept richer authored forms (backward compatible):
  - `gap: 8` → uniform `[8, 8]`
  - `gap: (8, 12)` → `[8, 12]` (main, cross)
  - `padding: 10` → `[10,10,10,10]`
  - `padding: (8, 12)` → vertical 8, horizontal 12 → `[12, 8, 12, 8]`
  - `padding: (4, 8, 4, 8)` → `[4, 8, 4, 8]` (left, top, right, bottom)
- Update the `ContainerLayout` group handler (`property_engine.rs`) and the `gap`/`padding` schemas to parse tuples.
- Wire per-axis gap and per-side padding into `taffy_layout.rs::compute_taffy_linear_layout` / `compute_taffy_grid_layout` (Taffy already accepts `Size` gap and `Rect` padding — just consume the arrays instead of broadcasting a scalar).
- Update all `ContainerMetadata { ... }` literal construction sites: `build/entry.rs`, `actions/mod.rs` (reorder/swap sites), and any test helpers.
- **Stack alignment**: extend `compute_stack_layout` (`timeline/layout.rs`) to honor `align` for cross-axis positioning of each child within the stack's content box (start/center/end), and allow `align`/`gap` on `Stack` in `property_registry.rs` (currently restricted to Row/Col/Grid). Decide Stack+`gap` semantics (see Q2).
- Docs: `docs/spec.md` §8 (container properties), architecture §4.

**Dependencies:** Phase 2 (so container property handling lives in the primitive cleanly). Strictly, the data-model change could be done without Phase 2, but doing it after avoids re-touching the just-migrated code.

**Complexity:** Medium. Taffy already supports the richer inputs; the work is data-model + parsing + literal-site updates + Stack logic.

**Files/components:**
- `crates/animatix/src/timeline/mod.rs` (`ContainerMetadata`)
- `crates/animatix/src/timeline/layout.rs`, `taffy_layout.rs`
- `crates/animatix/src/timeline/property_registry.rs`, `property_engine.rs` (group handler)
- `crates/animatix/src/timeline/build/entry.rs`, `actions/mod.rs` (literal sites)
- `crates/animatix/src/primitives/stack.rs` (align semantics)
- `docs/spec.md`, `docs/architecture.md`

**Success criteria:**
- `Row, gap: (8, 4), padding: (10, 20, 10, 20) { ... }` spaces children 8px on the main axis, 4px cross, with asymmetric padding.
- `Stack, align: "start" { ... }` aligns children to the cross-axis start instead of center.
- Scalar `gap`/`padding` (existing syntax) still works identically.
- All existing layout tests pass; new unit tests for tuple parsing and per-axis Taffy input.

**Risks:**
- `ContainerMetadata` is `Clone` and constructed in many places; changing its fields is a wide-ish refactor. Mitigate with a constructor `ContainerMetadata::new(layout_type, gap, padding, align, cols)` that accepts the resolved arrays, so literal sites call the constructor instead of struct syntax.
- Stack + `gap` semantics is ambiguous for an "overlap" container (see Q2). Recommend emitting a diagnostic if `gap` is set on `Stack` without a `direction`, OR introducing `direction: row|col` to make gap meaningful. Needs a design call.

---

## 7. Phase 4 — Plain-Text Fast Path (Bypass Typst)

**Goal:** Render simple `Text` actors without invoking `typst::compile`, using `fontdb` + `ttf_parser` directly. Major perf win for text-heavy scenes; foundation for performant wrapping and baseline alignment.

**Deliverables:**
- New `renderer/text.rs::compile_text_fast(content, family, weight, style, size, color, font_ctx)` that:
  - Resolves the face via `FontContext::load_face(family, weight, style)` (from Phase 1).
  - Iterates characters, maps to glyph ids via `ttf_parser::Face`, sums `glyph.x_advance` (scaled by `size / units_per_em`), and builds outline paths via the existing `PathBuilder`/`OutlineBuilder`.
  - Returns `Vec<TextPath>` in the same coordinate convention as `extract_glyphs` (so `center_text_paths` / `measure_text_paths` work unchanged).
  - Honors `letter_spacing` (add to advance) and `word_spacing` (add on space).
- A `is_plain_text(content)` predicate: true iff the string contains no Typst markup-special chars (`* _ $ \ # < > ~` etc.) and no newlines (single-line). Multi-line fast path is Phase 5.
- Route `primitives/text.rs::evaluate` (and `evaluate_text_paths` in `primitives/mod.rs`) to the fast path when `is_plain_text` is true; fall back to `compile_text` otherwise.
- Extend `TextCompiler::compile` to dispatch to `compile_text_fast` for `TextKind::Text` when plain, sharing the same cache (key already extended in Phase 1).
- Phase 1 integration: the fast path uses `load_face` with weight/style, so bold/italic plain text works without Typst.
- Metric extraction: alongside paths, return an ascent/descent/line-gap struct (read from `ttf_parser::Face::ascender`/`descender`/`line_gap`) — stored for Phase 6. Keep this behind a struct now even if unused, to avoid a second pass later.
- Tests: metric-equivalence tests comparing fast-path glyph positions against Typst output for a corpus of plain strings (allow sub-pixel tolerance for hinting/kerning differences). Keep `compile_text` as an opt-in fallback for parity debugging.

**Dependencies:** Phase 1 (for `load_face` with weight/style and the extended cache key).

**Complexity:** Large. The glyph-extraction infra exists, but building a correct shaper-adjacent layout (advances, kerning pairs, RTL, combining marks) is the hard part. **Scope to LTR Latin** for the fast path initially; fall back to Typst for anything with complex script requirements. Detect non-Latin and route to Typst.

**Files/components:**
- `crates/animatix/src/renderer/text.rs` (new `compile_text_fast`, `is_plain_text`, metric struct)
- `crates/animatix/src/primitives/mod.rs` (`evaluate_text_paths` routing)
- `crates/animatix/src/primitives/text.rs` (evaluate dispatch)
- New test module comparing fast vs Typst paths

**Success criteria:**
- A scene with 100 distinct plain `Text` actors renders ≥5× faster (benchmark vs current Typst path).
- Output is visually indistinguishable from the Typst path for plain Latin strings (golden frame diff within tolerance).
- `font_weight: 700` plain text renders bold via the fast path (no Typst).
- Non-plain text (contains `*`, `_`, `$`, etc.) still routes through Typst automatically.
- `cargo test -p animatix` green; new perf benchmark in `tests.rs` or a `benches/` target.

**Risks:**
- **Kerning/hinting mismatch** with Typst for specific glyph pairs. Mitigate: enable `ttf_parser` kerning (`Face::tables().kern` / use `harfbuzz_ref` if available — but prefer no new deps; accept minor kerning differences and document). If parity is unacceptable, keep Typst as the source of truth and use the fast path only when exact parity is confirmed for the font.
- **Complex scripts** (Arabic, CJK, Indic) require a real shaper. Mitigate: detect script, fall back to Typst. Document the fast path as Latin-only initially.
- This is the highest-risk phase; consider a feature flag (`config { text_fast_path: true }`) to roll out incrementally.

---

## 8. Phase 5 — Text Wrapping, Width Constraints & `text_align`

**Goal:** Automatic line wrapping for `Text`/`Typst`/`Code`, driven by a `width`/`max_width` property and container-provided available width; implement `text_align` (left/center/right/justify) at runtime; add overflow handling.

**Deliverables:**
- New properties in `property_registry.rs`: `width` (already exists for sized actors — extend `Applicable` to `Text`/`Typst`/`Code`) or a dedicated `max_width`; `text_align` (`"left"|"center"|"right"|"justify"`, default `"left"`); `overflow` (`"visible"|"clip"|"ellipsis"`, default `"visible"`).
- New `AnimationTrack` fields + `ActorField` variants for `text_align`, `overflow` (and reuse `size`/`width` for the constraint).
- Parse in `declarations_text.rs`; thread `width`/`text_align`/`overflow` into compile calls.
- **Typst path** (`renderer/text.rs`): wrap content in `#block(width: Xpt)[...]` when `width` is set; set `#set align(center|left|right)` for `text_align`. Ship this first (works without Phase 4).
- **Fast path** (Phase 4): greedy word-wrap using measured advances; break on whitespace; lay out lines top-down using ascent/descent; honor `text_align` per line; `ellipsis` truncation with `…` glyph; `clip` via a per-actor clip rect.
- **Container → child width**: in `timeline/layout.rs` / `scene_eval.rs`, when a text child is admitted to a Row/Col/Grid, compute the available content-box width (container width minus padding/gap/siblings) and seed the child's `max_width` track (or pass via `EvaluateCtx`). For `Col`, available width = container content width; for `Row`, available width is unbounded unless the child has an explicit `width`.
- Implement `text_align` runtime (currently analyzer-only — `symbol_table.rs`/`completer.rs` already list it). Remove the "analyzer-only" status note in `spec.md`.
- Docs: `spec.md` §9, §14; add a wrapping/overflow section.

**Dependencies:** Phase 4 for the fast-path wrap. The Typst-path wrap can ship with only Phase 1. Phase 5 is most valuable after Phase 4 so both paths wrap performantly.

**Complexity:** Large. Container→child width propagation is the architecturally tricky part (the current layout system is declaration-time measure/place; passing a per-frame available width to text children interacts with `dynamic_layout` and the `layout_size` track).

**Files/components:**
- `crates/animatix/src/timeline/property_registry.rs`, `track.rs`, `declarations_text.rs`
- `crates/animatix/src/renderer/text.rs` (Typst `#block(width:)`, fast-path wrap)
- `crates/animatix/src/primitives/mod.rs` (`evaluate_text_paths` passes width/align), `text.rs`/`typst.rs`/`code.rs`
- `crates/animatix/src/timeline/layout.rs`, `scene_eval.rs` (available-width propagation)
- `crates/animatix-analyzer/src/symbol_table.rs`, `completer.rs` (add `width`/`overflow`; `text_align` already present)
- `docs/spec.md`

**Success criteria:**
- `blurb: Text, text: "long ...", width: 200, text_align: "center"` wraps to multiple lines, centered, within 200px.
- A `Col` containing a `Text` child without explicit `width` wraps the text to the column's content width.
- `overflow: "ellipsis"` truncates with `…` when text exceeds `width`; `overflow: "clip"` cuts at the boundary.
- Single-line text with no `width` set renders exactly as today (no regression).
- `text_align` is no longer analyzer-only; it produces visible alignment.

**Risks:**
- Container→child width propagation may conflict with the "declaration-time measure/place" contract (`layout.rs` header). Text measurement currently happens at build time (`declarations_text.rs` compiles and measures); wrapping requires knowing the width *before* measuring. Mitigate: two-pass — seed `max_width` from container build, then measure text with that width. For `dynamic_layout`, re-measure per frame (the `TextCompiler` cache makes this cheap).
- `justify` alignment needs word-spacing distribution; edge case for single-word lines.
- Typst `#block(width:)` may add its own padding/margins; verify and counter-set.

---

## 9. Phase 6 — Baseline Alignment & Text Metrics

**Goal:** Stop discarding font metrics in `center_text_paths`; preserve ascent/descent so text actors can baseline-align within containers.

**Deliverables:**
- In `renderer/text.rs`, capture per-compilation metrics (ascent, descent, line gap, baseline offset) alongside glyph paths. For the fast path, read from `ttf_parser::Face`; for Typst, extract from the `Frame` (Typst frames expose baseline per line).
- Replace the unconditional bbox-centering in `center_text_paths` with **optional metric-based centering**: center vertically by the font's cap-height/median instead of the path bbox, so text of different sizes/families aligns consistently. Gate behind a flag or a new `vertical_align` property (`"center"|"baseline"|"top"|"bottom"`, default `"center"` to preserve current behavior).
- Add `align: "baseline"` support for text children in layout containers (`timeline/layout.rs` / `taffy_layout.rs` via Taffy's `AlignItems::Baseline`), so a `Row` of text actors of different sizes shares a baseline.
- Store the baseline offset on the `AnimationTrack` (or in a side table keyed by compilation) so `scene_eval.rs` can offset text placement.
- Docs: `spec.md` §4/§8 (baseline alignment), architecture §4.

**Dependencies:** Phase 4 ideally (fast path yields metrics directly). Without Phase 4, Typst frame baseline extraction is possible but requires restructuring `extract_glyphs` to return metrics — feasible as a standalone.

**Complexity:** Small–Medium. The metrics exist; the work is plumbing them through `center_text_paths` (or its replacement) and adding the `AlignItems::Baseline` path.

**Files/components:**
- `crates/animatix/src/renderer/text.rs` (metric capture, `center_text_paths` → metric-aware variant)
- `crates/animatix/src/timeline/track.rs` (store baseline/metrics), `scene_eval.rs` (apply baseline offset)
- `crates/animatix/src/timeline/layout.rs`, `taffy_layout.rs` (baseline align)
- `crates/animatix/src/timeline/property_registry.rs` (`vertical_align` if added)
- `docs/spec.md`

**Success criteria:**
- A `Row` with `align: "baseline"` containing `Text` actors of `font_size` 48 and 24 shares a common baseline.
- Text vertical centering is consistent across fonts (no longer shifts when a font's descender depth differs).
- Existing centered-text golden frames remain acceptable (may shift sub-pixel due to metric vs bbox centering — decide tolerance; provide `vertical_align: "center"` to mean bbox-center if exact backward compat is required).

**Risks:**
- Changing default vertical centering from bbox to metric will shift existing text vertically by a few pixels. Decide: keep bbox as default and make metric centering opt-in, or accept the shift and update golden tests. **Recommendation:** keep bbox default for backward compat; add `vertical_align: "baseline"` as opt-in, and make container `align: "baseline"` the primary entry point.
- Typst frame baseline extraction depends on Typst internals staying stable.

---

## 10. Phase 7 — Percentage & Intrinsic Content Sizing

**Goal:** Allow container children to be sized by percentage and by intrinsic/min/max content; let containers themselves size to fit content (`size: auto`/`fit`).

**Deliverables:**
- **Percentage child sizing**: accept `size: (50%, auto)`, `size: fill`, `width: 30%` for layout-managed children. Resolve percentages against the parent container's content box (after padding/gap). Wire `Dimension::Percent` / `LengthPercentage::Percent` in `taffy_layout.rs` (Taffy supports this — currently `fixed_leaf_style` uses `Dimension::length`). The `layout_size` track needs to express "percentage" vs "fixed"; introduce a resolved-size pass after container content-box is known.
- **Intrinsic content sizing**: for `size: auto`/`fit` on a container, compute the container's size from children's intrinsic sizes (Row → sum of child widths + gaps; Col → max child width; Grid → grid tracks). Use Taffy's content-sized tracks (`GridTemplateComponent::Auto`) or pre-compute via `compute_grid_tracks`-style logic. This lets a `Row` shrink-wrap its children.
- **min/max content**: `min_width`/`max_width`/`min_height`/`max_height` properties → Taffy `min_size`/`max_size`.
- Note: percentage *placement* (`at: (50%, 60%)`) already works (spec §8); this phase is specifically percentage *sizing* and intrinsic container sizing.
- Distinguish from existing `size: fill` on `Image` inside `Filter` (already works in that context) — generalize the mechanism.
- Docs: `spec.md` §8 (sizing model).

**Dependencies:** Phase 2 (primitive build) and Phase 3 (rich spacing, since content-box computation depends on per-side padding). Percentage sizing also benefits from Phase 5 (text wrapping) so `auto`-width text can wrap to a percentage width.

**Complexity:** Large. The layout system is currently declaration-time with fixed child half-sizes (`ChildExtent.half_size` is `[f32; 2]`). Percentage and intrinsic sizing require a two-pass measure: container computes content box → children resolve percentage/intrinsic → container shrink-wraps. This is the biggest architectural change in the roadmap.

**Files/components:**
- `crates/animatix/src/timeline/layout.rs` (`ChildExtent`, measure/place contract), `taffy_layout.rs` (`Dimension::Percent`, `Auto`, `min/max_size`)
- `crates/animatix/src/timeline/track.rs` (`layout_size` semantics — percentage vs fixed), `property_registry.rs` (`min_width` etc.)
- `crates/animatix/src/timeline/build/entry.rs`, `build/actor.rs` (two-pass measure)
- `crates/animatix/src/primitives/row.rs`/`col.rs`/`grid.rs` (container shrink-wrap)
- `docs/spec.md`

**Success criteria:**
- `Col { a: Rect, size: (50%, 40); b: Rect, size: fill }` sizes `a` to half the column content width and `b` to fill the remainder.
- `Row, size: auto { ... }` shrink-wraps to the total width of its children.
- `min_width`/`max_width` constrain a child's resolved size.
- Existing fixed-size layouts unchanged.

**Risks:**
- Two-pass measurement conflicts with the declaration-time contract documented in `layout.rs`. May require extending the contract to allow a bounded measure pass. This is the phase most likely to surface deeper architectural rework.
- Interaction with `dynamic_layout` (per-frame relayout) — percentage sizing must re-resolve per frame when the container animates.
- Taffy `Auto` track sizing for grids can be subtle; test against CSS Grid expectations.

---

## 11. Cross-Cutting Concerns

### Documentation (audit gap D1–D4)
Each phase updates `docs/spec.md` and `docs/architecture.md` as noted. Additionally:
- **Phase 1**: document `font_weight`/`font_style`/`line_height`/`letter_spacing`/`word_spacing` in spec §9 and the LLM generation checklist.
- **Phase 3**: document per-axis `gap` / per-side `padding` tuple forms and Stack `align` in spec §8.
- **Phase 5**: remove the "analyzer-only" implication for `text_align`; document `width`/`text_align`/`overflow` and wrapping behavior.
- **Phase 6**: document `vertical_align` / baseline alignment.
- **Phase 7**: document percentage/intrinsic sizing model.
- Keep `docs/roadmap.md` as the only remaining-work list: remove items as they ship, do **not** add this roadmap's phases to `roadmap.md` (this file is the plan; `roadmap.md` stays high-level).

### Testing
- Each phase adds unit tests in the touched module + at least one `.amx` example under `examples/` exercising the new feature.
- Golden frame tests: any phase that changes rendering (1, 4, 5, 6) must either keep existing golden frames identical or explicitly update them with a documented reason.
- Perf: Phase 4 adds a benchmark; Phase 5 verifies wrapping doesn't regress fast-path throughput.
- Per `AGENTS.md`: `cargo check` (0 errors) and `cargo test --no-fail-fast` (all passing) before commit; `cargo test -p animatix` and `cargo test -p animatix-gui` when relevant.

### Conventional commits
Scopes from `cog.toml`: `parser`, `renderer`, `timeline`, `analyzer`, `lsp`, `syntax`, `docs`. Example: `cog commit feat "add font_weight/style" timeline`.

---

## 12. Risks & Open Questions

**Cross-phase risks:**
- The typography track (1→4→5→6) and layout track (2→3→7) are mostly independent, but **Phase 5 (wrapping) needs container width propagation**, which couples the two tracks at that point. Sequence Phase 3 before Phase 5 if possible.
- **Renderer/Typst API stability**: Phases 1, 4, 5, 6 all depend on Typst internals (`Frame` structure, `#set text` rules, `#block(width:)`). Pin Typst version; monitor upstream.
- **Performance regressions**: loading multiple font faces per family (Phase 1) and two-pass layout measurement (Phase 7) add build-time cost. Benchmark before/after each phase.

**Open questions (need decisions before/during implementation):**

- **Q1 (Phase 2):** Should the primitive `build()` apply layout immediately (children not yet processed) or via a `finalize_children()` trait hook called after the body walker processes children? The latter is safer but expands the `Primitive` trait. **Recommendation:** add a `finalize_container_build()` default-no-op trait method invoked by the body walker after children are processed; layout containers override it.
- **Q2 (Phase 3):** Stack + `gap` semantics — overlap container with gap is contradictory. Options: (a) `gap` on Stack is a diagnostic unless `direction` is set; (b) introduce `direction: row|col` on Stack to make it a non-overlapping flow (overlaps with Row/Col — may not be worth it); (c) leave Stack overlap-only, support only `align` (cross-axis). **Recommendation:** (c) + (a) — support `align`, emit a diagnostic for `gap` on `Stack`.
- **Q3 (Phase 5):** Does `text_align` apply to single-line text (anchor within a `width` box) or only to wrapped multi-line text? **Recommendation:** apply to both — single-line text with `width` + `text_align: center` centers within the box (useful for buttons/labels).
- **Q4 (Phase 6):** Backward compat for vertical centering — bbox (current) vs metric. **Recommendation:** keep bbox as default, add `vertical_align: "baseline"` opt-in, and make container `align: "baseline"` the primary opt-in path.
- **Q5 (Phase 7):** Percentage sizing relative to parent *content box* (after padding/gap) or *border box*? **Recommendation:** content box (CSS-standard).
- **Q6 (Phase 4):** If fast-path kerning parity with Typst proves unattainable for a target font, do we ship the fast path with documented differences, or gate it to fonts where parity is verified? **Recommendation:** ship with a `text_fast_path` config flag (default on for Latin-only), document differences, allow per-file opt-out.

---

## 13. Sequencing Summary

```
Phase 1 (typography props)         ─┐
                                    ├─► (independent quick wins, do first)
Phase 2 (10b.3 container migrate)  ─┘
        │
        ├── Phase 3 (rich spacing + Stack)  ──┐
        │                                     ├── Phase 7 (percentage/intrinsic sizing)
        │                                     │
Phase 4 (text fast path) ──┬── Phase 5 (wrapping + text_align) ──┬── Phase 6 (baseline/metrics)
        │                  │                                      │
        └─ (needs Phase 1) └─ (Typst-only wrap needs only Ph 1)  └─ (Typst baseline possible w/o Ph 4)
```

**Milestone suggestion:** Phases 1–3 form a coherent "usability" milestone (bold/italic, rich spacing, clean container architecture). Phases 4–6 form a "text performance & layout" milestone. Phase 7 is an "advanced layout" milestone that can follow once the foundation is stable.
