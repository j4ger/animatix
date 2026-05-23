# Gradual Typing Implementation Plan

> Optional type annotations for component/action parameters.  
> Property values remain schema-typed as they are today.  
> See [roadmap](roadmap.md) for prioritization.

## Principles

1. **Optional everywhere** — Unannotated code works exactly as before
2. **Small lattice** — Only types that appear in parameter positions
3. **Single-pass** — No HM inference, no unification, no effect system
4. **Additive only** — No breaking changes to existing `.amx` files

---

## Type Lattice

```
Any          (unannotated — accepts anything)
Num          (f64)
Str          (String)
Bool         (bool)
Vec2         ([f64; 2])
Vec4         ([f64; 4])
Color        ([f64; 4])  — subtype of Vec4 at runtime
Actor        (actor label string — validated at build time)
Scene        (scene name string)
List<T>      (Vec<Value>)
```

**Subtyping rules:**
- `Color <: Vec4` (color values can be passed where Vec4 is expected)
- Numeric literal `<: Num`
- Actor label literal `<: Actor`
- Scene name literal `<: Scene`
- `Any` is the top type (unannotated params accept anything)

---

## Phase 0 — Syntax Foundation (~3 days)

### T0.1 AST: Add `TypeAnnotation` enum
**File:** `crates/animatix-syntax/src/ast.rs`  
**Task:** Replace `ParamDef.param_type: Option<String>` with `Option<TypeAnnotation>`.

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum TypeAnnotation {
    Num, Str, Bool, Vec2, Vec4, Color,
    Actor, Scene,
    List(Box<TypeAnnotation>),
    /// Unspecified — parse accepts anything
    Any,
}
```

**Impact:** `ParamDef` struct changes; all sites creating `ParamDef` need update.

### T0.2 Parser: Parse optional type annotations on component params
**File:** `crates/animatix-syntax/src/parser/mod.rs` (~line 950)  
**Current:**
```rust
ident.then_ignore(just(':').padded()).then(expr.map(Some).or_not())
    .map(|(name, default)| ParamDef { name, param_type: None, default })
```
**New:**
```rust
ident
.then(just(':').padded().ignore_then(type_annotation()).or_not())
.then(just('=').padded().ignore_then(expr).or_not())
.map(|((name, param_type), default)| ParamDef { name, param_type, default })
```

**Backward compatibility:** `text: "OK"` parses as `param_type: None` (Any).

### T0.3 Parser: Parse optional type annotations on action params
**File:** `crates/animatix-syntax/src/parser/mod.rs`  
**Task:** Same grammar as T0.2 but for `ComponentAction` params.  
**Note:** `ComponentAction` already has `params: Vec<ParamDef>` — just populate the type field.

### T0.4 Source emission: `to_source.rs` writes type annotations
**File:** `crates/animatix-syntax/src/to_source.rs`  
**Task:** `ParamDef::to_source()` should emit `name: Type = value` when `param_type` is `Some`.

### T0.5 Tree-sitter grammar: type annotation nodes
**File:** `tree-sitter-animatix/grammar.js`  
**Task:** Add `type_annotation` rule:
```js
type_annotation: $ => seq(':', choice('Num', 'Str', 'Bool', 'Vec2', 'Vec4', 'Color', 'Actor', 'Scene', seq('List', '<', $.type_annotation, '>')))
```
Add highlight query for type names (`@type`).

---

## Phase 1 — Type Checker Core (~5 days)

### T1.1 Define `TypeEnv` and lattice operations
**File:** `crates/animatix/src/timeline/typecheck.rs` (new)  
**Task:**
```rust
pub struct TypeEnv {
    /// Component name → param types
    component_params: HashMap<String, Vec<(String, TypeAnnotation)>>,
    /// Action name → param types (module-scoped and component-scoped)
    action_params: HashMap<String, Vec<(String, TypeAnnotation)>>,
}

