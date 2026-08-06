# Symbol-Aware Type System for Animatix

Status: the canonical `Type`/`TypeEnv` foundation, namespace path resolution,
runtime literal sharing, analyzer `PropertyType` removal, and component-internal
linter handling have landed.

## Problem

Probe 005 exposed that Animatix's static type layer is not symbol-aware.
`{red, green, blue}` is rejected as `List<Color>` because `Expr::Ident` is
typed as `Any` unless a hardcoded special case knows the name.

A shared literal table would fix named colors, but it would not make the DSL a
versatile typed language. The real gaps are deeper:

- `let` variables, action/component params, loop variables, and actor labels
  are not typed symbols to the inferencer.
- An actor named `red` cannot be distinguished from the color literal `red`.
- Component instances and their nested actor/property paths are not typed.
- Module namespaces and colorscheme namespaces are not resolved through a
  shared type environment.
- Type inference is duplicated between `animatix-syntax::typecheck::expr_type`
  and `animatix-analyzer::symbol_table::infer_expr_type`.
- List inference only looks at the first element, and only by a shallow
  syntax-level type.

## Goal

Build a canonical symbol-aware type system in `animatix-syntax`, then make
both the syntax typechecker and the analyzer consume it.

The shared literal registry becomes one small seed inside a `TypeEnv`, not the
design itself.

## Canonical Type

Add `crates/animatix-syntax/src/typing.rs` with an internal `Type` enum:

- `Num`
- `Str`
- `Bool`
- `Vec2`
- `Vec3`
- `Vec4`
- `Color`
- `Actor(String)` — actor label type
- `Component(String)` — component instance type
- `List(Box<Type>)`
- `Tuple(Vec<Type>)`
- `Function { params: Vec<Type>, ret: Box<Type> }`
- `Any`

Keep `TypeAnnotation` as the user-facing annotation syntax. Provide
`Type::from_annotation(&TypeAnnotation)` and `TypeAnnotation::from_type(&Type)`
where needed.

## TypeEnv

Add `TypeEnv` next to `Type`:

```rust
pub struct TypeEnv {
    scopes: Vec<HashMap<String, Type>>,
    actors: HashMap<String, Type>,
    components: HashMap<String, ComponentSignature>,
    namespaces: HashMap<String, NamespaceType>,
    builtins: HashMap<String, Type>,
    functions: HashMap<String, FunctionSignature>,
    property_types: PropertyTypes,
}
```

- `push_scope()` / `pop_scope()` for `always`, `sequence`, `stagger`, `if`,
  `match`, `for`, component bodies, and action bodies.
- `bind(name, type)` for `let`, params, and loop vars.
- `declare_actor(label, ty)` for actor declarations.
- `declare_component_instance(label, component_name)` for component instances.
- `register_builtin(name, type)` and `register_function(name, signature)`.
- `lookup(expr_path)` resolves:
  - local variable/param
  - actor label
  - component instance
  - builtin literal or function
  - colorscheme namespace (`accent.*`, `text.*`, `surface.*`, `stroke.*`)
  - module alias namespace
  - actor property path (`deck.bar[1].color`)

The property type lookup should be supplied through a small trait so
`animatix-syntax` does not depend on the runtime property registry:

```rust
pub trait PropertyTypes {
    fn property_type(&self, actor_type: &str, property: &str) -> Option<Type>;
}
```

## Shared Inference

Add a single recursive inferencer:

```rust
pub fn infer_expr_type(expr: &Expr, env: &TypeEnv) -> Type
```

Rules include:

- `Ident`: environment lookup first, then builtin literal, then `Any`.
- `Path`: namespace/colorscheme/module resolution, then actor property lookup.
- `Index`: `List<T>` -> `T`; `Vec2/Vec3/Vec4/Color` -> `Num`; `Str` -> `Str`.
- `Call`: function signature from `TypeEnv`.
- `Method`: method table keyed by receiver type.
- `List`: infer every element, then compute the common supertype.
- `Tuple`: infer element types; 2/3/4 numeric tuples normalize to vector types.
- `Conditional` / `Match`: common supertype across branches/arms.
- `Construct`: known constructor signature from `TypeEnv`.

