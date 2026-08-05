# Animatix — Fix Plans (Part A) + Design Proposals (Part B)

Grounded in `docs/spec.md`, `docs/architecture.md`, and source verification.
No files were edited; this is a plan only.

---

# PART A — Fix Plans (5 confirmed issues)

## Sequencing / parallelism summary

| Task | Primary file(s) | Can parallel with |
|------|-----------------|-------------------|
| **B1-core** | `timeline/build/entry.rs` | everything (disjoint file) |
| **B2** | `main.rs` | everything (disjoint file) |
| **L1** | `analyzer/symbol_table.rs` | B1-core, B2 (NOT L2/L3/B1-analyzer — same file) |
| **L2** | `analyzer/symbol_table.rs` | B1-core, B2 (NOT L1/L3/B1-analyzer — same file) |
| **L3** | `analyzer/symbol_table.rs` | B1-core, B2 (NOT L1/L2/B1-analyzer — same file) |
| **B1-analyzer** (follow-up) | `analyzer/symbol_table.rs`, `analyzer/diagnostics.rs` | B1-core, B2 (NOT L1/L2/L3 — same file) |

**Rule:** L1, L2, L3, B1-analyzer all mutate `crates/animatix-analyzer/src/symbol_table.rs` and must be applied **sequentially** (or merged into one commit). B1-core (`entry.rs`) and B2 (`main.rs`) are disjoint and fully parallel-safe.

---

## B1-core — Top-level `for`-loop actor generation is silently skipped

### Root cause
`Timeline::build_impl` in `crates/animatix/src/timeline/build/entry.rs` dispatches top-level statements at lines **233–248**. The match arm forwards `ActorDecl | Assignment | Sequence | Stagger | LetDecl | Always` to `process_body()`, but **`Stmt::ForLoop` is absent** from that list and falls into the `_ => {}` catch-all at **line 250** — silently dropped, no diagnostic.

For-loops **nested inside a keyframe body** (`Stmt::Keyframe { body, .. }` arm at line ~224 → `process_body`) ARE handled, because `process_body` in `crates/animatix/src/timeline/build/process.rs` has a `Stmt::ForLoop` arm at **lines 71–73** that calls `process_for_loop_stmts`. So the bug is specifically **top-level for-loops outside any keyframe** (the minimal repro: `for c,k in {red,white,blue} { a[k]: Rect, ... }` with no `#0s`).

### Files
- `crates/animatix/src/timeline/build/entry.rs` — `build_impl`, match at lines 233–250.

### Precise change
Add `Stmt::ForLoop { .. }` to the existing `|`-chain so top-level for-loops are forwarded to `process_body` with the same pre-keyframe opacity handling as the other statements:

```rust
Stmt::ActorDecl { .. }
| Stmt::Assignment { .. }
| Stmt::Sequence { .. }
| Stmt::Stagger { .. }
| Stmt::LetDecl { .. }
| Stmt::Always { .. }
| Stmt::ForLoop { .. } => {          // ← ADD this line
    let saved_opacity = timeline.default_opacity;
    if !has_seen_keyframe {
        timeline.default_opacity = 0.0;
    }
    timeline.process_body(
        current_build_time_ms,
        std::slice::from_ref(stmt),
        None,
        &mut diagnostics,
    );
    timeline.default_opacity = saved_opacity;
}
```

`process_body` already routes `Stmt::ForLoop` → `process_for_loop_stmts` (process.rs:71), which iterates `for_iter_values`, binds the loop var + index var, and re-enters `process_body` per iteration. `resolve_array_index` (process.rs ~line 330) evaluates the index `Expr` and produces `format!("{}__{}", label, n)`. No new logic needed — just stop dropping the statement.

### Expected outcome
A top-level `for mag, i in values { bars[i]: Rect, size: (12, mag*180) }` (no enclosing `#0s`) now creates `bars__0..bars__N` tracks. `fade-in bars[0]` no longer errors "target 'bars__0' is not declared yet". `lint` no longer says "Unused actor: 'bars'".

### Verification
```bash
cargo check --workspace
cargo test -p animatix --lib
# New regression test: parse + build a top-level-only for-loop, assert tracks contain bars__0..bars__2
# Render check on a minimal top-level for-loop example:
cargo run -p animatix -- image <(echo 'config { resolution: (200,200) }
for v, i in {40,80,60} { b[i]: Rect, size: (20, v), at: (i*30, 100), color: red }') --time 0 -o /tmp/b1.png
```
Then confirm `examples/28_generation_reactive.amx` (see note below).

### Note on example 28
`examples/28_generation_reactive.amx` places its `for (x,y), i in {...} { dots[i]: ... }` **after** `#0s` and **before** `#0.4s`. Per the parser (`parser/top_level.rs` keyframe = `#` + time + `stmt.repeated()`; `stmt` includes `for_stmt` per `parser/stmt.rs` choice list), that for-loop is collected into the `#0s` keyframe **body** and should already reach `process_body`. The fixer should verify empirically whether example 28's dots render after this fix:
- If they do → the top-level dispatch was the sole cause (the for-loop was somehow landing top-level).
- If they still don't → example 28 has a **secondary** issue (likely in `always` override resolution for `at` on for-loop-generated actors, or the `732px` measurement region); file a separate follow-up. **Do not expand B1-core's scope** to chase that here.

### Non-goals
- Adding a build-time warning when an `always` override targets a non-existent actor (separate follow-up; see B1-analyzer below).
- Changing `resolve_array_index` or the `__` naming convention.
- Fixing example 28's `always`/`at` behavior if unrelated to the top-level dispatch.

---

## B2 — `check` command skips the semantic analyzer (`lint` catches more)

### Root cause
`Commands::Check` in `crates/animatix/src/main.rs` (lines **518–615**) runs `ModuleGraph::load_program_with_source` → `program.typecheck()` → `BuildTarget::from_ast`, collecting only parse + typecheck + build diagnostics. It **never constructs `animatix_analyzer::Analyzer`**, so unknown-action, undefined-label, unused-label, unknown-property, type-mismatch, and duplicate-label diagnostics are absent from `check` but present in `lint` (lines 641–712, which uses `Analyzer::new_with_path` + `diagnostics_with_config`).

### Files
- `crates/animatix/src/main.rs` — `Commands::Check` arm, after the `diagnostics.extend(report.diagnostics)` at ~line 565.

