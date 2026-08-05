# Example-audit gap remediation plan

## Goal
Close four parser/typing gaps surfaced by the example audit so documented `.amx`
syntax parses, builds, and type-checks without spurious diagnostics.

## Findings (verified)

- **PEG parser is the build path.** `crates/animatix/src/main.rs:742`,
  `composition.rs`, and `timeline/utils.rs` all build from
  `animatix_syntax::parser::parser_simple()`. Tree-sitter (`ts_convert.rs`) is
  only used for highlighting/analysis. So gaps that "tree-sitter supports" are
  still real for the engine.
- **`Expr::Index` runtime is done.** Tree-walker (`timeline/utils.rs:278`) and
  VM/IR (`modifier_runtime/ir/lower.rs:189`, `ir/eval.rs:180`, `vm.rs:295`) both
  evaluate `Index`. The gap is purely in the PEG parser front-end + target
  resolution, not evaluation.
- **Array labels expand to `{label}__{index}`** via
  `timeline/build/process.rs:336` (`resolve_array_index`). `dots[0]` must resolve
  to track `dots__0`.
- **PEG expression parser** (`parser/expr.rs`) `access` chain handles `.field`
  and method calls only — no `[...]` subscript. `items[0]` currently fails in the
  build path even though spec.md:1400 documents it.
- **PEG targets/assignments** use `common::dotted_ident()` (`parser/common.rs`),
  which splits on `.` only. `fade-in dots[0]` and `dots[0].at = ...` cannot parse.
- **`Any` type:** tree-sitter grammar.js:140 already lists `'Any'`; PEG
  `type_annotation` in `parser/stmt.rs` (the `simple` choice, ~line 74) omits it.
  `ts_convert` does not extract `param_type` at all (`ts_convert.rs:893`), so it is
  unaffected. AST + Display already support `TypeAnnotation::Any`
  (`ast.rs:380,395`).
- **`color`/`blend` on actions:** `highlight` declares them in its signature
  (`actions/highlight.rs` `highlight_timing_params`) and reads them directly, but
  `parse_timing_modifiers` (`timeline/timing.rs`, `Some(name) =>` catch-all near
  line ~515) emits `UnsupportedModifierKey` for any key it does not itself handle.
  Precedent for skipping action-specific keys exists: `Some("intensity" |
  "frequency") => {}`.
- **`accent.*` typed as Any:** both `typecheck.rs` `expr_type` (`Expr::Path` arm,
  line ~354 only handles `len()==1`, multi-segment falls to `_ => Any`) and
  analyzer `symbol_table.rs:686` (`Expr::Path(_) => Any`). Colorscheme namespaces
  are fixed: `accent.*`, `text.*`, `surface.*`, `stroke.*` are always colors;
  `scene.*` is mixed (`scene.background` is a color but `scene.right`/`scene.bottom`
  are anchors — do NOT type `scene.*` as Color).

## Independence
- Gap 2 (`Any`), Gap 3 (modifier validator), Gap 4 (path typing) are fully
  independent and can land in any order / parallel.
- Gap 1 has two independent sub-parts: (1a) expression subscript, (1b)
  target/assignment subscript. 1b is the documented headline case
  (`fade-in dots[0]`, `dots[0].at`). 1a (`items[0]` in value position) is a
  smaller separate fix. Do 1a first since 1b can reuse the index atom.

---

## Plan

### Gap 2 — `Any` in PEG type annotations (smallest, do first)

1. **Add `Any` to the PEG `type_annotation` simple choice.**
   - File/fn: `crates/animatix-syntax/src/parser/stmt.rs`, the `simple = choice((…))`
     inside `let type_annotation = recursive(...)` (~line 74).
   - Change: add `text::keyword("Any").to(TypeAnnotation::Any),` to the choice.
   - Outcome: `action f(x: Any = 0) {}` and `component C(p: Any) {}` parse with
     `param_type: Some(TypeAnnotation::Any)`.
   - Verify: add a unit test in `parser/stmt.rs` or `parser/mod.rs` tests asserting
     a param with `Any` annotation parses; `cargo test -p animatix-syntax`.

2. **Justification note + sync check.** tree-sitter already has `Any`
   (grammar.js:140) so no grammar change needed; note this in the commit body.
   - Verify: `bash scripts/check-parser-sync.sh`.

### Gap 3 — `color`/`blend` (and other action-declared) modifiers not flagged

