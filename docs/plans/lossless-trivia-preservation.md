# Lossless Whitespace/Trivia Preservation — Effort Assessment

> Date: 2026-06-09
> Status: Assessment — no implementation started
> Related: [roadmap.md](../roadmap.md) P2 — Source Editing

## Problem Statement

Every inspector edit in the GUI re-serializes the *entire* AST through
`format_core` (normalized formatting). This discards the user's original
whitespace, indentation, blank lines, and expression spacing choices.
While the AST roundtrip is semantically correct, the formatting loss means:

- Custom indentation (tabs, 4-space, etc.) is converted to 2-space
- Multiple blank lines between blocks are collapsed to one
- Spacing inside expressions is normalized (e.g. `(x+  y)` → `(x + y)`)
- Inline vs. multi-line formatting choices are lost
- Line-break choices are overridden

## Current Architecture

```
User action in GUI
  → handler in document_controller.rs
  → source_edit::apply_edit(&mut stmts, edit)      # mutate AST
  → stmts_to_source(&stmts)                         # full re-serialization (normalized)
  → SourceIndex::build(&stmts)                      # rebuild index
  → apply_source(new_source, source_index)          # apply to editor buffer
```

### What's already preserved

| Feature | Location | Notes |
|---------|----------|-------|
| `trailing_comment` on properties | `ast.rs:Property` | Captured during parse |
| Standalone `//` comments | `ast.rs:Stmt::Comment` | Captured during parse |
| `ByteSpan` / `Span` | Most AST nodes | Populated by parser for source locations |
| `value_span` on Property & Assignment | `ast.rs` | Byte range of the value expression |
| `try_surgical_keyframe_move` | `document_controller.rs` | Text-level replacement of time literal |

### Existing surgical optimization

```rust
// document_controller.rs:770
fn try_surgical_keyframe_move(source, old_time_s, new_time_s, is_relative) -> Option<String>
```

Only for `MoveKeyframeTime` — finds `#2s`, `#+500ms` etc. in the source text and
replaces the time literal, avoiding full re-serialization.

### What gets lost