### Precise change
After the existing build diagnostics are gathered (and before the `render_smoke` block / output formatting), invoke the analyzer the same way `lint` does, merging its diagnostics:

```rust
// After: diagnostics.extend(report.diagnostics);
let analyzer = animatix_analyzer::Analyzer::new_with_path(&source, if file_label == "-" { None } else { Some(std::path::Path::new(&file_label)) });
let lint_config = animatix_analyzer::LintConfig::from_source(&source);
let semantic = analyzer.diagnostics_with_config(&lint_config);
// Convert analyzer diagnostics to animatix_syntax::diagnostics::Diagnostic (or print inline)
// and merge into `diagnostics` (or a separate printed stream).
```
The analyzer's `Diagnostic` type (`animatix_analyzer::diagnostics::Diagnostic`) differs from `animatix_syntax::diagnostics::Diagnostic`. The check command currently prints `animatix_syntax` diagnostics via `format_diagnostic_with_source`. Either (a) map each analyzer diagnostic into a syntax `Diagnostic` with `DiagnosticCode::Lint*`/warning severity and the span from `line/col`, or (b) print them in a separate "lint:" stream. Option (a) keeps a single sorted output. Match the `format` (JSON/Text) branch already present.

### Expected outcome
`animatix check file.amx` now reports the same unknown-action/undefined-label/unknown-property warnings as `animatix lint file.amx`. The two commands agree.

### Verification
```bash
cargo check --workspace
# Before: check is silent on a file with an unknown action; after: it reports it
cargo run -p animatix -- check examples/12_reorder.amx
cargo run -p animatix -- lint examples/12_reorder.amx
# Diff the diagnostic sets — they should now overlap for semantic codes
```

### Non-goals
- Making `check` fail (exit 1) on lint warnings — keep current error/warning severity semantics; only merge the diagnostic set.
- Deduplicating diagnostics that both the build pass and the analyzer emit (e.g. some type mismatches); acceptable to show both for now, note as follow-up.

---

## L1 — `swap`/`reorder`/`reveal-in`/`bounce`/`pulse`/`shake`/`highlight`/`unhighlight` flagged as unknown-action (stale allowlist)

### Root cause
`BUILTIN_ACTIONS` const in `crates/animatix-analyzer/src/symbol_table.rs` (lines **152–157**) lists only 13 verbs: `fade-in, draw-in, wipe-in, fade-out, wipe-out, reveal-out, draw-out, move, shift, rotate, scale, persist, remove`. The runtime registry `get_builtin_actions()` in `crates/animatix/src/timeline/actions/mod.rs` (lines 269–293) returns **21** actions — the 8 missing ones are: `reveal-in, shake, pulse, bounce, highlight, unhighlight, swap, reorder`. `check_stmt` (`analyzer/diagnostics.rs` ~line 305) emits `unknown-action` when `!symbols.actions.contains(&action.verb)`.

### Constraint
`animatix-analyzer` has **no dependency on `animatix`** (the runtime crate) — by design (architecture.md §14, §18: the analyzer depends only on `animatix-syntax` to keep WGPU/Vello out of the LSP compile graph). So the allowlist cannot import `get_builtin_actions`. It must be maintained as a parallel static list. (A shared constants module in `animatix-syntax` is a future refactor; not required for this fix.)

### Files
- `crates/animatix-analyzer/src/symbol_table.rs` — `BUILTIN_ACTIONS` const, lines 152–157.

### Precise change
Append the 8 missing verbs to `BUILTIN_ACTIONS`, matching the runtime order in `get_builtin_actions()`:

```rust
const BUILTIN_ACTIONS: &[&str] = &[
    "fade-in", "draw-in", "wipe-in", "reveal-in",
    "fade-out", "wipe-out", "reveal-out", "draw-out",
    "move", "shift", "rotate", "scale",
    "shake", "pulse", "bounce",
    "highlight", "unhighlight",
    "persist", "remove",
    "swap", "reorder",
];
```
That is 21 entries — the exact set returned by `get_builtin_actions()`.

### Expected outcome
`animatix lint` / `check` no longer warns `Unknown action: swap` / `reorder` / `reveal-in` / `bounce` / `pulse` / `shake` / `highlight` / `unhighlight` on valid code. `examples/12_reorder.amx` (uses `swap`/`reorder`) and `examples/21_actions.amx` (uses effects/highlight) lint clean on those verbs.

### Verification
```bash
cargo check --workspace
cargo test -p animatix-analyzer
cargo run -p animatix -- lint examples/12_reorder.amx
cargo run -p animatix -- lint examples/21_actions.amx
# Assert no "unknown-action" diagnostics for swap/reorder/reveal-in/bounce/pulse/shake/highlight/unhighlight
```
Optional hardening: add a test that asserts `BUILTIN_ACTIONS` length == 21 and contains every name from a duplicated literal list, so future drift is caught.

### Non-goals
- Moving the action list to a shared `animatix-syntax` module (separate refactor; track as follow-up).
- Validating action **arity**/target shape (e.g. `swap` needs 2 targets) — only the verb allowlist here.

---

## L2 — Container child labels unresolved (analyzer doesn't recurse into `InlineItem`)

### Root cause
`SymbolTable::collect_stmt` in `crates/animatix-analyzer/src/symbol_table.rs`, the `Stmt::ActorDecl` arm (lines **~407–418**), inserts only the outer `label` into `self.labels` and never touches `children: Vec<InlineItem>`. No `InlineItem` variant is handled anywhere in `collect_stmt` / `collect_refs_from_stmt`. So a `row: Row { a: Rect, b: Rect }` registers `row` but not `a`/`b`, producing spurious `undefined-label` on `a`/`b` references and `unused-label` on the children.

### Files
- `crates/animatix-analyzer/src/symbol_table.rs` — `collect_stmt`, `Stmt::ActorDecl` arm (~lines 407–418). Add an `InlineItem` recursion helper.

### Precise change
In the `Stmt::ActorDecl` arm, after inserting the outer label, iterate `children` and register labels / recurse. Add a helper `collect_inline_item(&mut self, item: &InlineItem)`:

