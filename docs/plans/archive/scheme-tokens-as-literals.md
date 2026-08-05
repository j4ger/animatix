# Plan: Roadmap #1 — Scheme tokens as constant literals

## Goal
Make scheme tokens (`accent.danger`, `text.muted`) and arbitrary expressions
(variables, `let` bindings, named colors) accepted wherever literal values
(colors, numbers, strings) are accepted in bar-chart properties and `PointList`
parsing. Replace silent drops with diagnostics.

## Context (verified from source)

- `build_bar_chart_paths` is a **free function** (`crates/animatix/src/timeline/build/plot.rs:1608`), NOT a `Timeline` method. Its current signature:
  ```rust
  pub(crate) fn build_bar_chart_paths(
      props: &[Property], size: [f32;2], color: [f32;4], stroke_color: [f32;4],
      stroke_width: f32, x_domain: [f64;2], y_domain: [f64;2],
      parent_size: Option<[f64;2]>, diagnostics: &mut Vec<Diagnostic>, label: &str,
  ) -> Vec<VelloPath>
  ```
  It does **not** receive an `Environment`. That is the root blocker for fixing
  items 1–5 below: the eval helpers need an env.
- Single call site: `process_plot_actor` at `plot.rs:936` (a `Timeline` method).
  `initial_eval_env` is built at `plot.rs:456` via `self.build_eval_env(...)`.
  Scheme tokens ARE present in it: `apply_colorscheme` calls
  `colorscheme.seed_environment(&mut self.env)` (`build/colorscheme.rs:10`),
  and `build_eval_env` clones `self.env` (`frame_env.rs:58-62`). So passing
  `&initial_eval_env` into `build_bar_chart_paths` is correct and sufficient.
- Eval helpers (`timeline/property_lookup.rs:81,100`):
  - `evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject) -> Option<Value>`
  - `parse_color_in_env_with_lookup_diagnostic(label, prop_name, expr, env, diagnostics, subject) -> Option<[f32;4]>`
    (returns fallback gray `[0.8,0.8,0.8,1.0]` + `UnknownColorReference` diag on failure)
- Scheme tokens parse to `Expr::Path(["accent","danger"])`; `evaluate_expr_inner`
  resolves `Expr::Path` via `env.get(parts.join("."))` (`utils.rs:490`). Named
  colors (`RED`) resolve via `Expr::Ident` → `named_color` in `resolve_color_in_env`.
- `Value` coercions (`env.rs:111+`): `as_num()`, `as_bool()`, `as_str()`. `as_bool()`
  returns `false` for non-`Bool`; `as_str()` returns `""` for non-`Str`.
- Existing example `examples/fft_explain.amx:139` uses `bar_colors: {(1,0,0,1),...}`
  (`Expr::List` of `Expr::Tuple`) and `show_axis: true` (`Expr::Bool`). The current
  `show_axis` arm only matches `Expr::Str`, so `show_axis: true` is silently dropped
  (defaults to `true` by luck; `show_axis: false` would be a real bug).
- No existing tests cover bar-chart properties (grep of `tests/` for
  `bar_colors|bar_width|show_axis|max_value` → no matches).

## Architecture decision

**Thread the environment into `build_bar_chart_paths`** (Option A — recommended).
Add one `env: &Environment` parameter and pass `&initial_eval_env` from the call
site. This is consistent with how every other prop in `process_plot_actor` already
works (all use `&initial_eval_env`), and there is exactly one call site to update.

Rejected Option B (pre-resolve props in `process_plot_actor`, pass resolved
values): more invasive, splits parsing logic across two functions, and the
function still needs `diagnostics`+`label` per-prop anyway. Not worth it.

---

## Plan

### Task 0 — Thread `Environment` into `build_bar_chart_paths`
**File:** `crates/animatix/src/timeline/build/plot.rs`
**Functions:** `build_bar_chart_paths` (sig at :1608), call site at :936

- Change signature to add `env: &Environment` after `label: &str` (or before
  `diagnostics`; pick the position matching the file's convention — the eval
  helpers take `env` before `diagnostics`, so place it before `diagnostics`).
  ```rust
  pub(crate) fn build_bar_chart_paths(
      props: &[Property], size: [f32;2], color: [f32;4], stroke_color: [f32;4],
      stroke_width: f32, x_domain: [f64;2], y_domain: [f64;2],
      parent_size: Option<[f64;2]>, env: &Environment,
      diagnostics: &mut Vec<Diagnostic>, label: &str,
  ) -> Vec<VelloPath>
  ```
