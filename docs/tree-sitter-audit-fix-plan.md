# Tree-sitter Grammar Audit & Fix Plan

> **Date:** 2025-06-05
> **Scope:** `tree-sitter-animatix/` (grammar) + `crates/tree-sitter-animatix/` (Rust crate)
> **Cross-referenced against:** `docs/spec.md`, `animatix-syntax/src/parser/mod.rs`, `animatix-analyzer/`, all `examples/*.amx`

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Architecture Context](#architecture-context)
3. [Critical Issues](#critical-issues)
4. [Medium Issues](#medium-issues)
5. [Low Issues](#low-issues)
6. [Informational (No Action Needed)](#informational-no-action-needed)
7. [Fix Phases](#fix-phases)
8. [Testing Strategy](#testing-strategy)

---

## Executive Summary

The tree-sitter grammar is **syntactically correct for the features it covers** but is **significantly out of sync with the actual language** as implemented in the chumsky parser. The analyzer (`animatix-analyzer`) has **12+ dead code paths** referencing tree-sitter node kinds that don't exist in the grammar. This means LSP features (completions, hover, go-to-definition, find-references) are partially broken.

### Severity Breakdown

| Severity | Count | Description |
|---|---|---|
| **Critical** | 2 | Grammar/analyzer desync, missing language features |
| **Medium** | 7 | `if`-expressions, `null`, `auto`/`fill`, keyframe redundancy, highlights |
| **Low** | 9 | Tests, docs, build paths, metadata |
| **By Design** | 2 | Hyphens in identifiers, no escape sequences — confirmed correct per spec |

---

## Architecture Context

The system uses **two parallel parsers**:

```
Source (.amx)
    ├──→ Chumsky parser (animatix-syntax) → authoritative AST → runtime
    └──→ Tree-sitter parser (tree-sitter-animatix) → parse tree → analyzer/LSP
```

- **Chumsky** produces `Vec<Stmt>` — the canonical AST used by the timeline compiler and renderer.
- **Tree-sitter** produces a CST (concrete syntax tree) used only for **position lookups** (line/col) and **context detection** (completions, hover, references).
- The `Analyzer` in `animatix-analyzer` runs both parsers, builds the symbol table from chumsky, then walks the tree-sitter tree to enrich symbols with real spans.

**Key implication:** The tree-sitter grammar must match the chumsky parser's language — not the other way around. The chumsky parser is the source of truth.

---

## Critical Issues

### C1. Analyzer References Non-Existent Tree-Sitter Node Kinds

**Location:** `crates/animat-analyzer/src/` (completer.rs, hover.rs, references.rs, lib.rs)

**Problem:** The analyzer references 12+ tree-sitter node kind strings that don't exist in the grammar. These code paths are dead — completions, hover, and position enrichment silently fail.

| Analyzer References | Grammar Has | Files Affected |
|---|---|---|
| `"action_statement"` | `action_invocation` | `completer.rs` |
| `"for_statement"` | `for_block` | `lib.rs` (enrich_positions) |
| `"duration_literal"` | `time_literal` | `hover.rs` |
| `"text_statement"` | `actor_declaration` | `completer.rs` |
| `"math_statement"` | `actor_declaration` | `completer.rs` |
| `"svg_statement"` | `actor_declaration` | `completer.rs` |
| `"code_statement"` | `actor_declaration` | `completer.rs` |
| `"image_statement"` | `actor_declaration` | `completer.rs` |
| `"type_identifier"` | `identifier` | `hover.rs`, `references.rs`, `completer.rs` |
| `"property_block"` | — (doesn't exist) | `completer.rs` |
| `"declaration_property_list"` | — (doesn't exist) | `completer.rs` |
| `"text_shorthand"` | — (chumsky-only) | `lib.rs` (enrich_positions) |
| `"drive_statement"` | — (no drive in grammar) | `lib.rs` (enrich_positions) |
| `"percentage"` | — (no dedicated node) | `hover.rs` |

**Fix Plan:**

1. **Decide on naming convention:** Either rename grammar rules to match the analyzer, or update the analyzer to use the grammar's names. Recommendation: update the analyzer — the grammar names are more descriptive.
2. **Update analyzer references:**
   - `completer.rs`: Change `"action_statement"` → `"action_invocation"`
   - `lib.rs`: Change `"for_statement"` → `"for_block"`
   - `hover.rs`: Change `"duration_literal"` → `"time_literal"`
   - `completer.rs`: Remove or rework `find_actor_type()` — it references 5 non-existent node kinds. Instead, check if an `actor_declaration` has a `type` field matching the expected actor type.
   - `hover.rs`, `references.rs`, `completer.rs`: Change `"type_identifier"` → `"identifier"` (tree-sitter uses `identifier` for everything)
   - `completer.rs`: Remove `"property_block"` and `"declaration_property_list"` references — dead code
   - `lib.rs`: Remove `"text_shorthand"` enrichment — chumsky-only concept, not representable in tree-sitter
3. **Add missing grammar rules first** (see C2) for `"drive_statement"`, `"percentage"`, etc., then update analyzer to reference them.

**Estimated effort:** 2–3 hours

---

### C2. Missing Language Features in Grammar

**Location:** `tree-sitter-animatix/grammar.js`

**Problem:** Features present in the spec and chumsky parser are absent from the tree-sitter grammar. This means the tree-sitter parser produces error nodes for these constructs, and the analyzer can't provide tooling for them.

| Feature | Spec | Chumsky | tree-sitter | Example |
|---|---|---|---|---|
| `drive` blocks | ❌ removed | ❌ removed | ❌ removed | Intentionally removed (sugar over `always`) |
| `:=` reactive binding | ✅ | ✅ | ❌ | `prop := expr` |
| `use` statements | ✅ | ✅ | ❌ | `use module.path` |
| `null` literal | ✅ | ✅ | ❌ | `let x = null` |
| `if` as expression | ✅ | ✅ | ❌ | `let x = if cond { a } else { b }` |
| `percentage` literal | ✅ | ✅ | ❌ | `50%`, `75%` |
| `auto` keyword | ✅ | ✅ | ❌ | `color: auto` |
| `fill` keyword | ✅ | ✅ | ❌ | `size: fill` |

**Fix Plan — Add each missing rule to `grammar.js`:**

#### 2a. `null` literal

```js
// Add to _expression choice:
$.null_literal,

// Add new rule:
null_literal: $ => 'null',
```

Also add `'null'` to the reserved keywords list if one exists, and add to highlights:
```scheme
"null" @keyword
```

#### 2b. `if` as expression

The current grammar has `if_statement` as a `_statement` choice. To support `if` in expression position:

```js
// Add to _expression choice:
$.if_expression,

// Add new rule (distinct from if_statement):
if_expression: $ => prec.left(seq(
  'if',
  field('condition', $._expression),
  field('consequence', $.block),
  optional(seq('else', field('alternative', $.block)))
)),

// Keep if_statement for backwards compatibility or merge them:
if_statement: $ => $.if_expression,  // alias
```

**Decision needed:** Should `if_statement` and `if_expression` be merged into a single rule? The chumsky parser treats `if` as an expression everywhere. Recommendation: merge — make `if_expression` the rule, add it to both `_statement` and `_expression` choices.

#### 2c. `percentage` literal

```js
// Add to _expression choice:
$.percentage,

// Add new rule:
percentage: $ => seq($.number, '%'),
```

**Note:** This may conflict with the `%` (modulo) operator. Need to ensure `%` after a number is lexed as part of `percentage`, not as binary modulo. Tree-sitter's longest-match rule should handle this: `50%` → `percentage`, `a % b` → `binary_expression`.

Also add to highlights:
```scheme
(percentage) @number
```

#### 2d. `drive` block

```js
// Add to _statement choice:
$.drive_block,

// Add new rule:
drive_block: $ => seq(
  'drive',
  $.block
),
```

Also add `'drive'` to keywords in highlights.scm.

#### 2e. `:=` reactive binding

```js
// Add to _statement choice:
$.reactive_binding,

// Add new rule:
reactive_binding: $ => seq(
  field('target', $.path_expression),
  ':=',
  field('value', $._expression),
  optional($.modifier_block)
),
```

**Note:** `:=` must be a distinct token from `:` (property separator) and `=` (assignment). Tree-sitter should handle this since `:=` is two characters.

#### 2f. `use` statement

```js
// Add to _statement choice:
$.use_statement,

// Add new rule:
use_statement: $ => seq(
  'use',
  field('path', $.path_expression)
),
```

Also add `'use'` to keywords in highlights.scm.

#### 2g. `auto` and `fill` keywords

These are used as special values, not as standalone statements. Two options:

**Option A — Treat as reserved identifiers:**
```js
// Add to _expression choice (or handle in identifier rule):
// No change needed — they parse as identifiers. Update analyzer to recognize them.
```

**Option B — Add as keywords:**
```js
auto: $ => 'auto',
fill: $ => 'fill',

// Add to _expression choice:
$.auto,
$.fill,
```

**Recommendation:** Option A — they're context-dependent values, not general keywords. The analyzer/chumsky handles semantic meaning. Just ensure the highlights capture them:
```scheme
"auto" @constant.builtin
"fill" @constant.builtin
```

**Estimated effort:** 3–4 hours

---

## Medium Issues

### M1. `keyframe` Rule Redundancy

**Location:** `grammar.js:88-93`

**Problem:** The `keyframe` rule has a redundant `choice` branch:
```js
keyframe: $ => seq(
  '#',
  choice(
    seq(optional('+'), $.number, optional($.time_unit)),  // covers both with and without +
    seq('+', $.number, optional($.time_unit))              // entirely redundant
  )
)
```

**Fix:** Simplify to:
```js
keyframe: $ => seq('#', optional('+'), $.number, optional($.time_unit))
```

**Estimated effort:** 2 minutes

---

### M2. `block` vs `children_block` Are Identical

**Location:** `grammar.js`

**Problem:** Both rules produce different node types but have identical structure:
```js
block: $ => seq('{', repeat($._statement), '}'),
children_block: $ => seq('{', repeat($._statement), '}'),
```

**Fix:** Either:
- **Option A:** Alias one to the other: `children_block: $ => alias($.block, $.children_block)`
- **Option B:** Keep both (for semantic clarity in the AST) — no code change needed, just document the distinction.

**Recommendation:** Option B — the semantic distinction is useful for the analyzer.

**Estimated effort:** 5 minutes (documentation only)

---

### M3. `parameter` Rule Has Confusing Dual-Optional Structure

**Location:** `grammar.js:58-63`

**Problem:** Two overlapping `optional()` calls make the rule fragile:
```js
parameter: $ => seq(
  field('name', $.identifier),
  optional(seq(':', field('type', $.type_annotation), optional(seq('=', field('default', $._expression))))),
  optional(seq('=', field('default', $._expression)))
),
```

**Fix:** Collapse to a single `optional(choice(...))`:
```js
parameter: $ => seq(
  field('name', $.identifier),
  optional(choice(
    seq(':', field('type', $.type_annotation), optional(seq('=', field('default', $._expression)))),
    seq('=', field('default', $._expression))
  ))
),
```

**Estimated effort:** 5 minutes

---

### M4. Missing Highlights for `#`, `=>`, `@`

**Location:** `queries/highlights.scm`

**Problem:** Several tokens are not highlighted:
- `#` in `scene_declaration` and `keyframe`
- `=>` in `closure_expression`
- `@` in `slot_fill`

**Fix:**
```scheme
; Add to highlights.scm:

; Scene/keyframe prefix
(scene_declaration "#" @punctuation.special)
(keyframe "#" @punctuation.special)

; Closure arrow
"=>" @operator

; Slot fill prefix
(slot_fill "@" @punctuation.special)
```

**Estimated effort:** 5 minutes

---

### M5. Missing `rerun-if-changed` for `grammar.js`

**Location:** `crates/tree-sitter-animatix/build.rs`

**Problem:** If `grammar.js` is modified and `parser.c` is regenerated, cargo may not rebuild the crate.

**Fix:**
```rust
println!("cargo:rerun-if-changed=../../tree-sitter-animatix/grammar.js");
println!("cargo:rerun-if-changed=../../tree-sitter-animatix/src/parser.c");
```

**Estimated effort:** 2 minutes

---

### M6. Fragile Relative Paths in Build Script and Lib

**Location:** `crates/tree-sitter-animatix/build.rs`, `crates/tree-sitter-animatix/src/lib.rs`

**Problem:** Hardcoded relative paths break if the workspace layout changes:
```rust
// build.rs
let src_dir = std::path::Path::new("../../tree-sitter-animatix/src");

// lib.rs
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../../tree-sitter-animatix/queries/highlights.scm");
```

**Fix:** Use `CARGO_MANIFEST_DIR` for robust path resolution:
```rust
// build.rs
fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = manifest_dir.join("../../tree-sitter-animatix/src");
    // ...
}
```

For `include_str!`, the path is resolved relative to the file, so this is harder to fix. Options:
- Keep the relative path (it works in the monorepo)
- Copy `highlights.scm` into the crate at build time and `include_str!` from there
- Use a build script to embed the query as a string constant

**Recommendation:** Keep the relative path for now — document the layout requirement. Only fix if the crate is extracted from the monorepo.

**Estimated effort:** 10 minutes

---

### M7. Analyzer `find_actor_type()` References Non-Existent Node Kinds

**Location:** `crates/animat-analyzer/src/completer.rs`

**Problem:** The function checks for `"text_statement"`, `"math_statement"`, `"svg_statement"`, `"code_statement"`, `"image_statement"` — none of which exist. The grammar uses `actor_declaration` for all of these.

**Fix:** Rework the function to:
1. Check if the node is an `actor_declaration`
2. Read the `type` field
3. Match on the type's text content (`"Text"`, `"Math"`, `"Svg"`, `"Code"`, `"Image"`)

```rust
fn find_actor_type(node: tree_sitter::Node) -> Option<&str> {
    if node.kind() != "actor_declaration" { return None; }
    let type_node = node.child_by_field_name("type")?;
    match type_node.text() {
        "Text" => Some("text"),
        "Math" => Some("math"),
        "Svg" => Some("svg"),
        "Code" => Some("code"),
        "Image" => Some("image"),
        _ => None,
    }
}
```

**Estimated effort:** 30 minutes

---

## Low Issues

### L1. No Corpus Tests

**Location:** `tree-sitter-animatix/` (missing `test/corpus/`)

**Problem:** Standard tree-sitter practice is to have `test/corpus/*.txt` files with test cases. The grammar has no formal test suite.

**Fix:** Create `test/corpus/` with test files covering:
- All statement types
- Expression precedence and associativity
- Edge cases (empty blocks, nested expressions, long chains)
- Error recovery behavior

Example test file structure:
```
test/corpus/
├── statements.txt      # actor_declaration, let_declaration, etc.
├── expressions.txt     # binary, unary, call, method, index, etc.
├── keyframes.txt       # absolute, relative, scene declarations
├── components.txt      # component_definition, action_definition, slots
├── control_flow.txt    # if, for, sequence, stagger, always
└── edge_cases.txt      # empty blocks, nested expressions, errors
```

**Estimated effort:** 2–3 hours

---

### L2. No README for Grammar or Crate

**Location:** `tree-sitter-animatix/`, `crates/tree-sitter-animatix/`

**Fix:** Add brief READMEs:
- Grammar README: what the grammar covers, how to regenerate `parser.c`, how to run tests
- Crate README: what the crate exports, how to use it, link to the grammar

**Estimated effort:** 30 minutes

---

### L3. Missing `Cargo.toml` Metadata

**Location:** `crates/tree-sitter-animatix/Cargo.toml`

**Fix:** Add:
```toml
license = "MIT OR Apache-2.0"  # or whatever the project uses
repository = "https://github.com/..."
keywords = ["tree-sitter", "animatix", "animation", "dsl"]
categories = ["parser-implementations"]
```

**Estimated effort:** 5 minutes

---

### L4. No `package.json` for Grammar Source

**Location:** `tree-sitter-animatix/`

**Problem:** Standard tree-sitter grammars include a `package.json` with metadata and scripts.

**Fix:** Create:
```json
{
  "name": "tree-sitter-animatix",
  "version": "0.1.0",
  "description": "Tree-sitter grammar for Animatix DSL",
  "main": "bindings/node",
  "scripts": {
    "generate": "tree-sitter generate",
    "test": "tree-sitter test"
  },
  "devDependencies": {
    "tree-sitter-cli": "^0.22"
  }
}
```

**Estimated effort:** 10 minutes

---

### L5. Over-Declared Conflicts May Mask Real Ambiguities

**Location:** `grammar.js` conflicts array

**Problem:** 20 conflict declarations, some may be unnecessary. Over-declared conflicts slow down the parser and mask real ambiguities.

**Fix:**
1. Run `tree-sitter generate` and check for warnings
2. Try removing each conflict one at a time and run `tree-sitter test` (once corpus tests exist)
3. Document why each remaining conflict is necessary

**Estimated effort:** 1–2 hours (after corpus tests exist)

---

### L6. `@slot` Reserves the Name "slot"

**Location:** `grammar.js` — `slot_marker` vs `slot_fill`

**Problem:** `slot_marker` matches the literal `@slot`, so you can never create a slot named "slot" — `@slot { content }` would parse as `slot_marker` followed by a block.

**Fix:** Document this as a language rule: "slot" is a reserved slot name. No code change needed.

**Estimated effort:** 5 minutes (documentation)

---

### L7. Identifier Regex Allows Hyphens — Document as Language Rule

**Location:** `grammar.js:164`

**Problem:** `a-b` is lexed as one identifier, not `a - b`. This is correct per the spec (hyphens are allowed in identifiers), but surprising for users.

**Fix:** Add to language documentation:
> Hyphens are allowed in identifiers (e.g., `fade-in`, `ease-out`). When using the subtraction operator, always use spaces: `a - b`, not `a-b`.

**Estimated effort:** 5 minutes (documentation)

---

### L8. String Regex Disallows Escape Sequences — Document as Language Rule

**Location:** `grammar.js:158-161`

**Problem:** No `\n`, `\"`, `\\` escape sequences. This matches the spec ("no string escape sequences"), but should be documented.

**Fix:** Add to language documentation:
> Strings do not support escape sequences. To include a quote, use the other delimiter: `"it's fine"` or `'he said "hi"'`.

**Estimated effort:** 5 minutes (documentation)

---

### L9. Missing Highlights for `auto`, `fill`, `null`

**Location:** `queries/highlights.scm`

**Fix:** Add (once `null` is added to grammar):
```scheme
"null" @constant.builtin
"auto" @constant.builtin
"fill" @constant.builtin
```

**Estimated effort:** 2 minutes

---

## Informational (No Action Needed)

| # | Finding | Status |
|---|---|---|
| I1 | `parser.c` (295KB) checked into repo | ✅ Standard tree-sitter practice |
| I2 | `unsafe extern "C"` syntax correct for Rust 2024 | ✅ Correct |
| I3 | `object_expression` vs `call_expression` disambiguated | ✅ `Foo{}` vs `Foo()` |
| I4 | `actor_declaration` vs `property_assignment` disambiguated | ✅ `:` vs `.` |
| I5 | No catastrophic backtracking risks in regexes | ✅ All patterns safe |
| I6 | `scene_declaration` vs `keyframe` disambiguated | ✅ `identifier` vs `number` |
| I7 | Expression precedence matches spec | ✅ Standard operator precedence |
| I8 | `path_expression` handles `a.b.c` correctly | ✅ `prec.left` recursion |
| I9 | `modifier_block` syntax matches spec | ✅ `[duration, key: value]` |

---

## Fix Phases

### Phase 1: Critical — Grammar/Analyzer Alignment (Week 1)

| Step | Task | Files | Effort |
|---|---|---|---|
| 1.1 | Add missing grammar rules: `null`, `if_expression`, `percentage`, `drive_block`, `reactive_binding`, `use_statement` | `grammar.js` | 2h |
| 1.2 | Regenerate `parser.c` | `tree-sitter generate` | 5min |
| 1.3 | Update analyzer node kind references | `completer.rs`, `hover.rs`, `references.rs`, `lib.rs` | 2h |
| 1.4 | Rework `find_actor_type()` | `completer.rs` | 30min |
| 1.5 | Run `cargo test -p animatix-analyzer` | — | 15min |
| 1.6 | Run `cargo test -p animatix` | — | 15min |

**Total Phase 1:** ~5 hours

### Phase 2: Medium — Grammar Polish (Week 1–2)

| Step | Task | Files | Effort |
|---|---|---|---|
| 2.1 | Simplify `keyframe` rule | `grammar.js` | 2min |
| 2.2 | Collapse `parameter` dual-optional | `grammar.js` | 5min |
| 2.3 | Add missing highlights (`#`, `=>`, `@`, `null`, `auto`, `fill`) | `highlights.scm` | 10min |
| 2.4 | Regenerate `parser.c` | `tree-sitter generate` | 5min |
| 2.5 | Add `rerun-if-changed` for `grammar.js` | `build.rs` | 2min |
| 2.6 | Document relative path layout requirement | `build.rs` comments | 5min |

**Total Phase 2:** ~30 minutes

### Phase 3: Low — Testing & Documentation (Week 2)

| Step | Task | Files | Effort |
|---|---|---|---|
| 3.1 | Create corpus test suite | `test/corpus/*.txt` | 3h |
| 3.2 | Add READMEs | `tree-sitter-animatix/README.md`, `crates/tree-sitter-animatix/README.md` | 30min |
| 3.3 | Add `Cargo.toml` metadata | `Cargo.toml` | 5min |
| 3.4 | Add `package.json` for grammar | `package.json` | 10min |
| 3.5 | Document hyphen/escape language rules | `docs/spec.md` | 10min |
| 3.6 | Audit and trim conflict declarations | `grammar.js` | 2h |

**Total Phase 3:** ~6 hours

---

## Testing Strategy

### Before Each Phase
```bash
# Ensure existing tests pass
cargo test -p animatix
cargo test -p animatix-gui
cargo test -p tree-sitter-animatix
```

### After Phase 1
```bash
# Verify new grammar rules parse correctly
tree-sitter parse examples/06_reactive.amx    # drive, if-expression
tree-sitter parse examples/11_colors.amx       # auto
tree-sitter parse examples/08_effects.amx     # blur, brightness, sepia

# Verify analyzer works with updated node kinds
cargo test -p animatix-analyzer
```

### After Phase 2
```bash
# Verify highlights render correctly
tree-sitter highlight examples/00_hello.amx
tree-sitter highlight examples/09_components.amx

# Verify build script rebuilds correctly
touch tree-sitter-animatix/grammar.js
cargo build -p tree-sitter-animatix
```

### After Phase 3
```bash
# Run full corpus test suite
cd tree-sitter-animatix && tree-sitter test

# Run all workspace tests
cargo test --workspace
```

---

## Appendix: Grammar Rule Reference

### Current Rules (19 statement types)
```
comment, config, import_statement, let_declaration, component_definition,
action_definition, scene_declaration, keyframe, actor_declaration,
property_assignment, action_invocation, sequence_block, stagger_block,
always_block, for_block, if_statement, play_statement, slot_marker, slot_fill
```

### Missing Rules (to add in Phase 1)
```
null_literal, if_expression, percentage, drive_block, reactive_binding, use_statement
```

### Current Expression Types (16)
```
number, time_literal, string, boolean, identifier, path_expression,
unary_expression, binary_expression, call_expression, index_expression,
tuple_expression, array_expression, closure_expression, object_expression,
parenthesized_expression, method_call_expression
```

### Missing Expression Types (to add in Phase 1)
```
null_literal, if_expression, percentage
```

### Node Kind Mapping (analyzer → grammar)

| Analyzer Uses | Grammar Has | Action |
|---|---|---|
| `action_statement` | `action_invocation` | Update analyzer |
| `for_statement` | `for_block` | Update analyzer |
| `duration_literal` | `time_literal` | Update analyzer |
| `type_identifier` | `identifier` | Update analyzer |
| `text_statement` | `actor_declaration` | Rework `find_actor_type()` |
| `math_statement` | `actor_declaration` | Rework `find_actor_type()` |
| `svg_statement` | `actor_declaration` | Rework `find_actor_type()` |
| `code_statement` | `actor_declaration` | Rework `find_actor_type()` |
| `image_statement` | `actor_declaration` | Rework `find_actor_type()` |
| `property_block` | — | Remove dead code |
| `declaration_property_list` | — | Remove dead code |
| `text_shorthand` | — | Remove dead code |
| `drive_statement` | (new) `drive_block` | Add grammar rule, update analyzer |
| `percentage` | (new) `percentage` | Add grammar rule, update analyzer |
