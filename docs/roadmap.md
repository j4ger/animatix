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

### P2.1 Fix Tree-Sitter Grammar Drift

**Gap:** The tree-sitter grammar is missing ~40% of language features that the chumsky parser accepts. Corpus tests reference node types that don't exist in `grammar.js`.

**Impact:** Syntax highlighting and tree-based navigation in editors will break on modern syntax.

**Work:**
- Synchronize `grammar.js` with `parser.rs` (add missing: `play`, `scene`, reactive binding, `use`, `@slot`, `drive`, index/method expressions, percent literals).
- Fix corpus tests (`test/corpus/statements.txt`) to match current grammar.
- Add CI check that tree-sitter corpus passes.

**Refs:** `crates/tree-sitter-animatix/grammar.js`, `crates/tree-sitter-animatix/test/corpus/statements.txt`

**Effort:** 1–2 days.

---

### P2.2 Fix Parser Bugs

**Gap:** Two confirmed bugs in `parser.rs`:
1. `Action.args` is hardcoded to `vec![]` — arguments are never parsed despite existing in AST.
2. Access chains (`foo().bar`) silently drop the `.bar` segment.

**Impact:** Language features exist in AST but are not reachable from source code.

**Work:**
- Implement action argument parsing.
- Fix access chain to either support method/index chains fully or emit a parse error.

**Refs:** `crates/animatix-syntax/src/parser.rs:820-833`, `crates/animatix-syntax/src/parser.rs:323`

**Effort:** 4–6 hours.

---

### P2.3 Split `evaluate_node` God Function

**Gap:** `timeline/scene_eval.rs:evaluate_node` is ~400 lines handling transforms, effects, rendering, hit regions, mask logic, and child recursion.

**Impact:** Single biggest maintenance liability. Any rendering bug requires editing this monolith.

**Work:**
- Split into phases: `eval_transform()`, `eval_effects()`, `render_dispatch()`, `render_children()`.
- Extract mask logic into its own helper.
- Add unit tests for each extracted phase.

**Refs:** `crates/animatix/src/timeline/scene_eval.rs:291-771`

**Effort:** 1–2 days.

---

### P2.4 Split animatix-analyzer `lib.rs`

**Gap:** `animatix-analyzer/src/lib.rs` is 838 lines hosting `Analyzer`, `Workspace`, hover, go-to-definition, references, document symbols, and position enrichment.

**Impact:** Violates single-responsibility principle; hard to extend.

**Work:**
- Split into `workspace.rs`, `hover.rs`, `definition.rs`, `references.rs`, `document_symbol.rs`.
- Dedupe `update()` and `force_rebuild_symbols()` (share parsing/symbol-building logic).

**Refs:** `crates/animatix-analyzer/src/lib.rs`

**Effort:** 1 day.

---

### P2.5 Split animatix-gui `app/mod.rs` and Remove Dead Stores

**Gap:** `app/mod.rs` is 1,468 lines with a 50-field `GuiShell` struct. `app/stores/` contains `DocumentStore`, `RuntimeStore`, `UiStore`, `WorkspaceStore` that are never instantiated.

**Impact:** State ownership is unclear. Previous refactoring attempt stalled.

**Work:**
- Either migrate `GuiShell` fields into the store modules or delete `app/stores/` entirely.
- Extract domain structs: `PreviewState`, `DocumentState`, `ExportState`, `SelectionState`.

**Refs:** `crates/animatix-gui/src/app/mod.rs`, `crates/animatix-gui/src/app/stores/`

**Effort:** 2–3 days.

---

### P2.6 Wire Up or Remove Bytecode VM

**Gap:** `timeline/modifier_runtime/vm.rs` is a fully implemented stack VM (~486 lines) with compiler and executor, but the build pipeline only lowers modifiers to IR. `apply_modifier_bytecode_program` is never invoked.

**Impact:** ~500 lines of dead code to maintain.

**Work:**
- Either wire `compile_modifier_bytecode` into `build/entry.rs` (after IR lowering) and add tests, or delete the VM entirely.

**Refs:** `crates/animatix/src/timeline/modifier_runtime/vm.rs`, `crates/animatix/src/timeline/build/entry.rs:145-160`

**Effort:** 4–8 hours.

---

### P2.7 Fix LSP Performance and Fragility

**Gap:** Two issues in `animatix-lsp`:
1. `rebuild_workspace()` rebuilds from all open documents on every keystroke — O(n²).
2. `references()` parses hover markdown with backticks to extract symbol names — extremely fragile.

**Impact:** LSP will become unusable at scale. References break on any hover formatting change.

**Work:**
- Make workspace rebuilds incremental (only update changed file, then re-resolve imports).
- Add `Analyzer::symbol_at(line, col) -> Option<String>` and use it instead of parsing markdown.

**Refs:** `crates/animatix-lsp/src/main.rs:37-58`, `crates/animatix-lsp/src/main.rs:353-363`

**Effort:** 1 day.

---

### P2.8 Split renderer/video.rs Megafile

**Gap:** `renderer/video.rs` is 1,696 lines mixing GIF encoding, video encoding, image export, and threaded render loops.

**Impact:** Hard to navigate and test. Single responsibility violation.

**Work:**
- Split into `encode/video.rs`, `encode/gif.rs`, `encode/image.rs`, and `render_pipeline.rs`.

**Refs:** `crates/animatix/src/renderer/video.rs`

**Effort:** 4–6 hours.

---

### P2.9 Add Missing Tests

**Gap:** Several critical paths have zero test coverage:
- `scene_eval.rs` — zero unit tests (most complex file in core crate).
- `renderer/` — no tests for `core.rs`, `offscreen.rs`, `video.rs`, `window.rs`.
- `animatix-gui` — zero tests for UI code (preview canvas, inspector, drag interactions).
- `animatix-analyzer` — zero tests for `hover_at()`, `definition_at()`, `find_references()`, `document_symbols()`.

**Work:**
- Add unit tests for `evaluate_node` phases after P2.3.
- Add initialization tests for `renderer/core.rs` and `renderer/offscreen.rs`.
- Add `egui_kittest` or screenshot regression for GUI drag/selection flows.
- Add analyzer tests for hover, definition, references, and document symbols.

**Effort:** 2–3 days.

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