```rust
Stmt::ActorDecl { label, ty, children, span, .. } => {
    self.labels.insert(label.clone(), LabelInfo { /* Actor */ .. });
    for child in children {
        self.collect_inline_item(child);
    }
}
```
```rust
fn collect_inline_item(&mut self, item: &InlineItem) {
    match item {
        InlineItem::Labeled { label, ty, children, span, .. } => {
            self.labels.insert(label.clone(), LabelInfo { name: label.clone(), kind: LabelKind::Actor, ty: Some(ty.clone()), span: *span, .. });
            for child in children { self.collect_inline_item(child); }   // nested containers
        }
        InlineItem::ForLoop { var, index_var, body, span, .. } => {
            // mirror the Stmt::ForLoop arm: register loop vars, recurse into body items
            // (body is Vec<InlineItem>, not Vec<Stmt>)
            // register var(s) + index_var as LabelKind::For, then for item in body { self.collect_inline_item(item) }
        }
        InlineItem::Anonymous { children, .. } => {
            for child in children { self.collect_inline_item(child); }
        }
        InlineItem::SlotMarker => {}
        InlineItem::SlotFill { items, .. } => {
            for item in items { self.collect_inline_item(item); }
        }
    }
}
```
Confirm `InlineItem` variant names/fields against `crates/animatix-syntax/src/ast.rs` lines 455–510 (`Anonymous`, `Labeled`, `ForLoop`, `SlotMarker`, `SlotFill`).

### Expected outcome
`animatix lint examples/02_layout.amx` no longer emits `undefined-label` / `unused-label` for container children. Dotted references like `row.a` resolve.

### Verification
```bash
cargo check --workspace
cargo test -p animatix-analyzer
cargo run -p animatix -- lint examples/02_layout.amx
cargo run -p animatix -- lint examples/08_effects.amx
# Add a unit test: build a SymbolTable from a Row with two labeled children, assert all three labels present
```

### Non-goals
- Resolving component-instance-prefixed nested labels (e.g. `btn.badge`) — that uses a different namespacing path; keep this to literal `InlineItem` children.
- Expanding array-indexed children (`a[i]`) — that is B1-analyzer.

---

## L3 — `at` flagged as unknown-property

### Root cause
`known_properties()` in `crates/animatix-analyzer/src/symbol_table.rs`, the `common` vec (lines **171–179**), lists `position, anchor, offset, scale, rotation, opacity, color` — **`at` is absent**. `check_stmt` emits `unknown-property` (Info severity) at `analyzer/diagnostics.rs` lines **375** (assignment) and **437** (declaration) when `!known_props.contains(property)`. Since `at` is the idiomatic placement property (used pervasively in examples and `property_registry.rs` line 629 `schema!("at", ...)`), this is a high-noise false positive.

### Files
- `crates/animatix-analyzer/src/symbol_table.rs` — `known_properties()`, `common` vec (line 171), and optionally `known_property_types()`.

### Precise change
Add `"at"` to `common`:
```rust
let common = vec![
    "position".to_string(),
    "anchor".to_string(),
    "offset".to_string(),
    "at".to_string(),          // ← ADD
    "scale".to_string(),
    "rotation".to_string(),
    "opacity".to_string(),
    "color".to_string(),
];
```
Since `common` is `.clone()`d into every type's property set, this covers all actor types in one place. Optionally add a type entry in `known_property_types()` mirroring `position`'s `PropertyType::Vec2`:
```rust
map.insert((ty.to_string(), "at".to_string()), PropertyType::Vec2);
```
in the same loop that registers `position` (lines ~307–313), so `at = (x, y)` type-checks against Vec2.

### Expected outcome
`animatix lint`/`check` no longer emits `Property 'at' not commonly used on ...` on any actor. `at = (…)` assignments type-check as Vec2.

### Verification
```bash
cargo check --workspace
cargo test -p animatix-analyzer
cargo run -p animatix -- lint examples/01_shapes.amx
# Grep output: no "unknown-property" with property 'at'
```

### Non-goals
- Adding other registry-backed common properties (`transform`, `stroke`, `fill_opacity`, `stroke_progress`, min/max size, typography) — track as a separate "sync analyzer property lists with PROPERTY_REGISTRY" follow-up.

---

## B1-analyzer (follow-up, sequential with L1/L2/L3) — Analyzer doesn't expand array-indexed actors in for-loops

### Root cause
`collect_stmt`'s `Stmt::ForLoop` arm (`symbol_table.rs` lines 441–459) registers the loop vars and recurses into `body`, but a body statement `Stmt::ActorDecl { label: "bars", array_index: Some(Expr::Ident("i")), .. }` registers `bars` (the bare label) — **not** `bars__0..bars__N`. Meanwhile `indexed_dotted_ident` (`parser/common.rs` lines 99–122) rewrites **target** `bars[0]` → `bars__0` at parse time. So `fade-in bars[0]` produces a target `bars__0` that the analyzer never registered → `undefined-label`. And if nothing references the bare `bars`, the analyzer reports `unused-label` on it. (`collect_refs_from_stmt` / `diagnostics.rs` lines 310–348 share this gap.)

### Files
- `crates/animatix-analyzer/src/symbol_table.rs` — `collect_stmt` `Stmt::ForLoop` arm + a new helper to expand array-indexed declarations.
- `crates/animatix-analyzer/src/diagnostics.rs` — `check_stmt` target-label check (lines ~325–335) should accept `label__N` when `label` was declared with an `array_index`.

### Precise change (sketch)
The analyzer is I/O-free and doesn't evaluate `Expr`, so it cannot know the loop count. Two options:
1. **Register the bare label AND synthesize a wildcard** that matches `label__<integer>` in the undefined-label check. Concretely: when `collect_stmt` sees `ActorDecl { array_index: Some(_), label, .. }`, insert `label` as `LabelKind::Actor` **and** record `label` in a new `array_labels: HashSet<String>` on `SymbolTable`. In `diagnostics.rs`, treat a target whose first segment matches `^(.+)__\d+$` as defined when the prefix is in `array_labels`. This avoids evaluating the iterable.
2. **Conservative:** only suppress `undefined-label`/`unused-label` for the bare `label` when it has an `array_index`, and for any `label__N` target whose prefix is a known array label. Same data, same check.

Option 1 is cleaner. Do not attempt to enumerate `bars__0..bars__N` (no iterable evaluation in the analyzer).

### Verification
```bash
cargo check --workspace
cargo test -p animatix-analyzer
cargo run -p animatix -- lint examples/15_for_loop.amx
# 'fade-in bars[0]' no longer → undefined-label 'bars__0'; bare 'bars' no longer → unused-label
```

### Non-goals
- Evaluating for-loop iterables in the analyzer (kept I/O-free and side-effect-free).
- The `always` override no-op warning (different file: `timeline/modifier_exec.rs` / `scene_eval.rs`); track separately.

