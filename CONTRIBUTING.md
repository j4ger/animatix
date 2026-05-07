# Contributing to Animatix

Animatix is still evolving quickly, so good contributions are not just about making code compile. The most useful changes keep the design, docs, implementation, tests, and demos aligned with each other.

## Contribution Order of Operations

When making a non-trivial change, follow this order:

1. **Align the design first**
2. **Update the docs to match the intended behavior**
3. **Implement the feature or fix with sane, readable code**
4. **Add or update unit tests**
5. **Add or update demos/examples**
6. **Verify the code and demos actually work**

If a change alters the user-facing language surface, docs and examples should land with the code. Do not leave the repo in a state where the implementation and documentation disagree.

## Before You Start Coding

For anything beyond a tiny local fix:

- Check whether the behavior already has a design note in `docs/`
- Check whether the current runtime already supports the intended surface
- Decide whether the change is:
  - a runtime-backed feature
  - a parser-only experiment
  - a planned design that should remain clearly marked as future work

Avoid widening the documented language surface unless the runtime actually supports it.

## Documentation Rules

Keep these files in sync when relevant:

- `Readme.md` — high-level user-facing capabilities and CLI usage
- `docs/primitives.md` — runtime-supported primitives and planned gaps
- `docs/spec.md` — language behavior and status callouts
- `examples/README.md` — curated runnable demos
- `tree-sitter-animatix/README.md` — grammar package scope and maintenance contract for parser-surface changes

If you add, remove, or substantially change a feature, update the related docs in the same change.

If you change accepted `.amx` syntax, update the grammar package in the same change unless the parser/test/example change is intentionally incomplete and still not ready for tooling exposure.

## Implementation Rules

Prefer small vertical slices over broad speculative rewrites.

Good implementation changes usually:

- match existing patterns in `crates/animatix/src/`
- close parser/runtime mismatches instead of adding more of them
- keep changes focused and easy to review
- avoid dead feature branches in the codebase

### Code quality expectations

- Keep code readable and explicit
- Prefer clear names over clever tricks
- Avoid unnecessary abstractions
- Do not suppress type or compiler issues just to get a build through
- Fix the root cause instead of layering special cases on top

### Comments

Comments should explain **why** something exists, not narrate obvious code.

Good comments:

- explain non-obvious invariants
- call out runtime/parser distinctions
- document rendering, timeline, or evaluation constraints
- explain tradeoffs that are easy to miss in review

Avoid comments that merely restate the line below them.

## Tests and Demos

Every meaningful feature change should consider both **tests** and **demos**.

### Unit tests

Add or update tests when changing:

- parser behavior
- AST structures
- timeline evaluation
- morphing/path logic
- module/import handling
- Tree-sitter grammar shape or highlight queries

Tests should be targeted and readable. Prefer focused coverage over giant snapshot-style tests that are hard to debug.

### Demos

Examples are part of the product surface. Keep them honest.

- `examples/*.amx` should be runnable on the current runtime
- If a demo depends on unimplemented syntax, it does not belong in the runnable example set

When adding a new runtime feature, try to add one focused demo that proves it is real.

### Grammar validation

If a change affects accepted `.amx` syntax or syntax highlighting support, validate the grammar package too:

```bash
cd tree-sitter-animatix
tree-sitter generate
tree-sitter test
tree-sitter highlight ../examples/reactive_runtime.amx
```

Keep Tree-sitter changes corpus-first. A new grammar rule should arrive with a corpus case, and highlight-query changes should follow the real generated node shapes rather than assumptions.

## Validation Checklist

Before opening a PR or creating a commit, validate your work.

### Build and tests

```bash
cargo build
cargo test
```

### CLI inspection and debug workflow

```bash
# Inspect parser/module output
cargo run -- ast path/to/file.amx

# Compact AST for quick review
cargo run -- ast path/to/file.amx --compact

# Preview in the renderer
cargo run -- render path/to/file.amx

# Export a single frame at a specific time
cargo run -- image path/to/file.amx --time 1.5 --output /tmp/frame.png

# Export a full video render
cargo run -- video path/to/file.amx --output /tmp/demo.mp4 --fps 30
```

Use `image --time` aggressively when validating timeline changes. It is the most practical way today to inspect a specific moment in an animation.

## Internal Debugging Utilities

The current public CLI exposes these practical debugging tools:

- `ast` for parser/module inspection
- `render` for local visual preview
- `image` for frame-level debugging
- `video` for full animation validation

Animatix does have an internal keyframed timeline system, but it does **not** currently provide a dedicated public keyframe-export CLI. If you are debugging keyframe behavior today, inspect the AST and validate the timeline through targeted frame renders.

## Commit Hygiene

Commits should be coherent and honest.

- Keep each commit focused on one logical change
- Do not mix roadmap rewrites, runtime changes, and unrelated cleanup unless they are inseparable
- Make sure docs and examples move with the implementation when the user-facing surface changes
- Do not commit broken demos, stale docs, or speculative syntax as if it were shipped

In practice, a good change usually reads like this:

1. clarify design expectations
2. align docs
3. implement the feature cleanly
4. add tests and examples
5. verify with CLI tools and renders

## If You Are Unsure

When a feature spans parser, runtime, and docs, favor the smallest truthful version first. A narrower feature that is implemented, tested, and documented is better than a broader surface that only partially works.