- Update the single call site (:936) to pass `&initial_eval_env`.
- Add `use` for `Environment` if not already in scope (check top of file; the
  function already uses `Expr` so the ast import is present, but `Environment`
  may need `use crate::timeline::env::Environment` — verify).
- Rename `_subject` (:1638) → `subject` (will now be used).
- **Preserve:** all existing literal behavior (Task 1–5 must keep `Expr::Num`,
  `Expr::Str`, `Expr::Tuple` literals working via the same eval path, since
  `evaluate_expr` handles literals natively).
- **Verify:** `cargo check -p animatix` (0 errors). No behavior change yet —
  env is threaded but unused until Tasks 1–5 land. (May produce an unused-var
  warning on `env` until Task 1; acceptable as an intermediate commit only if
  Tasks 0+1 are committed together. Prefer to land Tasks 0–5 as one commit.)

> Dependency: Tasks 1–5 all depend on Task 0. Tasks 1–5 are mutually
> independent (different match arms) and can be done in any order, but they
  edit the same `match` block so should land in one commit to avoid churn.

---

### Task 1 — Fix `bar_colors` (:1652–1678)
**File:** `crates/animatix/src/timeline/build/plot.rs`, arm `"bar_colors"`.

**Current (broken):** only `Expr::List` → `Expr::Tuple` → `Expr::Num`. Drops
scheme tokens, named colors, variables, and mixed lists.

**Target:** preserve the `"auto"` special case; otherwise evaluate each element
as a color via `parse_color_in_env_with_lookup_diagnostic`; also accept a single
(non-list) color expression.

```rust
"bar_colors" => {
    // "auto" (Ident or Str) → leave bar_colors_auto = true
    let is_auto = match &prop.value {
        Expr::Ident(s) | Expr::Str(s) => s == "auto",
        _ => false,
    };
    if is_auto {
        // keep defaults
    } else if let Expr::List(colors) = &prop.value {
        let mut parsed = Vec::with_capacity(colors.len());
        for c in colors {
            if let Some(col) = parse_color_in_env_with_lookup_diagnostic(
                label, "bar_colors", c, env, diagnostics, &subject,
            ) {
                parsed.push(col);
            }
            // on None: helper already emitted UnknownColorReference diag; skip item
        }
        if !parsed.is_empty() {
            bar_colors = parsed;
            bar_colors_auto = false;
        }
    } else {
        // Single color expression (not a list) → uniform color for all bars
        if let Some(col) = parse_color_in_env_with_lookup_diagnostic(
            label, "bar_colors", &prop.value, env, diagnostics, &subject,
        ) {
            bar_colors = vec![col];
            bar_colors_auto = false;
        }
    }
}
```

**Preserve:**
- Existing RGBA-tuple literals `{(1,0,0,1),...}` still work (each tuple evals to
  `Value::Vec4`/`Color` via `evaluate_expr`, and `color_from_value` accepts both).
- `bar_colors_auto` stays `true` when value is `"auto"` or list is empty/all-invalid.
- Empty-result fallback (fall back to base `color` per-bar at :1835) unchanged.

**Edge cases:**
- `bar_colors: {accent.danger, (0,1,0,1)}` — mixed list: each element resolved
  independently; bad ones get a diag and are skipped.
- `bar_colors: accent.primary` (single token, no braces) — handled by the
  single-color else-branch.
- `bar_colors: nonexistent.token` — emits `UnknownColorReference`, falls back to
  `bar_colors_auto` (base color).
- If `parsed` ends up shorter than `data.len()`, the existing index guard at
  :1835 (`i < bar_colors.len()`) already falls back to base `color` for the
  overflow bars. Good — no extra work.

**Verify:**
```
cargo test -p animatix -- bar_colors
```
plus new unit test (see Testing).

---

### Task 2 — Fix `bar_width` (:1640–1644) and `gap` (:1646–1650)
**File:** `crates/animatix/src/timeline/build/plot.rs`, arms `"bar_width"` and `"gap"`.

**Current (broken):** only `Expr::Num`. Drops variables/scheme-tokens/expressions.

**Target:**
```rust
"bar_width" => {
    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
        &prop.value, env, diagnostics, &subject,
    ) {
        bar_width_val = v.as_num() as f32;
        bar_width_auto = false;
    }
    // None → helper emitted diag; keep auto default
}
"gap" => {
    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
        &prop.value, env, diagnostics, &subject,
    ) {
        gap_val = v.as_num() as f32;
        gap_auto = false;
    }
}
```

