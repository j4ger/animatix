# Migration Plan: Standardize List vs Tuple Syntax

**Goal:** Clean break — `(...)` = fixed-size tuple/vector, `{...}` = variadic list. No backwards compat.

## Target Design

| Syntax | AST Node | Usage |
|--------|----------|-------|
| `(x, y)`, `(r, g, b, a)`, `(-6, 6)` | `Expr::Tuple(Vec<Expr>)` | Fixed-size: Vec2, Vec4, domains, colors |
| `{a, b, c}`, `{1, 4, 9}` | `Expr::List(Vec<Expr>)` | Variadic: points, commands, levels, data, for-iterables, let-assignments |

## Phases

### Phase 1 — AST Foundation
- `crates/animatix-syntax/src/ast.rs` — add `Expr::List(Vec<Expr>)`, update `references_ident`
- `crates/animatix-syntax/src/walk.rs` — traverse list expressions
- `crates/animatix-syntax/src/module/rewrite.rs` — rewrite inside lists
- `crates/animatix-syntax/src/module/inline_actions.rs` — substitute params inside lists
- `crates/animatix/src/timeline/utils.rs` — hash/evaluate `Expr::List`

### Phase 2 — Parser & Tree-sitter
- `crates/animatix-syntax/src/parser/expr.rs` — `{...}` → `Expr::List`, `(...)` → `Expr::Tuple`
- `tree-sitter-animatix/grammar.js` — remove `array_expression` (square brackets), rename/keep `set_expression` as list; regenerate
- `crates/animatix-syntax/src/ts_convert.rs` — `convert_set` → `Expr::List`, `convert_tuple` → `Expr::Tuple`
- Tree-sitter corpus/highlights — update if node names change

### Phase 3 — Formatting & Source Write-Back
- `crates/animatix-syntax/src/format_core.rs` — `Expr::List` → `{...}`, `Expr::Tuple` → `(...)`
- `crates/animatix-gui/src/app/commands.rs` — GUI emits `Expr::List` for lists
- Primitives (`polygon.rs`, `path.rs`, `svg_import.rs`) — default points/commands as `Expr::List`

### Phase 4 — Types & Properties
- `crates/animatix-syntax/src/typecheck.rs` — infer 2-tuple→Vec2, 4-tuple→Vec4, list→List<T>
- `crates/animatix-analyzer/src/symbol_table.rs` — update list inference
- `crates/animatix/src/timeline/property_registry.rs` — fix `levels` from `Vec2` to list-capable type

### Phase 5 — Runtime Evaluation
- `crates/animatix/src/timeline/utils.rs` — evaluate `Expr::List` → `Value::List`
- `crates/animatix/src/timeline/modifier_runtime/ir/lower.rs` — compile `Expr::List`
- `crates/animatix/src/timeline/property_lookup.rs` — iterate `Expr::List` in for-loops

### Phase 6 — Domain-Specific Consumers
- `crates/animatix/src/timeline/value_parser.rs` — PointList/CommandList from `Expr::List` outer
- `crates/animatix/src/timeline/shapes/mod.rs` — parse points/commands from list shape
- `crates/animatix/src/timeline/build/plot.rs` — parse `levels`, `data`, `bar_colors` from lists
- `crates/animatix/src/timeline/actions/reorder.rs` — `order: {c, b, a}` as list

### Phase 7 — Examples, Docs & Tests
- `examples/*.amx` — all outer lists to `{...}` (points, commands, levels, data, bar_colors, for, let)
- `docs/spec.md` — canonical syntax documentation
- `docs/architecture.md` — update if needed
- Tests — update expected ASTs across all test files

## Key Risks
- **Parser ambiguity**: `{...}` is both list expression and construct/block syntax — resolved by expression-vs-statement context
- **Singletons**: `{42}` is a 1-element list; `(42)` is parenthesized `42`
- **Empty literals**: `{}` = empty list; `()` = empty tuple — need explicit tests
- **Runtime fallout**: removing tuple-to-list fallback will break old tests — intentional for clean break
- **Property registry drift**: `levels`, `data`, `bar_colors` are build-time special cases — must update type metadata in lockstep with parsing