---

# PART B — Design Proposals (4 language-design gaps)

Each proposal: syntax + example, semantics, fit rationale, eval-path impact (tree-walker + IR/VM), effort, risks. Effort tags: **small** / **medium** / **large**. Items requiring **both** eval paths are marked.

Conventions confirmed from source:
- Modifier runtime has **three tiers**: tree-walker (`timeline/modifier_exec.rs`), IR (`modifier_runtime/ir/lower.rs` + `eval.rs`), VM (`modifier_runtime/vm.rs`). All three already handle `Stmt::ForLoop`, `Stmt::Conditional`, `Stmt::Assignment`, `Stmt::LetDecl` in `always` bodies.
- World-space position resolution already exists: `Timeline::actor_world_affine` (`timeline/mod.rs` ~line 1026) and `Timeline::resolve_actor_world_position` (`scene_eval.rs` ~line 211), used by the `TargetResolver` trait (`timeline/callout_geometry.rs`) for `Callout` targeted mode. This is the reuse anchor for G5/G6.
- Position binding model: `at` + `anchor` + `offset` → `PositionBinding` group (`property_registry.rs` line 629, `timeline/position.rs`). Variants: `Absolute`, `SceneAnchor`, `ScenePercent`, `ContainerDefault`, `ContainerPercent`.
- The language promise (architecture.md §8): *the frame at time t is a random-access function of the source, the requested time, and the render dimensions.* `always` is stateless by design; per-actor stateful updaters were explicitly dropped (spec §10, roadmap §4.1).

---

## G3 — Data-dependent stepwise animation (without imperative state)

### Proposal: `when`/`cases` declarative phase branching (sugar over `if`/`else if`) + build-time `let` shadowing for algorithm precomputation

**Pick:** the **declarative `when`** option. It is the most idiomatic with the existing `always` + conditional model and preserves the random-access promise. Runtime `state { }` with per-frame mutable variables is **rejected** (breaks random-access; explicitly dropped per roadmap §4.1).

### Syntax + example
```amx
// Frame-time branching over a pure function of t (inside always)
always {
  let step = floor(t / 0.6)        // which phase are we in
  when step {
    0 => { bars[0].color = accent.danger }
    1 => { bars[1].color = accent.danger }
    2 => { bars[2].color = accent.danger }
    else => {}
  }
}
```
`when` accepts an integer-valued expression and a list of `literal => { stmts }` arms plus an optional `else => { stmts }`. It is **pure sugar**: desugar at parse time to nested `Stmt::Conditional` (`if step == 0 {…} else if step == 1 {…} else {…}`). No new AST storage needed if desugared; alternatively add `Stmt::When { scrutinee, arms, else_arm }` and lower it to the same IR.

For **true sequential algorithms** (sorting, where step *k* depends on step *k−1*), the complementary mechanism is **build-time precomputation** — write the algorithm in a `for` loop (which already emits keyframes/actions per iteration at build time) using `let` **shadowing** to carry state across iterations:
```amx
#0s
let arr = {5, 2, 8, 1, 9, 3}
for i in {0, 1, 2, 3, 4} {
  for j in {0, 1, 2, 3} {
    if arr[j] > arr[j+1] {
      swap bars[j], bars[j+1] [300ms]
      let arr = list_swap(arr, j, j+1)   // shadows previous arr for subsequent iterations
    }
  }
}
```
This requires (a) `let` shadowing/reassignment at build time (today `let` writes a keyframe at `time_ms`; a second `let` at the same `time_ms` overwrites — usable but undocumented) and (b) a `list_swap`/`list_set` built-in. Both are **build-time only** — no frame-time state, random-access preserved.

### Semantics
- `when` in `always`: scrutinee is evaluated per frame; the first matching literal arm's statements run; `else` runs if none match. Exactly `if`/`else if` semantics.
- `when` outside `always` (build time): scrutinee must be a build-time constant or loop variable; arms may contain **actions** (unlike `always` arms, which are assignments-only).
- Build-time `let` shadowing: a later `let x = …` in the same scope overwrites the `variable_tracks` entry at that `time_ms`; subsequent expressions in the same build pass read the new value. (This is how it already behaves; the proposal makes it documented + adds list built-ins.)

### Fit rationale
- Extends the existing `always` + `Expr::Conditional` model rather than inventing a new paradigm.
- Keeps the random-access guarantee intact (no per-frame mutable state).
- `when` is cosmetic sugar — zero semantic risk; users can already write the `if`/`else if` chain by hand.
- Build-time `let` shadowing reuses the existing `for`-loop expansion + `variable_tracks` mechanism; no new runtime.

### Eval-path impact
- **`when` (desugared):** NONE — if desugared to `Stmt::Conditional` at parse time, the tree-walker, IR (`lower.rs` has a `Conditional` arm), and VM all already handle it. If kept as a distinct `Stmt::When`, it must be added to **all three** paths (tree-walker `apply_modifier_stmt`, IR `lower_modifier_stmt`, VM compile+execute). **Recommend desugar at parse time** to avoid touching eval paths.
- **Build-time `let` shadowing:** build-time only (`process_body` `LetDecl` arm, process.rs ~line 137). No tree-walker/IR/VM change. Needs `list_swap`/`list_set` built-ins in `evaluate_expr` (expression evaluator, shared by build and frame-time) — single path.

### Effort
- **`when` sugar: small** (parser only, desugar to Conditional). **Change size: small.**
- **Build-time `let` shadowing + list built-ins: medium** (document/validate shadowing semantics, add 2–3 list built-ins to the expression evaluator, tests). **Change size: small-to-medium.**
- Combined: **medium**.

### Risks / tradeoffs
- `when` adds little power over `if`/`else if` — its value is readability for multi-step phasing. If the team prefers not to add sugar, skip it and document the `if`/`else if` pattern.
- Build-time `let` shadowing is subtly dependent on `variable_tracks` keyframe-overwrite at the same `time_ms`; needs an explicit test to pin the behavior, and a diagnostic if a user shadows across different `time_ms` (which would create two keyframes, not an overwrite).
- Does **not** solve true online/stateful algorithms (e.g. real-time sensor-driven animation); those remain out of scope per the language promise.

---

## G5 — Follow / attach primitive (track another actor's resolved position)