- Custom indentation (→ normalized to 2-space)
- Extra blank lines (→ normalized to 1)
- Whitespace inside expressions (→ canonical spacing)
- Mid-expression comments (parser strips all `//` before parsing)
- Line break choices (→ all on one line or formatter's choice)
- Inline vs multi-line container children layout
- Spacing around `{}`, `[]`, `()` delimiters
- Spacing after `,` and before/after operators

---

## Three Approaches Compared

### Approach A — AST Trivia Nodes (Full rewrite)

Store original whitespace/comments as `Trivia` attached to every AST node.
Re-serialize using stored trivia; only re-format truly new nodes.

**Effort: 10–13 weeks**

**Sub-tasks:**

| # | Task | Effort | Dependencies |
|---|------|--------|--------------|
| A1 | Design `Trivia` types (leading/trailing whitespace, comments, blank lines) | 3 days | — |
| A2 | Add `trivia` fields to every AST node (7+ enum variants, 15+ struct types) | 5 days | A1 |
| A3 | Update chumsky parser to capture trivia (post-parse recovery from source using ByteSpan) | 2 weeks | A1 |
| A4 | Update tree-sitter converter `ts_convert.rs` to capture trivia | 1 week | A1 |
| A5 | Rewrite `format_core.rs` to emit stored trivia instead of generating new whitespace | 2–3 weeks | A2, A3 |
| A6 | Update all consumers of AST (typecheck, module, source_index) for new trivia fields | 1 week | A2 |
| A7 | Add "format new nodes" mode for nodes created during source edits (no stored trivia) | 1 week | A5 |
| A8 | Integration: update `apply_edit()` to clear trivia on mutated nodes | 3 days | A2 |
| A9 | Update all roundtrip/idempotency tests — many will break | 1 week | A5 |
| A10 | Add trivia-preservation roundtrip tests (various input styles) | 3 days | A9 |
| A11 | Fuzz testing: random formatting → edit → preserve | 3 days | A10 |

**Key risks:**
- Massive AST bloat (every `Expr`, `Property`, `Stmt` carries string(s))
- `format_core` is 470+ lines of careful matching — rewriting to conditionally
  emit trivia vs. generated whitespace is high-risk and fragile
- The choice between stored trivia vs. re-formatting for each sub-node is a
  source of bugs (e.g., a property value changed → clear its trivia, but keep
  parent's trivia)
- Every new AST feature needs trivia integration
- Parser `strip_comments()` approach makes comment recovery awkward

**Verdict: Not recommended.** Too invasive, too risky, too much AST churn
for a non-functional formatting concern.

---

### Approach B — Patch-Based Surgical Source Editing

Keep the original source text as the single source of truth. Make surgical
text replacements for value-level edits, only re-serializing the AST for
structural changes (add/remove/reorder statements).

Extends the existing `try_surgical_keyframe_move` + `diff_text` patterns.

**Effort: 4–6 weeks**

**Sub-tasks:**

| # | Task | Effort | Dependencies |
|---|------|--------|--------------|
| B1 | Audit all 19+ `SourceEdit` variants by "surgical-ability" | 1 day | — |
| B2 | Ensure ByteSpan coverage: add `byte_span` to Modifier, and verify coverage on all `Stmt` variants | 3 days | — |
| B3 | Centralize edit dispatch in `document_controller.rs` via a new `try_surgical_edit(source, stmts, edit) -> Option<NewSource>` | 2 days | B1 |
| B4 | **Surgical value replacement** for `SetProperty` — find property value's `value_span` in source, replace | 2 days | B2 |
| B5 | **Surgical value replacement** for `InsertProperty` — append to the actor's declaration line | 2 days | B2 |
| B6 | **Surgical value replacement** for `MergeKeyframe` — find assignment value's `value_span` in source, replace | 2 days | B2, B4 |
| B7 | **Surgical value replacement** for `SetConfigProperty` — find config property `value_span`, replace | 1 day | B2, B4 |
| B8 | **Surgical value replacement** for `SetKeyframeEasing` — find `ease:` modifier in action/assignment, replace | 2 days | B2 |
| B9 | **Surgical insert/delete keyframe** for `InsertKeyframe`, `DeleteKeyframe` — insert/remove lines at the right position | 3 days | B1 |
| B10 | **Surgical property insert/delete** for `InsertProperty` on existing lines | 1 day | B4 |
| B11 | Fallback path: structural edits (reorder, rename, reparent, extract scene) fall back to full re-serialization | 1 day | — |
| B12 | Improve `diff_text` utility to handle multi-edit cases | 1 day | — |
| B13 | **Testing**: property edits preserve formatting; structural edits still work | 3 days | B4-B11 |
| B14 | **Guarded rollout**: log/report surgical success rate; monitor formatting complaints | 1 day | B3 |

**Key design decisions:**
- **Only value-level edits go surgical.** Structural edits (reorder, rename,
  reparent, scene ops) fall back to full re-serialization. This covers the
  vast majority of inspector interactions.
- **Source text is the single source of truth.** The AST mutation is done for
  correctness/semantics, but the surgical path uses the original source text.
- **Fallback is always available.** Every surgical path falls through to
  `stmts_to_source(stmts)` if the surgical pattern isn't found.

**Risks:**
- ByteSpan must exactly match the source text — off-by-one errors corrupt the file
- Multi-byte Unicode needs careful handling (already done in `diff_text`)
- Comments overlapping with edited regions could be orphaned
- The cell notebook editor splits source into cells — surgical edits on the full
  source need to align with cell boundaries (or re-merge/re-split)
- `InsertProperty` appended to an existing line must match the line's formatting
  (comma placement, spacing)

**Verdict: Recommended.** Builds on existing infrastructure, preserves all
formatting for the common case, and is reversible (full fallback).

---

### Approach C — Minimal Surgical (subset of B)

Only handle the single most common edit — `SetProperty` (value change on an
existing property) — surgically. Everything else falls back to full
re-serialization. This covers ~60% of inspector edits with minimal code.

**Effort: 1.5–2 weeks**

**Sub-tasks:**

| # | Task | Effort | Dependencies |
|---|------|--------|--------------|
| C1 | Verify `value_span` is populated for all property values in the parser | 1 day | — |
| C2 | Implement surgical `SetProperty` in document_controller dispatch | 2 days | C1 |
| C3 | Implement surgical `MergeKeyframe` (assignment value change) | 2 days | C1 |
| C4 | Implement surgical `SetConfigProperty` | 1 day | C1 |
| C5 | Fallback for everything else (existing behavior) | 0 days | — |
| C6 | Tests for surgical value preservation | 2 days | C2-C4 |
| C7 | Guard: log successful surgical edits vs. fallbacks for monitoring | 1 day | C2 |

**Risks:**
- Lower coverage (only value changes, not structural edits or keyframe inserts)
- Users who frequently rearrange blocks still see formatting loss
- But: 80% of inspector actions are property value tweaks

**Verdict:** Good incremental step, low risk, low reward. Could be done as
Phase 1 of Approach B.

---

## Detailed Analysis: What Would Need to Change

### Parser (`crates/animatix-syntax/src/parser/mod.rs`)

Currently `strip_comments()` replaces `//` with spaces before parsing. For
**Approach A**, this would need to be replaced with a trivia-capturing parse.
For **Approach B/C**, no parser changes needed — we work with the original
source text and the existing `ByteSpan` ranges.

### AST (`crates/animatix-syntax/src/ast.rs`)

**Approach A:**
- Add `Trivia` enum: `Whitespace(String)`, `Comment(String)`, `BlankLine`
- Add `leading_trivia: Vec<Trivia>`, `trailing_trivia: Vec<Trivia>` to
  `Property`, `Modifier`, `Stmt` variants, `Expr` variants, `InlineItem`
  variants, `Action`, `Transition`, `ParamDef`, `ComponentDef`
- Or more granularly: attach trivia to alternatives of the enums

**Approach B/C:**
- No AST changes needed
- Possibly add a `byte_span: Option<ByteSpan>` to `Modifier` (currently missing)
  for finding ease: modifier positions surgically

### Format Core (`crates/animatix-syntax/src/format_core.rs`)

**Approach A:** Major rewrite. Every `format_*` function gains a conditional:
if the node has stored trivia, emit it; otherwise, generate canonical whitespace.
This doubles the complexity of 470+ lines of formatting logic.

**Approach B/C:** No changes. The surgical path bypasses format_core entirely.
Format_core remains the fallback for structural edits.

### Source Edit (`crates/animatix-gui/src/source_edit/`)

**Approach A:** `apply_edit` would need to clear trivia on all nodes it touches
so they get re-formatted. For structural edits, clearing trivia on the enclosing
scope.

**Approach B/C:**
- No changes to the 19 edit functions themselves
- The surgical dispatch happens in `document_controller.rs`, not in
  `source_edit/apply.rs`
- The edits still mutate the AST (for correctness/index rebuild), but the
  surgical path uses the original source text

### Document Controller (`crates/animatix-gui/src/app/document_controller.rs`)

**All approaches:** The centralization point. Currently every handler follows
the pattern:
```rust
source_edit::apply_edit(stmts, edit);
let new_source = stmts_to_source(stmts);  // ← replace this
let source_index = SourceIndex::build(stmts);
self.apply_source(new_source, source_index);
```

**Approach B/C:** Replace with:
```rust
source_edit::apply_edit(stmts, edit);
let old_source = &self.document_store.source.document.source_text;
let new_source = try_surgical_edit(old_source, stmts, &edit)
    .unwrap_or_else(|| stmts_to_source(stmts));
let source_index = SourceIndex::build(stmts);
self.apply_source(new_source, source_index);
```

Where `try_surgical_edit` is a new function in document_controller.rs
(or a new module) that dispatches to per-variant surgical handlers.

### Text Diff (`crates/animatix-gui/src/text_diff.rs`)

**Approach B/C:** The existing `diff_text` utility is used by the editor
buffer for cell-level text replacement. The surgical path doesn't need it —
it directly replaces byte ranges. But `diff_text` could be enhanced for
multi-edit support (B12).

### Source Index (`crates/animatix-syntax/src/source_index.rs`)

**All approaches:** No change needed. SourceIndex still maps
`(actor, property) → ByteSpan` for diagnostics. If anything, surgical edits
make the source index more accurate (original byte positions are preserved).

### Tree-Sitter Converter (`crates/animatix-syntax/src/ts_convert.rs`)

**Approach A:** Would need to capture trivia from tree-sitter's CST (which
natively has whitespace tokens). Moderate effort.

**Approach B/C:** No changes needed.

### Cell-Based Editor (`crates/animatix-gui/src/cell_editor/`)

**Approach B/C:** The notebook editor splits source into `Cell`s (Keyframe, Code).
A surgical edit on the full source text could violate cell boundaries. After
surgical edit, cells need to be re-parsed via `parse_cells()`. This is already
handled by `EditorBuffer::replace_text()` → re-parses cells.

**Risk:** A surgical edit that modifies a keyframe time line might merge/split
cells. The editor's `parse_cells()` handles this via regex-based cell boundary
detection, so this should be OK.

---

## Recommended Approach

**Start with Approach C (2 weeks), then extend to Approach B (additional 2–3 weeks).**

### Phase 1 (Weeks 1–2): Value-level surgical edits (Approach C)

Implement surgical replacement for:
1. `SetProperty` — replace value_span in source
2. `MergeKeyframe` — replace assignment value_span in source
3. `SetConfigProperty` — replace config value_span in source

**Effort:** 2 weeks
**Coverage:** ~60% of inspector edits (value changes)

### Phase 2 (Weeks 3–5): Keyframe and property structural edits

Add surgical support for:
1. `InsertKeyframe` — insert lines at the right position, format new content
2. `DeleteKeyframe` — remove lines
3. `InsertProperty` — append to existing declaration line
4. `SetKeyframeEasing` — replace `ease:` modifier in source
5. `InsertAction` — insert action line at keyframe position

**Effort:** 3 weeks
**Coverage:** ~90% of inspector edits

### Phase 3 (Optional, Week 6+): Structural edit improvements

For complex edits (reorder, rename, reparent), consider scoped
re-serialization: instead of re-serializing the entire file, only
re-serialize the affected block/keyframe/scope and splice it back into
the original source.

**Effort:** 1–2 weeks
**Coverage:** ~95%+

### Monitoring

Add a telemetry counter in the dispatch function:
```rust
// In try_surgical_edit:
match surgical_result {
    Ok(new_source) => { surgical_hits += 1; new_source }
    Err(()) => { surgical_misses += 1; stmts_to_source(stmts) }
}
```

If the surgical miss rate is high (>20%), the patterns need adjustment.
If it's low (<5%), the user experience is near-lossless.

---

## Summary

| Approach | Effort | Preserves Formatting | Code Change | Risk |
|----------|--------|----------------------|-------------|------|
| A (Full trivia AST) | 10–13 weeks | 100% (all edits) | Massive rewrite | High |
| B (Full surgical) | 4–6 weeks | 90%+ (value + keyframe edits) | Moderate addition | Low |
| C (Minimal surgical) | 1.5–2 weeks | ~60% (value edits only) | Small addition | Very low |
| Phase 1→2 (C→B) | 5 weeks total | 90%+ | Moderate addition | Low |

**Bottom line:** Avoid the full AST trivia rewrite (Approach A) — it's too
invasive for a formatting concern. Instead, implement surgical source editing
(Approach B/C) which builds on the existing `try_surgical_keyframe_move`
pattern and `ByteSpan` infrastructure. Start with value-level edits (Phase 1,
2 weeks) and incrementally add keyframe-level edits (Phase 2, +3 weeks).

The original 6–8 week estimate on the roadmap reflects the full AST trivia
approach. A surgical approach would be **4–6 weeks** for full coverage or
**2 weeks** for the most impactful subset (value changes).