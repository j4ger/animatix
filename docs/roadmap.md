# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## P2 — Maintainability & Code Quality (Post-Audit)

> Findings from oracle audit of all crates. Ordered by impact.

---

### P2.10 Refactor Parser Monolith

**Gap:** `parser.rs` is 1,456 lines containing expressions, statements, inline items, modifiers, and top-level grouping.

**Impact:** Significant barrier to modification.

**Work:**
- Split into `parser/expr.rs`, `parser/stmt.rs`, `parser/inline.rs`, `parser/top_level.rs`.

**Refs:** `crates/animatix-syntax/src/parser.rs`

**Effort:** 1 day.

---

### P2.11 Clean Up AST Dead Code

**Gap:** `AnimatixFile` and `FileType` exist but are never constructed. `Expr::Tuple` is overloaded for both tuples and arrays.

**Work:**
- Remove `AnimatixFile` / `FileType` or start using them.
- Rename `Expr::Tuple` to `Expr::ArrayOrTuple` or split into two variants.
- Make span handling uniform (`ByteSpan` vs `Span`).

**Refs:** `crates/animatix-syntax/src/ast.rs`

**Effort:** 4–6 hours.

---

### P2.12 Add Structured Error Types

**Gap:** `Result<(), String>` is used everywhere in animatix-gui. `animatix-analyzer` discards chumsky structured errors immediately. `RenderError` is a minimal string-wrapper.

**Work:**
- Define `GuiError` enum in `animatix-gui`.
- Define `ParseError` struct in `animatix-analyzer` preserving positions.
- Add `#[source]` to `RenderError` variants.

**Refs:** `crates/animatix-gui/src/document.rs`, `crates/animatix-gui/src/preview_surface.rs`, `crates/animatix-analyzer/src/lib.rs:131,171`, `crates/animatix/src/renderer/error.rs`

**Effort:** 1 day.

---

## 3. Long-Term / Speculative

### 3.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 3.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 3.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## Deferred / Blocked

None currently.