**Preserve:** `bar_width_auto`/`gap_auto` default-on behavior when prop absent
or eval fails. The downstream auto-calc math (:1740–1770) is unchanged.

**Edge cases:**
- `bar_width: scene_width * 0.1` (expression) — now works.
- `bar_width: slider_value` (`let` variable) — now works.
- `bar_width: "wide"` (string) — `as_num()` returns `0.0`; sets `bar_width_val=0`,
  `bar_width_auto=false` → 0-width bars. Prefer to guard: only accept if `v` is
  `Value::Num`. **Recommendation:** match on `Value::Num(n)` and emit
  `InvalidPropertyValue` otherwise, to avoid silent 0-width:
  ```rust
  if let Some(Value::Num(n)) = evaluate_expr_with_lookup_diagnostic(...) {
      bar_width_val = n as f32; bar_width_auto = false;
  } else if /* Some but not Num */ { emit InvalidPropertyValue }
  ```
  (The `evaluate_expr_with_lookup_diagnostic` returns `None` on eval error
  already; the "Some-but-wrong-type" case needs an explicit diag. Decide
  whether to add it — see Risks.)

**Verify:** `cargo test -p animatix -- bar_width gap`

---

### Task 3 — Fix `show_axis` (:1681–1684) and `show_labels`
**File:** `crates/animatix/src/timeline/build/plot.rs`, arm `"show_axis"`.

**Current (broken):** only `Expr::Str`. The example `show_axis: true`
(`Expr::Bool`) is silently dropped.

**Target:** evaluate and coerce to bool, accepting `Bool`, `Str("true"/"1"/"false"/"0")`, `Num`.
```rust
"show_axis" => {
    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
        &prop.value, env, diagnostics, &subject,
    ) {
        show_axis = match v {
            Value::Bool(b) => b,
            Value::Str(s) => s == "true" || s == "1",
            Value::Num(n) => n != 0.0,
            _ => {
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("BarChart '{label}' show_axis expects a boolean, got {}", /*type name*/),
                ).with_subject(&subject));
                true // keep default
            }
        };
    }
}
```
**Note on `show_labels`:** currently `show_labels` is in the skip-list (:1052)
and is NOT parsed at all in `build_bar_chart_paths` (no arm). Rendering doesn't
use it. **Out of scope for this roadmap item** — `show_labels` is a separate
missing-feature, not a "silent drop of an accepted literal". Leave it; note in
risks.

**Preserve:** default `show_axis = true` when prop absent or eval fails.

**Verify:** `cargo test -p animatix -- show_axis`

---

### Task 4 — Fix `max_value` (:1689–1694)
**File:** `crates/animatix/src/timeline/build/plot.rs`, arm `"max_value"`.

**Current (broken):** only `Expr::Num`.

**Target:**
```rust
"max_value" => {
    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
        &prop.value, env, diagnostics, &subject,
    ) {
        let n = v.as_num() as f32;
        max_value_val = n;
        if n > 0.0 { max_value_auto = false; }
    }
}
```
**Preserve:** the `> 0.0` guard that keeps auto-mode for non-positive values
(:1692–1693). Downstream use at :1775 (`if !max_value_auto { max_value_val }`)
unchanged.

**Verify:** `cargo test -p animatix -- max_value`

---

### Task 5 — Fix `PointList` in `value_parser.rs` (:85–100)
**File:** `crates/animatix/src/timeline/value_parser.rs`, arm `ValueType::PointList`.

**Current (broken):** only `Expr::List` → `Expr::Tuple` → `Expr::Num`. Returns
`None` (silent) on any non-matching point; aborts the whole list on first bad item.

**Target:** evaluate each coordinate through the env; collect what succeeds;
emit a diag per bad point instead of aborting. Accept variables/expressions that
evaluate to `Value::Vec2`, or tuples whose elements eval to `Num`.