3. **Stop the generic validator from flagging action-specific signature keys.**
   - File/fn: `crates/animatix/src/timeline/timing.rs`, `parse_timing_modifiers`,
     the `Some(name) => push_modifier_diagnostic(... UnsupportedModifierKey ...)`
     catch-all (~line 515).
   - Preferred change: extend the existing skip arm
     `Some("intensity" | "frequency") => {}` to also cover the action-effect keys
     that actions consume directly: add `"color" | "blend" | "padding" | "radius"`.
     Mirror the existing comment ("valid but not timing modifiers").
   - Rationale: these are declared in `highlight`'s `ActionSignature.modifiers`
     and read in `Highlight::execute`; the timing parser only validates timing
     keys, so it must not warn on effect keys.
   - Outcome: `highlight f1 [color: accent.danger, blend: difference]` builds with
     no `unsupported-modifier-key` warning.
   - Verify: add a test in `actions/highlight.rs` `mod tests` building a highlight
     action with `color`/`blend` modifiers and asserting zero warning-severity
     `UnsupportedModifierKey` diagnostics; `cargo test -p animatix --lib`.

   - Alternative (more general, larger): thread the executing action's
     `ActionSignature.modifiers` into `parse_timing_modifiers` and skip any key
     present there. Better long-term but touches every `BuiltinAction::execute`
     caller and the fn signature — only do this if a second action needs custom
     keys. Note the tradeoff in the commit if choosing the hardcoded list.

### Gap 4 — colorscheme path expressions typed as `Any`

4. **Type fixed colorscheme namespaces as `Color` in the analyzer.**
   - File/fn: `crates/animatix-analyzer/src/symbol_table.rs`,
     `infer_expr_type`, `Expr::Path(_)` arm (line 686).
   - Change: match the first segment; if it is `accent | text | surface | stroke`
     and the path has ≥2 segments, return `PropertyType::Color`; otherwise keep
     `PropertyType::Any`. Do NOT include `scene` (mixed color/anchor).
   - Outcome: `color: accent.danger` no longer infers `Any`; type-mismatch check
     in `diagnostics.rs:392` can now validate it against `Color` props.
   - Verify: unit test in `symbol_table.rs` `mod tests` asserting
     `infer_expr_type(Path(["accent","danger"])) == Color` and
     `Path(["scene","right"]) == Any`; `cargo test -p animatix-analyzer`.

5. **Mirror the typing in the strict-mode typechecker.**
   - File/fn: `crates/animatix-syntax/src/typecheck.rs`, `expr_type`,
     add an `Expr::Path(parts) if parts.len() >= 2` arm before the `len()==1` arm
     (line ~354) using the same namespace→Color rule.
   - Outcome: strict mode treats `accent.*`/`text.*`/`surface.*`/`stroke.*` as
     `Color`, enabling strict component-prop checking instead of `Any`-passes.
   - Verify: unit test in `typecheck.rs` `mod tests`; `cargo test -p animatix-syntax`.
   - Note: keep the namespace set identical between steps 4 and 5; consider a
     shared helper if duplication is a concern (the two crates do not share a
     module, so a small duplicated `const`/fn with a cross-referencing comment is
     acceptable).

### Gap 1a — subscript in PEG value expressions (`items[0]`)

6. **Add an index/subscript fold to the PEG expression parser.**
   - File/fn: `crates/animatix-syntax/src/parser/expr.rs`, the `access` combinator
     (~line 230). Add a postfix `[ expr ]` fold producing `Expr::Index`, applied at
     the same precedence as field access (after `atom`, before `pow`). Indices use
     the full `expr` (allows `items[i]`, `items[n+1]`).
   - Outcome: `let first = items[0]` and `pos[1]` parse to `Expr::Index` in the
     build path; runtime already evaluates them.
   - Verify: unit test asserting `items[0]` → `Expr::Index(...)`;
     `cargo test -p animatix-syntax` and `cargo test -p animatix --lib`.
   - Risk: precedence vs the `pow`/`product` chain and vs modifier `[` brackets.
     `Index` must bind tighter than binary ops and only attach when `[` immediately
     follows a postfix atom (no padding before `[`, matching tree-sitter's
     `token.immediate('[')`). Reject `time_literal`/`percent` as bare index to
     avoid colliding with modifier lists, mirroring grammar.js `index_value`.

### Gap 1b — array-indexed targets (`fade-in dots[0]`, `dots[0].at = …`)