## Common Supertype

Replace the first-element list heuristic with a small LUB:

- empty -> `Any`
- all identical -> that type
- all `Color` or `Vec4` -> `Vec4`
- all numeric literal types -> `Num`
- otherwise -> `Any`

This also powers `Conditional` and `Match` branch inference.

## Symbol Collection

Add a `TypeEnvBuilder` that collects a loaded AST into a `TypeEnv`:

- Two passes: register components, imports, and known types first; then walk
  statements in source order.
- Register `let` bindings with their inferred value type.
- Register actor labels, component instances, and array actor bases.
- Enter lexical scopes for `always`, `sequence`, `stagger`, `if`, `match`,
  `for`, components, and actions.
- Register loop vars from the iterable's element type.
- Register action/component params from their annotations or inferred defaults.
- Merge module namespaces from the loaded `ModuleGraph`/`LoadedProgram` when
  available.

## Refactor Targets

### `crates/animatix-syntax/src/typecheck.rs`

Replace calls to `expr_type` with `infer_expr_type(value, &self.type_env)`.
Extend `TypeEnv` here to own the symbol-aware state while checking component
props, action params, assignments, and list element types.

### `crates/animatix-analyzer/src/symbol_table.rs`

Replace `infer_expr_type` and `PropertyType` with the shared `Type` from
`animatix-syntax`. Property registry maps become `Type` maps. The analyzer
builds a `TypeEnv` from its `SymbolTable` and passes it into diagnostics,
hover, completions, and type checks.

This removes the duplicated hardcoded `accent/text/surface/stroke` rules and
the separate analyzer-only type enum.

### Runtime

Keep the runtime `Value` model unchanged. Share the named-color literal table
with `TypeEnv` builtins so static typing and evaluation stay in sync.

## Regression Tests

- `Swatches(colors: List<Color>)` accepts `{red, green, blue}`.
- `{red, rgb(1, 0, 0)}` infers `List<Color>`.
- `{red, (1, 0, 0, 1)}` infers `List<Vec4>`.
- An actor named `red` is typed as an actor, while a color literal `red` is
  typed as `Color`.
- `let x = red` propagates `Color`; `let y = x` keeps it.
- Loop vars infer from iterable element types.
- `deck.bar[1].color` infers through component instance + array actor +
  property lookup.
- `scene.*` remains `Any`.
- Probe 005 no longer reports a type mismatch.
- Probe 004's component instance/action typing continues to lint clean except
  the known component-internal `unused-label` gap.

## Phases

1. **Canonical Type + TypeEnv**: add `typing.rs`, LUB, scope stack, builtin
   seeds, and unit tests.
2. **Shared inference**: implement `infer_expr_type` and migrate
   `animatix-syntax::typecheck` to it.
3. **Analyzer migration**: replace `PropertyType`/`infer_expr_type`, build a
   `TypeEnv` from `SymbolTable`, and wire diagnostics/hover/completions.
4. **Path resolution**: add colorscheme namespaces, module namespaces, actor
   property paths, and component nested labels.
5. **Runtime literal sharing**: move named-color names to a shared constant
   used by `TypeEnv` and runtime builtins.
6. **Regression suite + dogfood**: update probes 004/005, run
   `cargo check --workspace`, `cargo test --no-fail-fast`, and
   `bash scripts/check-parser-sync.sh`.

## Risks

- `let` inference is order-sensitive: a `let` must be visible only after its
  declaration unless we add a separate type-declaration pass.
- Module/import typing needs a `LoadedProgram` or `ModuleGraph`; the analyzer
  is currently I/O-free, so it must receive resolved symbols rather than open
  files itself.
- Actor property typing needs a property-type provider in `animatix-syntax`
  or a constructor-injected table; this is the main cross-crate boundary.
- Scoped environments can get expensive for hover/completion if cloned
  naively; use immutable snapshots or cheap scope references.
- `TypeAnnotation` stays the user-facing annotation syntax; do not conflate it
  with the richer internal `Type` in the first pass.