```rust
ValueType::PointList => {
    let items = match expr {
        Expr::List(items) => items,
        // A single point as a tuple, or a variable holding a list — try eval
        _ => {
            // Allow a variable/expression that evaluates to List(Vec2)
            match evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject) {
                Some(Value::List(items)) => {
                    // items are Values; convert directly
                    let mut points = Vec::with_capacity(items.len());
                    for item in items {
                        if let Value::Vec2([x,y]) = item {
                            points.push([x as f32, y as f32]);
                        } else {
                            // emit InvalidPropertyValue, skip
                        }
                    }
                    return Some(PropertyValue::PointList(points));
                }
                _ => return None,
            }
        }
    };
    let mut points = Vec::with_capacity(items.len());
    for item in items {
        // Each item: either an Expr::Tuple of 2 exprs, or an expr evaluating to Vec2
        let pair = match item {
            Expr::Tuple(t) if t.len() == 2 => {
                let x = evaluate_expr_with_lookup_diagnostic(&t[0], env, diagnostics, subject);
                let y = evaluate_expr_with_lookup_diagnostic(&t[1], env, diagnostics, subject);
                match (x, y) {
                    (Some(Value::Num(xv)), Some(Value::Num(yv))) => [xv as f32, yv as f32],
                    _ => { /* emit InvalidPropertyValue diag for this point */ continue; }
                }
            }
            _ => {
                // Evaluate as a value expecting Vec2
                match evaluate_expr_with_lookup_diagnostic(item, env, diagnostics, subject) {
                    Some(Value::Vec2([x,y])) => [x as f32, y as f32],
                    _ => { /* emit diag */ continue; }
                }
            }
        };
        points.push(pair);
    }
    if points.is_empty() { None } else { Some(PropertyValue::PointList(points)) }
}
```

**Preserve:** existing `{(x,y), (x,y)}` literal syntax still works (tuples of
`Expr::Num` eval to `Num` trivially). Empty list → `None` (same as before).

**Edge cases:**
- `{p1, p2}` where `p1`/`p2` are `let`-bound `Vec2` variables → now works via
  the per-item eval branch.
- A single bad point no longer kills the whole list — skip + diag.
- Scheme tokens as coordinates: `accent.danger` evals to `Value::Color`, not
  `Vec2`/`Num` → diag + skip. Correct (a color is not a valid coordinate).

**Verify:** `cargo test -p animatix -- pointlist` and existing polygon tests:
`cargo test -p animatix -- polygon`

> **Note:** Task 5 is independent of Tasks 0–4 (different file, no env-threading
> needed — `value_parser::parse_value` already receives `env`). Can be done in
> parallel / separate commit.

---

## Files to touch
- `crates/animatix/src/timeline/build/plot.rs` — Task 0 (sig + call site), Tasks 1–4 (match arms in `build_bar_chart_paths`).
- `crates/animatix/src/timeline/value_parser.rs` — Task 5 (`PointList` arm).
- `crates/animatix/src/timeline/tests/` — new test module/file for bar-chart props (no existing tests to update).
- `docs/roadmap.md` — remove item #1 once landed and tests green (per AGENTS.md rule 4).

## Dependencies / ordering
- **Task 0 is a hard prerequisite for Tasks 1–4** (they need `env` in scope).
- Tasks 1–4 edit adjacent arms of the same `match` in `build_bar_chart_paths` →
  commit together with Task 0 to avoid a transient unused-`env` warning.
- **Task 5 is fully independent** (different file, `env` already available) →
  can be a separate commit in parallel.
- Suggested commits:
  1. `fix(parser): accept expressions in PointList` (Task 5) — independent.
  2. `fix(renderer): evaluate bar-chart props through env` (Tasks 0–4) — one commit.
  3. `docs: remove completed roadmap #1` after tests pass.