7. **Add an indexed-target parser that rewrites to the `__{index}` label form.**
   - File/fn: `crates/animatix-syntax/src/parser/common.rs`. Add a helper
     `indexed_segment` parsing `ident ( '[' integer ']' )?` and, when an index is
     present and is a non-negative integer literal, emit the segment string
     `format!("{ident}__{index}")`. Build `indexed_dotted_ident` from it
     (dot-separated `indexed_segment`s) returning `Vec<String>`.
   - Outcome: a path like `dots[0].at` becomes segments `["dots__0", "at"]`,
     matching `resolve_array_index`'s scheme so existing target resolution works
     unchanged.
   - Verify: unit test on the helper; `cargo test -p animatix-syntax`.

8. **Use the indexed path parser in `action_target` and `assignment`/`reactive_binding`.**
   - File/fn: `crates/animatix-syntax/src/parser/stmt.rs`: replace `dotted_ident`
     with the new `indexed_dotted_ident` in `action_target` (~line 332) and in the
     `assignment` (~line 138) and `reactive_binding` (~line 185) target positions.
   - Outcome: `fade-in dots[0] [300ms]` and `dots[0].at = (10,10)` parse and build,
     hitting tracks `dots__0`.
   - Verify: parser unit tests + an integration test in
     `crates/animatix/tests/parser_tests.rs` building a scene with an array decl
     (`dots[3]: Rect …`) plus `fade-in dots[0]` and `dots[0].opacity = 1`,
     asserting no errors and the track exists. `cargo test -p animatix`.
   - Decision (variable index in targets): scope this task to **integer-literal**
     indices only (covers spec examples `dots[0]`). A variable index like
     `dots[i]` in a target requires build-time evaluation against the loop env and
     a new indexed-target AST representation; defer it and emit a clear parse
     diagnostic ("array index in a target must be an integer literal") so it fails
     loud rather than silently. State this limitation in the commit + spec note.

9. **Update spec/docs if behavior differs from documentation.**
   - File: `docs/spec.md` around lines 852–889 / 1400. Confirm `fade-in bars[0]`
     and indexed assignment are documented as integer-literal only; add the
     variable-index limitation note if step 8 defers it.
   - Verify: docs review only.

### Cross-cutting verification (run before commit, per AGENTS.md)
- `cargo check --workspace`
- `cargo test -p animatix-syntax`
- `cargo test -p animatix --lib`
- `cargo test -p animatix-analyzer`
- `cargo test --no-fail-fast`
- `bash scripts/check-parser-sync.sh` (Gap 2 touches type grammar surface; rest
  do not change `.amx` token shapes, but run it to be safe)

## Files to touch
- `crates/animatix-syntax/src/parser/stmt.rs` — add `Any` to type grammar (G2);
  swap to indexed path parser in targets/assignments (G1b).
- `crates/animatix-syntax/src/parser/expr.rs` — postfix `Index` fold (G1a).
- `crates/animatix-syntax/src/parser/common.rs` — `indexed_segment` /
  `indexed_dotted_ident` helpers (G1b).
- `crates/animatix-syntax/src/typecheck.rs` — multi-segment colorscheme path →
  Color (G4).
- `crates/animatix/src/timeline/timing.rs` — skip action effect keys in
  `parse_timing_modifiers` (G3).
- `crates/animatix/src/timeline/actions/highlight.rs` — add no-warning test (G3).
- `crates/animatix-analyzer/src/symbol_table.rs` — colorscheme path typing (G4).
- `crates/animatix/tests/parser_tests.rs` — indexed-target integration test (G1b).
- `docs/spec.md` — confirm/limit indexed-target docs (G1b).

## Risks
- **G1b naming coupling:** the `__{index}` rewrite hard-codes
  `resolve_array_index`'s scheme. If that scheme ever changes, the parser rewrite
  drifts. Mitigate with a cross-referencing comment in both places; consider a
  shared `const SEP: &str = "__"` later.
- **G1a precedence/ambiguity:** subscript `[` vs modifier-list `[`. Use
  immediate-bracket (no leading whitespace) and exclude time/percent index values,
  matching tree-sitter; add tests for `fade-in x [300ms]` (modifier, not index)
  vs `x[0]` (index) to guard the boundary.
- **G3 list drift:** hardcoding `color|blend|padding|radius` means a future action
  with a new effect key re-triggers the warning. The signature-threading
  alternative avoids this; pick it if a second custom-key action appears.
- **G4 scene mixing:** typing `scene.*` as Color would wrongly flag anchor uses
  (`scene.right`). Excluded by design — keep it excluded.
- **Two eval paths:** G1a/G1b only touch parsing; `Index` already exists in both
  tree-walker and VM, so no VM/IR change is needed. Confirm via the indexed-target
  integration test which exercises the build path.
- **Ordering:** land G2/G3/G4 first (isolated, low risk), then G1a, then G1b
  (depends on the index atom and is the largest blast radius).
