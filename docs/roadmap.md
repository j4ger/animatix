# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## Phase 5 — Source Editor Foundation

### 5.1 Lossless AST (green tree)

Immutable syntax tree that preserves whitespace and comments. Enables reliable source editing without formatting loss.

**Current state:** AST has `Span`/`ByteSpan` for positions, `Property.trailing_comment`, `Stmt::Comment`. Parser (Chumsky) discards whitespace/comments. `to_source` normalizes formatting on re-serialize.

**Verdict:** Defer. 6–8 weeks of work. Current source_edit + formatter handles 90% of cases. Only matters for perfect round-trip fidelity (rare edge case). Revisit if users report formatting loss.

| # | Task | Description | Effort | Blocker |
|---|------|-------------|--------|---------|
| 5.1.1 | **Trivia data structure** | Define `Trivia` type (leading whitespace, trailing whitespace, comments). Add to AST nodes. | 1 week | — |
| 5.1.2 | **Parser trivia capture** | Modify Chumsky parser to capture whitespace/comments as trivia instead of discarding. | 2–3 weeks | 5.1.1 |
| 5.1.3 | **to_source trivia emission** | Update `ToSource` impl to emit trivia when present, fall back to normalized formatting when absent. | 1 week | 5.1.2 |
| 5.1.4 | **Source edit trivia preservation** | Update `source_edit` module to preserve trivia during AST mutations (property edits, keyframe inserts, etc.). | 1–2 weeks | 5.1.3 |
| 5.1.5 | **Round-trip tests** | Parse → serialize → parse → serialize produces identical output for all example files. | 1 week | 5.1.4 |

---

## Phase 6 — Web Export & Media

> Expanding output targets and preview fidelity.

| # | Feature | Description | Effort | Blocker |
|---|---------|-------------|--------|---------|
| 1 | **Web canvas / WASM export** | Render to HTML5 Canvas or WebGPU for browser-based playback. Standalone `animatix-web` crate. | 1–2 months | Renderer abstraction |
| 2 | **Audio playback in preview** | Play audio segments during GUI preview (currently only muxed on video export). | 1 week | Audio backend (rodio/cpal) |
| 3 | **APNG export** | Animated PNG output for lossless web animations. Requires an APNG encoder backend. | 3 days | APNG encoder |

**Verdict:** Defer all. WASM export needs renderer abstraction (big refactor). Audio needs new dependency. APNG needs encoder. None are quick wins.

---

## Code Quality & Performance

---

### CQ-1: Duplicated guard-time keyframe logic

**Problem:** The pattern for inserting guard keyframes before instant-change actions with delay is copy-pasted 11 times across `entrance.rs`, `exit.rs`, `reveal.rs`.

| # | Task | File(s) | Details |
|---|------|---------|--------|
| CQ-1.1 | **Add `has_keyframe_at()` helper** | `actions/mod.rs` | `fn has_keyframe_at(track: &Option<impl AsRef<PropertyTrack<T>>>, time: u64) -> bool` — returns `track.as_ref().map(|t| t.keyframes.contains_key(&time)).unwrap_or(false)` |
| CQ-1.2 | **Add `ensure_guard_keyframe()` helper** | `actions/mod.rs` | Takes `&mut AnimationTrack`, field accessor fn, guard_time, prior_value, default. Calls `has_keyframe_at`, inserts keyframe if missing. |
| CQ-1.3 | **Replace inline patterns** | `entrance.rs`, `exit.rs`, `reveal.rs` | Replace all 11 guard blocks with calls to `ensure_guard_keyframe()`. Verify each action still behaves identically. |
| CQ-1.4 | **Add regression tests** | `actions/mod.rs` or inline | Test: action with delay + zero duration produces guard keyframe. Test: existing keyframe at guard_time is not overwritten. |

---

### CQ-2: Hot-path clone optimization in `track.rs:evaluate()`

**Problem:** `PropertyTrack<T>::evaluate()` clones `T` up to 9 times per call. Most `T` types are `f32`, `[f32; N]`, or small `Copy`-eligible enums.

| # | Task | File(s) | Details |
|---|------|---------|--------|
| CQ-2.1 | **Audit `T` types for `Copy` eligibility** | `track.rs`, `animation.rs` | List all `PropertyTrack<T>` instantiations. Mark which `T` can be `Copy` (`f32`, `[f32;2]`, `[f32;4]`, `[f32;6]`, `PlacementMode`, `PositionBinding`, `ShapeType`, `MorphOptions`). |
| CQ-2.2 | **Split `evaluate` into `evaluate_ref` + `evaluate_owned`** | `track.rs` | For `Copy` types, return `T` by value (zero-cost). For `Clone` types, keep current behavior. Alternatively, add `T: Copy` bound to a fast-path specialization. |
| CQ-2.3 | **Eliminate cache-hit clone** | `track.rs:~465` | Change cache return from `cached_value.clone()` to `*cached_value` for `Copy` types. For non-`Copy`, consider `Rc<T>` in cache. |
| CQ-2.4 | **Benchmark** | `benches/` or inline | Compare frame evaluation time before/after on a scene with 50+ tracks. Target: ≥20% reduction in evaluate() time. |

---

### CQ-3: Diagnostics position source inconsistency

**Problem:** `check_stmt` in `diagnostics.rs` mixes AST spans (Chumsky) with tree-sitter token lookups. The `unknown-action` verb gets precise tree-sitter position, but `undefined-label` for the same statement uses the full AST span.

