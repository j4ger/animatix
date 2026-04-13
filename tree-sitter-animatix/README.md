# tree-sitter-animatix

Standalone Tree-sitter grammar package for Animatix `.amx` files. This package is now the shared syntax artifact intended for external editors/tools and future GUI integration.

## Scope

This package intentionally tracks only the syntax that is currently accepted by `crates/animatix/src/parser.rs`.

- parser/tests/examples are the authoritative corpus
- runnable examples and `crates/animatix/tests/parser_tests.rs` drive the v1 surface
- removed or dead syntax is intentionally excluded

That means this grammar covers shipped syntax such as:

- comments
- strings, numbers, percentages, identifiers, dotted paths
- tuples and brace arrays
- calls, closures, and conditional expressions
- modifiers and dotted assignments
- actions
- actor declarations and inline children
- `Text`, `Math`, `Code`, `Svg`, and `Image` statements
- `always` / labeled `always`
- `if` / `else`
- `for`
- `import`
- `pub component`
- absolute keyframes (`#time`) and relative keyframes (`#+time`)

And it intentionally does **not** expose removed syntax like:

- `on appear`
- `on disappear`
- `loop`
- `yield`
- `stop`
- `pause`
- `resume`

## Workflow

From this directory:

```sh
tree-sitter generate
tree-sitter test
tree-sitter highlight ../examples/reactive_runtime.amx
```

These commands serve different purposes:

- `tree-sitter generate` verifies the grammar can still produce parser artifacts
- `tree-sitter test` verifies the corpus against the current concrete syntax tree shape
- `tree-sitter highlight ...` smoke-tests the highlight queries against a shipped example

## Synchronization rule

If the Animatix parser surface changes, update these files together:

1. `crates/animatix/src/parser.rs`
2. `crates/animatix/tests/parser_tests.rs`
3. runnable `.amx` examples
4. this grammar package

The Rust parser remains the source of truth; this package is a synchronized derivative for editor/tooling use.

## Guidance for future grammar changes

When changing grammar support, follow this order:

1. update `crates/animatix/src/parser.rs` first if the syntax itself is changing
2. add or update Rust parser tests in `crates/animatix/tests/parser_tests.rs`
3. add or update runnable `.amx` examples if the syntax is user-facing
4. add or update Tree-sitter corpus coverage under `test/corpus/`
5. run `tree-sitter generate`, `tree-sitter test`, and `tree-sitter highlight`
6. update highlight queries only after confirming the real generated node shapes

Rules to keep the package healthy:

- do not add grammar support for speculative or planned syntax the Rust parser does not accept
- do not leave removed syntax implied by highlights or README wording
- prefer corpus coverage for every new syntactic form before widening highlight rules
- treat highlight-query failures as structural drift, not as a reason to loosen the parser-authoritative boundary