### Proposal: `follow` property + `PositionBinding::Follow { actor, anchor, offset }`, resolved at frame time via the existing `actor_world_affine` path

### Syntax + example
```amx
n0: Ellipse, size: (60, 60), at: (300, 360), color: accent.primary

// Pointer label tracks n0's right edge
ptr: Text, text: "n0", follow: n0, anchor: right, offset: (14, 0)

always {
  n0.at = (300 + 80 * sin(t), 360)    // n0 moves
  // ptr follows automatically — no manual coord math
}
```
Anchor vocabulary (matches `scene.*` anchors + Callout's edge model): `center`, `top`, `bottom`, `left`, `right`, `top_left`, `top_right`, `bottom_left`, `bottom_right`.

`follow` can be reassigned in `always` to retarget:
```amx
always {
  ptr.follow = if t < 2 { n0 } else { n1 }
}
```

### Semantics
- `follow: <actor>, anchor: <point>, offset: <vec2>` sets a `PositionBinding::Follow { actor, anchor, offset }` at build time.
- At frame time, `scene_eval.rs::evaluate_node_transform` resolves the binding: call `self.actor_world_affine(target, time_ms, dims)` to get the target's world affine, compute the target's local bounds (existing `actor_local_bounds`), map the requested anchor point (e.g. `right` = bounds.right midpoint) to world space, then add `offset`. That world point becomes the follower's `base_position` with `PositionBinding::Absolute` (so no further anchor/scene resolution is applied — the follower is placed directly at the computed world point).
- If the target doesn't exist at frame time, emit a one-time `tracing::warn!` and fall back to the follower's own keyframed `at`/position.
- Works for layout-managed targets: `actor_world_affine` already walks the scene graph and applies layout positions when `dynamic_layout` is on (mod.rs ~line 1044).
- `follow` in `always`: the override writes `overrides[label]["follow"] = Value::Str("n0")` (or a structured `Value::Object`/tuple); `evaluate_node_transform` reads `node_overrides.get("follow")` and, if present, resolves it as the target instead of the build-time binding.

### Fit rationale
- Reuses `actor_world_affine` + `resolve_actor_world_position` (already built for `Callout` targeted mode + scene persistence re-rooting). No new geometric machinery.
- Stays inside the existing `at`/`anchor`/`offset` → `PositionBinding` group: `follow` is a new group input that produces a new `PositionBinding` variant. The `property_registry` `PositionBindingGroup` handler already coordinates `at`+`anchor`+`offset`; `follow` becomes a fourth input that, when present, wins (like `at` wins over `anchor`).
- Reads naturally alongside `anchor: scene.top` — same mental model, just `anchor: <actor>.right` instead of `anchor: scene.right`. (An alternative syntax `at: n0.right` is possible but `at` is typed `Vec2` in the registry; a dedicated `follow` property avoids overloading `at`'s value type.)

### Eval-path impact
- **Resolution lives in `scene_eval.rs`** (which has `&self`/`&Timeline`), **not** in the modifier runtime. The tree-walker / IR / VM only write the override value (e.g. `Value::Str("n0")`) into `overrides` — which they already do for any assignment. **No new IR/VM instructions.** This is the key design decision: keep `follow` resolution out of the modifier runtime (which has no `&Timeline` access).
- Mark: does **not** require both eval paths. Only `scene_eval::evaluate_node_transform` + the `PositionBinding` group handler + parser + property registry.

### Effort
- **Medium.** New `PositionBinding` variant + `Follow` resolution in `evaluate_node_transform` (bounds→anchor-point helper, reuse callout_geometry's edge math) + `follow` property schema in `property_registry` + parser (`follow: ident, anchor: <point>, offset: <vec2>`) + `to_source` round-trip + tree-sitter grammar sync + tests. **Change size: medium.**

### Risks / tradeoffs
- **Frame-time cost:** each follower adds one `actor_world_affine` call (scene-graph walk) per frame. Bounded by scene depth; acceptable for a handful of callouts. Document a soft limit.
- **Cycles:** `a.follow: b` + `b.follow: a` would recurse. Detect at build time (graph cycle check over `follow` edges) and emit a diagnostic.
- **Bounds fidelity:** `actor_world_affine` gives the transform; anchor-point computation uses local bounds (existing `actor_local_bounds`). Precise per-shape bounds for Text/Path are a known deferred item (architecture.md §15 Callout notes) — callouts already accept this approximation; `follow` inherits it.
- **`follow` + layout-managed follower:** if the follower is itself a layout-managed child, `follow` conflicts with the container's placement. Emit `AbsolutePositionOnLayoutManagedChild`-style warning (same as `at` on layout children).

---

## G6 — Arrow / Line endpoints as actor references

### Proposal: let `from`/`to` accept actor-anchor references (`from: n0.right`), resolved at frame time via the same `actor_world_affine` + anchor-point helper as G5

### Syntax + example
```amx
n0: Rect, size: (80, 50), at: (200, 300)
n1: Rect, size: (80, 50), at: (600, 300)

// Arrow auto-tracks both endpoints; relinking = reassign the ref
link: Arrow, from: n0.right, to: n1.left, head_size: 16, stroke: accent.primary

always {
  n0.at = (200 + 60 * sin(t), 300)   // both ends track
  n1.at = (600 - 60 * sin(t), 300)
}
```
Retargeting in `always`:
```amx
always {
  link.to = if t > 3 { n2.top } else { n1.left }
}
```

### Semantics
- `from`/`to` each accept either a `Vec2` (existing) or an **actor-anchor reference** `<ident>.<anchor>` (new). The anchor vocabulary is the same as G5.
- At build time, if the value is an actor-anchor ref, store it as a new `LineEndpointRef { actor, anchor }` on `AnimationTrack` (a small side-channel, paralleling the `func_transitions` side-channel pattern in architecture.md §10 for non-interpolatable values — an actor ref cannot `Interpolate`). If it's a `Vec2`, keep the existing `line_from`/`line_to` tracks.
- At frame time, in the shape render path (`scene_eval.rs` vector-shape sampling), if a `LineEndpointRef` is present, resolve it via `actor_world_affine` + the anchor-point helper (shared with G5) to a world-space `Vec2`; otherwise sample the keyframed `line_from`/`line_to`.
- `always` override: `link.to = n1.left` writes `overrides["link"]["to"] = Value::Str("n1.left")` (or structured tuple); the shape sampler reads the override and, if it parses as an actor-anchor ref, resolves it; if it's a `Vec2`, uses it directly. This keeps `to`/`from` polymorphic at frame time.
- Interaction with `Graph` coordinate mapping: inside a `Graph`, `from`/`to` are math coords mapped to screen. Actor-anchor refs resolve to **world-space** (screen) coords, so inside a `Graph` they should be treated as already-screen-space (skip the math→screen map), documented explicitly.

### Fit rationale
- `Callout` already has `target:` + `place:` + `standoff:` for one endpoint (architecture.md §15, `callout_geometry.rs`). G6 generalizes the same idea to both endpoints of `Arrow`/`Line`, reusing the `TargetResolver`/`actor_world_affine` infrastructure.
- Keeps `from`/`to` as the property names (no new `from_target`/`to_target` verbosity) — just widens the accepted value type. This is the ergonomic win: relinking a diagram = changing one reference, no coord math.
- The side-channel pattern (architecture.md §10) is the established way to handle non-`Interpolate` endpoint data.

### Eval-path impact
- **Rendering is tree-walker-only** (shapes are rendered in `scene_eval.rs`, not run through the modifier IR/VM). The IR/VM only handles `always` override writes — which already pass through `Value` unchanged. So **no new IR/VM instructions**; the work is in the shape-sampling code path inside `scene_eval.rs`.
- Mark: does **not** require both eval paths. Same as G5 — resolution in `scene_eval`, override passthrough in all three modifier tiers.

### Effort
- **Medium.** New `LineEndpointRef` side-channel on `AnimationTrack` + frame-time resolution in the vector-shape sampler (shared anchor-point helper with G5) + parser for `<ident>.<anchor>` in `from`/`to` value position (a restricted path expression — `label_expr` already parses `name[index]`; add `name.anchor`) + `to_source` + tree-sitter grammar + tests. **Change size: medium.**
- **Implement G5 and G6 together:** they share the anchor-point-from-bounds helper and the `actor_world_affine` resolution call. Splitting them duplicates that helper. Recommended as one work stream with two deliverables.

### Risks / tradeoffs
- **`from`/`to` value polymorphism** (Vec2 vs actor-anchor ref) complicates the property's `ValueType` in the registry (`ValueType::Vec2` today). Needs either a new `ValueType::Endpoint` (accepted by `from`/`to` only) or a build-time discrimination (peek the AST: `Expr::Path([actor, anchor])` vs `Expr::Tuple`/`Expr::Vec2`). The latter avoids registry churn.
- **Zero-length arrows** when both endpoints resolve to the same point (e.g. `from: a.center, to: a.center`) — emit a build-time diagnostic.
- **Layout-managed endpoint actors:** their positions resolve per frame only with `dynamic_layout: true`; without it, the endpoint is the build-time layout position (still correct, just not animated). Document.

---

## G1/G4 — Runtime-indexed targeting (list-of-actor-refs / index by a runtime variable)

### Proposal: runtime-indexed assignment targets in `always` — `bars[i].color = red` where `i` is a per-frame `let`, resolved to `bars__{i}` at frame time

### Syntax + example
```amx
#0s
for v, i in {40, 80, 60, 30, 90} {
  bars[i]: Rect, size: (30, v), at: (i*40, 400), color: surface.secondary
}

always {
  let selected = floor(t / 0.5) % 5     // runtime index
  bars[selected].color = accent.danger  // → overrides["bars__2"]["color"] when selected==2
  bars[selected].opacity = 1.0
  // dim the rest
  for j in {0, 1, 2, 3, 4} {
    if j != selected { bars[j].opacity = 0.3 }
  }
}
```

### Semantics
- Today, assignment **targets** are a pre-split `Vec<String>` (`Stmt::Assignment { target: Vec<String>, .. }`), and `indexed_dotted_ident` only accepts **integer-literal** indices in target position (`bars[0]` → `bars__0` at parse time). A variable index `bars[i]` is rejected by the target parser.
- Proposal: extend target parsing to accept a **runtime index expression** in one segment: `bars[<expr>]` (and `bars[<expr>].prop`). At build time, if the index is an integer literal, keep the current `bars__0` rewrite (zero change). If the index is a non-literal expression, store a new target form: `TargetSegment::Indexed { base: "bars", index: Expr }` (or a new `Stmt::Assignment` variant / an `Expr::Index`-bearing target).
- At **frame time** in the modifier runtime, when applying an assignment whose target has a runtime index segment: evaluate `index` in `frame_env` → `Value::Num(n)` → construct the override key `format!("{}__{}", base, n as usize)` → write `overrides[key][property] = value`. The scene traversal then applies it to the existing `bars__N` track (created by the for-loop).
- Only valid where the expanded actor set exists (i.e. a for-loop generated `bars__0..bars__N`). If `n` is out of range or non-integer, emit a `tracing::warn!` and skip (no crash).
- **Actions** (`highlight bars[selected]`) remain **build-time only** — actions are not allowed in `always`. Runtime-indexed targeting is for **assignments in `always`** (colors, opacity, positions, etc.). This is the key scoping decision: we are not making actions runtime; we are making assignment targets runtime-indexable.

### Fit rationale
- Composes directly with the existing for-loop `name[i]` → `name__N` expansion. The for-loop creates the actor set at build time; the runtime index just selects which member to override at frame time. No new "actor set" data structure needed — the `__`-encoded tracks ARE the set.
- Stays within the `always` assignment model (no new statement kind). The only change is that the override **key** is computed at frame time instead of being a static string.
- Pairs naturally with G3's `when` (`when step { i => bars[i].color = red }`) and with for-loops inside `always` (iterate `j`, guard `j != selected`).

### Eval-path impact — **REQUIRES BOTH EVAL PATHS** ⚠️
This is the one proposal that **must touch all three modifier tiers**, because the override key computation moves from build-time (static string) to frame-time (runtime-evaluated):
1. **Tree-walker** (`modifier_exec.rs::apply_modifier_stmt` Assignment arm, lines 20–35): currently `let label = assignment_target_key(target)` (static `target.join(".")`). Must detect an indexed segment, evaluate its `index` in `frame_env`, and build `format!("{}__{}", base, n)`.
2. **IR** (`modifier_runtime/ir/lower.rs` Assignment arm, line 56): currently emits `ModifierIrStmt::Assign { target: target.clone(), ... }` with a static `target: Vec<String>`. Must emit a new variant / carry the index `Expr` compiled via `compile_modifier_expr`, and `ir/eval.rs::execute_modifier_ir` must evaluate it and construct the key.
3. **VM** (`modifier_runtime/vm.rs`): needs a new `Instruction` (or an extended `Assign` that carries a compiled index expression) + execute logic to build the `__` key at runtime.
4. **`frame_env::apply_override_incremental`**: receives the computed key string — no change needed (it already takes a `&str` label).
5. **Parser** (`parser/stmt.rs` assignment target, `parser/common.rs::indexed_dotted_ident`): accept non-literal index in one segment; produce the new target form.
6. **Analyzer** (`symbol_table.rs`, `diagnostics.rs`): recognize `bars__N` targets as defined when `bars` is an array label (shares the `array_labels` set from B1-analyzer).

Marked: **touches tree-walker + IR + VM** (all three modifier tiers) plus parser + analyzer.

### Effort
- **Medium.** The AST/parser change is small (relax `indexed_dotted_ident` to accept an expression in one segment, add a target-segment enum). The **eval-path change is the bulk**: all three modifier tiers must learn to compute the key at frame time. Plus tests in each tier. **Change size: medium** (borderline medium-large if the team wants the VM to support it without a tree-walker fallback; a pragmatic cut is to support it in the tree-walker + IR first and have the VM fall back to the IR/tree-walker path for indexed targets, since the VM is an optimization tier).

### Risks / tradeoffs
- **Override-key correctness:** the `__` convention is an internal contract between `resolve_array_index` (build) and the runtime. The runtime must format identically (`format!("{}__{}", base, n as usize)` with the same integer truncation rules) or overrides silently miss. Add a shared `fn array_actor_label(base, n) -> String` used by both build and runtime.
- **Performance:** evaluating an index expression per assignment per frame is cheap, but the VM fast-path currently assumes static keys. Profile; the fallback (tree-walker for indexed targets) is acceptable for typical scene sizes.
- **Out-of-range indices** silently no-op (the `bars__N` track doesn't exist → override unused, same as the B1 `always` no-op). This argues for the B1-analyzer follow-up + a one-time build-time warning when an `always` indexed target's base has no generated actors at all.
- **Does not enable runtime actions** (`highlight bars[i]` in `always`). That's intentional — actions are build-time by design. For "highlight bar i at step i," use build-time `for` + `sequence`/`stagger` to emit `highlight` actions at cumulative times, OR use runtime-indexed **assignments** to mimic highlighting (`bars[i].color = highlight_color`, `bars[i].opacity = 1`).

---

## Prioritization recommendation

| Gap | Value | Complexity | Touches both eval paths? | Recommendation |
|-----|-------|-----------|--------------------------|----------------|
| **G5 + G6** (together) | **High** — callouts/pointers/arrows that auto-track actors; eliminates hardcoded coord math in every diagram. | **Medium** | No (resolution in `scene_eval`; IR/VM just passthrough) | **Do first.** Highest value-to-complexity ratio; shares one anchor-point helper; reuses `actor_world_affine`/`TargetResolver` already built for Callout. Implement as one work stream. |
| **G1/G4** | **High** — enables data-driven highlighting ("highlight bar *i*") and composes with for-loops + `always`. | **Medium** (borderline med-large if VM-native) | **Yes** — tree-walker + IR + VM | **Do second.** Unlocks the class of "iterate/select at runtime" animations that for-loops alone can't express. Cut scope by shipping tree-walker + IR support first, VM fallback. Pairs with B1-analyzer's `array_labels` check. |
| **G3** (`when` sugar) | **Medium** — cleaner multi-phase branching; mostly cosmetic over `if`/`else if`. | **Small** (if desugared at parse time) | No (if desugared to `Conditional`) | **Do third (cheap).** Low-risk readability win. If desugared, zero eval-path work. |
| **G3** (build-time `let` shadowing + list built-ins) | **Medium** — enables true algorithmic precomputation (sorting, etc.) at build time. | **Small-medium** | No (build-time + expression evaluator only) | **Do alongside G1/G4.** Complements runtime indexing: `let` shadowing carries algorithm state at build time; runtime indexing selects at frame time. |

**Suggested order:** G5+G6 (one stream) → G1/G4 + G3-build-time-let (parallel: different subsystems) → G3-`when` (quick polish).

**What to explicitly NOT do:** runtime per-frame mutable `state { }` (G3 rejected option) — it breaks the random-access promise that the architecture is built around (architecture.md §8, spec §10, roadmap §4.1) and would force sequential frame evaluation, killing parallel export and random-access seek.

---

# PART B — Amendments (post-discussion refinements)

These amendments **supersede** the corresponding details in the G3 / G5 / G6 sections above. Part A (fix plans) is unchanged.

## Amendment 1 — G3 uses Rust-style `match` (not `when`/`cases`)

Replace the `when`/`cases` proposal with a Rust-style `match` construct. The language already treats `if cond { expr } else { expr }` as a value-producing expression (used in `examples/28_generation_reactive.amx`: `dots__0.color = if t > 2.5 { accent.danger } else { accent.primary }`), so `match` is a natural, more elegant generalization — one construct serving two contexts (mirroring Rust):

**As an expression** (RHS of assignment, inside `always` / value contexts) — desugars to nested `Stmt::Conditional`, zero eval-path impact:
```amx
always {
  bars[i].color = match v {
    0 => red,
    2 => blue,
    _ => white,
  }
}
```

**As a statement** (inside keyframe / `sequence` / step bodies at **build time**) — arms are blocks that *may* emit action verbs (`swap`, `pulse`, `fade-in`). This is the branching-timeline mechanism, evaluated at build time over a precomputed event list:
```amx
let events = run_sort([2,0,1,2,1,0])      // build-time precompute (needs build-time `let` + list built-ins — unchanged from original G3)
for e, k in events {
  match e {
    (0, i, j) => { swap bars[i], bars[j] [700ms] },
    (1, i)    => { pulse bars[i] [300ms] },
    (2, i, p) => { cursor.at = slot(p) [300ms] },
  }
}
```

**Pattern subset (keep .amx from becoming general-purpose):** literal patterns (Num/Str/Bool), ranges `1..=3`, or-patterns `0 | 2`, tuple patterns, required `_` arm (no exhaustiveness inference — a missing `_` is a build error), trailing comma allowed. No `@` bindings, no struct patterns.

**Scope clarity (important):**
- A `match` arm inside `always` can hold only **per-frame assignments**, not action verbs (actions are keyframe-driven, not per-frame).
- The action-emitting `match` form lives only in **build-time** blocks (top-level / keyframe / `sequence`).
- `match` alone does **not** give stepwise algorithmic animation — it still pairs with build-time `let` shadowing + `list_swap`/`list_set` built-ins (unchanged from original G3) to *produce* the event list / array state to match over. `match` is the dispatch; the precompute is the engine.

**Eval-path impact:** expression form — none if desugared to `Conditional` at parse time (all three tiers already handle `Conditional`). Statement form (build time) — build-time only (`process_body`), no tier changes. Recommend desugar at parse time. **Effort: small** for `match`; **small-medium** combined with build-time `let` + list built-ins (unchanged).

## Amendment 2 — G5/G6 unified as dot-accessible actor anchor points (drop `follow`/`anchor`)

**Supersedes** the G5 `follow: n0, anchor: right, offset: (..)` proposal. The `anchor` word collides with the existing universal `anchor` property (scene anchor: `anchor: scene.center`, spec L532/L118). Instead, make anchor points **read-only computed properties on every actor**, reusing two things .amx already has:

1. **Actor property reads via dot-access already work** — `examples/28_generation_reactive.amx` reads `badge_group._animating_rotation == 0` as a value. So `n0.right` is just a new read-only property, not new syntax.
2. **The `Callout` target-resolution machinery already exists** (spec L540–546: `target:` + `place:` + `standoff`/`to_offset`; "resolves the target's world-space bounds by composing its full ancestor transform chain, so nested actors inside scaled or rotated containers attach correctly"). `n0.right` = that same world-space edge point, exposed as a `Vec2`.

**Unified syntax (G5 + G6 collapse to one feature):**
```amx
// G6: arrows/lines track nodes by ref — relinking = reassign the ref
link0: Arrow, from: n0.right, to: n1.left, color: accent.primary, head_size: 16
link0.to = null_box.left [600ms]      // reverse the link; option-b interpolation (see below)

// G5: labels follow actors — anchor point + Vec2 arithmetic (already supported)
zero_p: Text, text: "▼ zero", at: b0.top + (0, -20)

// per-frame, in always:
always { cursor.at = bars[i].top + (0, -20) }
```

**Anchor vocabulary** — reuse the `scene.*` names verbatim (spec L803): `top_left top top_right left center right bottom_left bottom right`, plus `.anchor` (the actor's own configured anchor point). Consistent vocabulary across scene and actors.

**Semantics — frame-time-resolved, world-space, with option-b interpolation:**
- `n0.right` is re-resolved every frame from the node's *resolved* (post-layout, post-transform) bounds via the shared `resolve_anchor_point(actor, point)` helper (built on the existing Callout `actor_world_affine`/`TargetResolver` resolver). Tracks moving, swapped, scaled, and nested actors for free. Also closes the "resolved child positions not readable" half of G5: a layout-managed child's `b3.right` reads its Taffy-resolved slot, not its ignored declared `at`.
- **Option-b interpolation (decided):** assigning a *new* anchor ref **with a duration** — `link0.to = null_box.left [600ms]` — interpolates from the *previous resolved Vec2* to the *new resolved Vec2* over the duration, **sampling the target each frame** (so the endpoint slides from n1's left over to null's left — exactly what the reverse-LL animation needs). **Without a duration** — `from: n0.right` / a bare assignment — it is a **live binding**, re-resolved each frame (equivalent to an implicit `always { link0.from = n0.right }` override; lowers to the existing per-frame override machinery).
- **Build-time vs frame-time:** anchor-point refs can't be used in build-time `let` constants (layout/transforms aren't resolved at build). Valid in `always`, as live `from`/`to`/`at` bindings, and as RHS values in per-frame assignments. Add a lint rule flagging `let x = n0.right` at top level.

**No naming collisions:** universal actor properties (spec L532) are `at, position, anchor, offset, opacity, rotation, scale, transform` — none of `top/right/center/…` appear. `.center`/`.right` (computed bounding-box points) are distinct from `.at`/`.position` (the placement property); `n0.center` ≠ `n0.at`.

**Eval-path impact (honest, revised):** unlike the original G5/G6 "touches nothing" claim, this version **does** touch both eval paths — but only at the **expression-evaluator** layer: a new read-only-property case that calls the shared `resolve_anchor_point` helper. Tree-walker (`modifier_exec`/`frame_env`), IR (`lower.rs`/`eval.rs`), and VM (`vm.rs`) each add **one case** pointing at the same helper. Shallow, not deep. The option-b interpolation reuses the existing property-track interpolation (sampling the target's resolved point per frame as the interpolation endpoint). **Effort: medium** (mostly the shared resolver + per-point edge math, shared between G5 and G6 — implement as one work stream).

## Revised prioritization

| Gap | Value | Complexity | Both eval paths? | Recommendation |
|-----|-------|-----------|------------------|----------------|
| **G5 + G6 (unified dot-access anchor points)** | **High** — callouts/pointers/arrows auto-track; eliminates hardcoded coord math in every diagram. | **Medium** | Shallow — expression-eval case in all three tiers, shared helper | **Do first.** One feature, shares `resolve_anchor_point` + Callout resolver. Option-b interpolation gives smooth relinking. |
| **G1/G4** (runtime-indexed `bars[i]` in `always`) | **High** — data-driven highlighting; composes with for-loops + `always`. | **Medium** (med-large if VM-native) | **Yes — deep** (tree-walker + IR + VM compute override key at frame time) | **Do second.** Ship tree-walker + IR first, VM fallback. Pairs with B1-analyzer's `array_labels`. |
| **G3 `match`** (Rust-style, desugared) | **Medium** — elegant multi-phase branching; generalizes existing `if`-expression. | **Small** (if desugared at parse time) | No (if desugared to `Conditional`) | **Do third (cheap).** Low-risk, high-readability. |
| **G3 build-time `let` + list built-ins** | **Medium** — enables algorithmic precomputation at build time. | **Small-medium** | No (build-time + expression evaluator only) | **Alongside G1/G4.** Build-time state + runtime selection are complementary. |

**Suggested order:** G5+G6 (one stream) → G1/G4 + G3-build-time-let (parallel) → G3-`match` (quick polish).

**Still explicitly NOT doing:** runtime per-frame mutable `state { }` — breaks the random-access promise (architecture.md §8, spec §10, roadmap §4.1).