## Risks
1. **`as_num()` silent zero for non-numeric values** (Tasks 2, 4): `Value::Str("wide").as_num() == 0.0`. If we naively call `as_num()`, a string `bar_width` becomes 0-width bars with no diag. **Mitigation:** guard on `Value::Num` and emit `InvalidPropertyValue` for other types. This is a design choice — confirm desired strictness. The existing `color`/`stroke_width` arms in the same file (:540, :574) use `v.as_num()` without type-guarding, so there's precedent for the lenient style. Recommend matching the existing lenient style for consistency, OR adding guards everywhere as a follow-up. **Decision needed.**
2. **`show_axis` type coercion** (Task 3): accepting `Num` as bool is a judgment call. The spec doesn't mention it. Keep it minimal: `Bool` + `Str("true"/"1")`. Avoid `Num` to reduce surprise.
3. **`bar_colors` single-color branch** (Task 1): accepting `bar_colors: accent.primary` (no braces) is new behavior. The registry declares `bar_colors` as `ValueType::String` (`property_registry.rs:613`) with default `"auto"` — this is inconsistent with the list semantics actually used. The registry type is likely wrong but changing it is out of scope (it's `ActorField::NoStorage` / build-time-only, so the registry type is advisory here). Just make the builder accept lists + single colors + `"auto"`.
4. **`build_bar_chart_paths` is build-time only** (no per-frame re-eval for static bar charts). Scheme tokens are constant colors, so build-time eval is correct. But if a bar chart is ever made procedural (references `t`), the current architecture rebuilds paths per frame elsewhere — confirm bar charts are NOT in the procedural-plot path. Grep shows `is_bar_chart` branch (:918) sets `vello_paths` directly and does NOT set `procedural_plot`, so bar charts are static-only. Safe.
5. **`show_labels` is entirely unimplemented** in the builder — not a "silent drop of a literal" but a missing feature. Out of scope; do not fix here.
6. **No existing tests** for bar-chart props — regressions in literal behavior won't be caught unless we add tests. The `fft_explain.amx` example exercises `bar_colors` + `show_axis` literals; add it (or a minimal snippet) to the test suite.

## Testing strategy

### New tests (bar-chart props) — add to `crates/animatix/src/timeline/tests/` (new file `bar_chart.rs` or extend an existing test module; register in `tests/mod.rs` if a new module).
Construct a minimal `Timeline` with a seeded colorscheme (`accent.danger = [1,0,0,1]`, `text.muted = [0.5,0.5,0.5,1]`) and a `BarChart` actor. Use the existing test helpers in `tests/mod.rs` (the `time: crate::ast::Time::Seconds(0.0)` + `InlineItem` builders seen in other test files). Cases:

1. **`bar_colors` with scheme tokens:** `bar_colors: {accent.danger, text.muted}` → assert rendered paths use those two colors (inspect `vello_paths[i].fill`).
2. **`bar_colors` with mixed list:** `bar_colors: {accent.danger, (0,1,0,1)}` → 2 distinct colors.
3. **`bar_colors: "auto"`** → falls back to base `color` for all bars (assert `bar_colors_auto` equivalent: all fills == base color).
4. **`bar_colors: accent.primary`** (single, no braces) → all bars same color.
5. **`bar_colors: nonexistent.token`** → diagnostic emitted with `UnknownColorReference`, fallback to base color.
6. **`bar_width` / `gap` with a `let` variable:** `let w = 30; bar_width: w` → assert bar geometry width.
7. **`show_axis: true` (Bool)** → axis path present. **`show_axis: false`** → axis path absent. (This is the regression test for the current silent-drop bug.)
8. **`show_axis: "false"` (Str)** → axis absent (preserve old behavior).
9. **`max_value` with expression:** `let m = 100; max_value: m` → scaling uses 100.
10. **Regression:** existing `fft_explain.amx` literal `bar_colors` + `show_axis: true` still renders identically (golden/snapshot or assert path count + first fill).

### New tests (PointList) — extend `tests/` polygon/shape tests.
11. **`points` with `let` Vec2 variable:** `let p = (1,2); Polygon { points: {p, (3,4)} }` → 2 points.
12. **`points` with one bad entry:** `{(1,2), "bad", (5,6)}` → 2 valid points + 1 `InvalidPropertyValue` diagnostic (does not abort the list).
13. **Regression:** existing literal `points: {(0,0),(1,1)}` unchanged.

### Existing tests to keep green
- `cargo test -p animatix` (full suite) — especially `tests/colorscheme.rs`, `tests/build.rs`, `tests/build_diagnostics.rs`.
- `cargo test -p animatix-gui` (per AGENTS.md, run when relevant — likely not touched here).
- The `fft_explain.amx` example: render once manually / via an example-based test if one exists.

### Pre-commit gates (AGENTS.md rules 2–3)
```
cargo check                                    # 0 errors
cargo test -p animatix --no-fail-fast          # all passing
cargo test -p animatix-gui --no-fail-fast      # if GUI unaffected, still run
```
Then `cog commit fix "evaluate bar-chart props through env" renderer` (scope from `cog.toml`: `renderer` covers `timeline/build/plot.rs`; if `parser` is more apt for the value_parser change, split commits).

## Open questions (ask before implementing)
1. **Strictness for numeric props** (Risk 1): guard on `Value::Num` + emit `InvalidPropertyValue` for other types, or match the existing lenient `as_num()` style used by `color`/`stroke_width` arms? Recommend strict (guard + diag) since the whole point of this roadmap item is "no silent drops".
2. **`show_axis` Num-as-bool** (Risk 2): accept `Num` or not? Recommend NOT (Bool + Str only).
3. **Should `bar_colors: <single color>` (no braces) be supported?** The registry says `ValueType::String`/default `"auto"`, implying string semantics, but the builder uses lists. Recommend supporting list + single-color + `"auto"` and filing a follow-up to fix the registry type mismatch. Confirm with maintainer.