| # | Task | File(s) | Details |
|---|------|---------|--------|
| CQ-3.1 | **Standardize on tree-sitter for all token-level diagnostics** | `diagnostics.rs` | When `tree` is `Some`, use `find_token_range()` for both verb and target positions. Fall back to AST span only when tree-sitter is `None`. |
| CQ-3.2 | **Add `find_target_range()` helper** | `diagnostics.rs` | Similar to `find_token_range()` but looks for `label_ref` or `target` node types. Used for `undefined-label` diagnostics. |
| CQ-3.3 | **Add position accuracy tests** | `diagnostics.rs` | Test that `undefined-label` diagnostic points to the target token, not the statement start. Use a test `.amx` file with known positions. |

---

### CQ-4: `scene_eval` double clone

**Problem:** `scene_eval.rs` clones `Scene` twice — once for cache, once for return — because `scene_buffer` takes ownership for encoding buffer reuse.

**Verdict:** Known limitation. Two clones are necessary because we need 3 copies: cache, return value, and buffer (for encoding reuse). The only way to eliminate this is to separate the Vello `Encoding` buffer from the scene data, which requires changes to the Vello API. Low priority — only matters for very large scenes.

---

### CQ-5: SVG import limitations

**Problem:** `svg_import.rs` has 9 unsupported SVG features. These are edge cases but block import of real-world SVGs.

| # | Task | File(s) | Details |
|---|------|---------|--------|
| CQ-5.1 | **`stroke-linecap` / `stroke-linejoin` mapping** | `svg_import.rs`, renderer | Blocked: Animatix renderer doesn't support linecap/linejoin yet. Requires adding `line_cap` and `line_join` properties to AnimationTrack and rendering them via Vello. ~1 week total. |
| CQ-5.2 | **`currentColor` support** | `svg_import.rs` | Resolve `currentColor` to the parent element's `color` property (CSS inheritance). Falls back to black if unset. |
| CQ-5.3 | **`inherit` fill/stroke** | `svg_import.rs` | Walk up the element tree to find the nearest ancestor with explicit fill/stroke. Use that value. |
| CQ-5.4 | **`<use>` element support** | `svg_import.rs` | Resolve `href`/`xlink:href` to the referenced element. Clone its children into the use element's position with optional transform. |
| CQ-5.5 | **`<clipPath>` support** | `svg_import.rs` | Parse `<clipPath>` definitions. Apply as Vello clip when referenced via `clip-path` attribute. |
| CQ-5.6 | **`<mask>` support** | `svg_import.rs` | Parse `<mask>` definitions. Apply as alpha/luminance mask. May require Vello mask shader. |
| CQ-5.7 | **SVG patterns** | `svg_import.rs` | Parse `<pattern>` definitions. Tile as fill/stroke. Complex — may defer to Phase 6. |
| CQ-5.8 | **Extended path commands** | `svg_import.rs` | Add support for `A` (arc), `H`/`V` (horizontal/vertical lineto), `S`/`T` (smooth curveto). `A` is the most impactful. |
| CQ-5.9 | **Import test suite** | `tests/svg/` | Create test SVGs covering each feature. Assert imported AST matches expected structure. |

---

### CQ-6: Analyzer namespace resolution for aliases

**Problem:** `workspace.rs:74` — aliased imports (`import "foo.amx" as foo`) are silently skipped. Symbols from aliased files are unreachable.

| # | Task | File(s) | Details |
|---|------|---------|--------|
| CQ-6.1 | **Add namespace field to `SymbolTable`** | `symbol_table.rs` | Add `namespace: Option<String>` field. When set, all symbols in this table are prefixed with `namespace.` in lookups. |
| CQ-6.2 | **Implement aliased merge in `resolve_symbols`** | `workspace.rs:~74` | When `import.alias.is_some()`, create a namespaced copy of the imported `SymbolTable` and merge it. Labels become `foo.label_name`, actions become `foo.action_name`. |
| CQ-6.3 | **Update `resolve_reference` for namespace lookup** | `symbol_table.rs` | When resolving `foo.bar`, split on first `.` and look up `bar` in the `foo` namespace. |
| CQ-6.4 | **Add completions support** | `lib.rs` | When cursor is after `foo.`, offer completions from the `foo` namespace only. |
| CQ-6.5 | **Add tests** | `tests/` | Test: aliased import resolves `foo.label`. Test: unaliased import still works. Test: completions after `foo.` offer namespace symbols. |

---

## Icebox

> Blocked on external dependencies, solve niche problems, or lack user demand. No committed timeline.

| Feature | Blocker / Reason |
|---------|------------------|
| **Zero-readback GPU filters** | Blocked on Vello GPU filter support ([#1296](https://github.com/linebender/vello/issues/1296)). Phase 8.6a (GPU compute + readback) ships the perf win; wait for Vello to eliminate the readback. |
| **Scene primitive (PiP)** | Existing components + Stack cover reuse. Needs composition-level design for parallel playback. Revisit after transition blending. |
| **Export performance: pre-compiled plot closures** | Only matters for dozens of plot actors. `always` block easing covers simpler cases. |
| **Asset usage tracking** | Show which actors reference an asset. Low user demand. |
| **Variable track UI** | GUI for `let` variable tracks. `always` blocks cover most cases. |
| **Module dependency graph** | Visual graph of `.amx` imports. Internal tooling, no user stories. |

---

## Next Steps (prioritized)

1. **Phase 5.1 — Lossless AST** (6–8 weeks)
   - Defer until users report formatting loss
   - Current system handles 90% of cases

2. **Phase 6 — Web Export** (2+ months)
   - Defer until renderer abstraction is done
   - Big refactor, no quick wins