impl TypeEnv {
    fn check_subtype(&self, actual: &TypeAnnotation, expected: &TypeAnnotation) -> bool;
    fn expr_type(&self, expr: &Expr) -> TypeAnnotation;
}
```

### T1.2 Component instantiation validation
**File:** `crates/animatix/src/timeline/typecheck.rs`  
**Task:** After module expansion, check each `ActorDecl` that instantiates a component:
```rust
for prop in &actor_decl.props {
    if let Some(expected_type) = env.get_component_param(&actor_decl.ty, &prop.name) {
        let actual_type = env.expr_type(&prop.value);
        if !env.check_subtype(&actual_type, expected_type) {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::TypeMismatch,
                DiagnosticPhase::Build,
                format!("Expected {expected_type} for parameter '{}', got {actual_type}", prop.name)
            ));
        }
    }
}
```

### T1.3 Action invocation validation
**File:** `crates/animatix/src/timeline/typecheck.rs`  
**Task:** Check `Stmt::Action` arguments against action parameter types.  
**Blocked on:** P2.1 (action parameters) — but we can validate component action `self` reference.

### T1.4 Add `DiagnosticCode::TypeMismatch`
**File:** `crates/animatix-syntax/src/diagnostics.rs`  
**Task:** Add new variant:
```rust
TypeMismatch,
```

---

## Phase 2 — Integration (~4 days)

### T2.1 Wire type checker into timeline build
**File:** `crates/animatix/src/timeline/mod.rs`  
**Task:** Add `typecheck_module()` call after expansion, before timeline build.  
**Pseudo:**
```rust
let expanded = module.expand()?;
typecheck::check_module(&expanded, &mut diagnostics)?;
let timeline = Timeline::build(expanded, &mut diagnostics)?;
```

### T2.2 Analyzer symbol table: add type info to `ParamInfo`
**File:** `crates/animatix-analyzer/src/symbol_table.rs`  
**Task:** `ParamInfo` gains `type_annotation: Option<TypeAnnotation>`.

### T2.3 Completer: suggest valid param values
**File:** `crates/animatix-analyzer/src/completer.rs`  
**Task:** When completing inside a component instantiation property value, filter suggestions by expected type. E.g., for `Vec2`, suggest `(0, 0)` template.

### T2.4 LSP: pipe type diagnostics
**File:** `crates/animatix-lsp/src/`  
**Task:** Type checker diagnostics already use `Diagnostic` — just ensure they flow through to LSP `publishDiagnostics`.

---

## Phase 3 — Features Unblocked (~5 days)

### T3.1 P2.1 — Action parameters
**Dependencies:** Phase 0, Phase 1  
**Task:**
- Parser: allow modifiers on action invocations to carry named args: `pulse btn [200ms, scale: 1.2]`
- Expansion: substitute action params like component params
- Type checker: validate argument types against action param types

### T3.2 P2.3 — Module-scoped actions
**Dependencies:** Phase 0, Phase 1, T3.1  
**Task:**
- Parser: accept `action Name(params) { ... }` at module scope
- Module: add `action_registry: HashMap<String, ActionDef>`
- Expansion: inline module-scoped actions at call sites (like component actions)
- Type checker: validate module-scoped action invocations

---

## Phase 4 — Strict Mode Opt-in (~2 days)

### T4.1 Config flag: `strict_types`
**File:** `crates/animatix/src/timeline/typecheck.rs`  
**Task:** Add `@config { strict_types: true }` support. When enabled:
- Unannotated component/action params emit warnings
- Type errors become hard failures (timeline build skipped)

### T4.2 Spec documentation
**File:** `docs/spec.md`  
**Task:** Add §13 "Type Annotations" covering syntax, lattice, subtyping, and `strict_types`.

---

## Effort Summary

| Phase | Tasks | Est. Days | Cumulative |
|-------|-------|-----------|------------|
| Phase 0 | Syntax | 3 | 3 |
| Phase 1 | Type checker | 5 | 8 |
| Phase 2 | Integration | 4 | 12 |
| Phase 3 | P2.1 + P2.3 | 5 | 17 |
| Phase 4 | Strict mode + docs | 2 | 19 |
| **Total** | | **~4 weeks** | |

**Parallelizable:**
- T0.4 (to_source) and T0.5 (tree-sitter) can happen alongside T0.2–T0.3
- T2.2–T2.4 (analyzer/LSP) can happen while T1.2–T1.3 (checker rules) are being written

**Risk:**
- Action parameter syntax (`pulse btn [200ms, scale: 1.2]`) may conflict with existing modifier parsing. Needs careful parser design.
- Module-scoped actions require `self` desugaring changes — `self` in a module action refers to the invocation target, not a component instance.
