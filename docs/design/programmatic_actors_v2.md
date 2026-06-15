# Programmatic Actor Generation — Array-Indexing Approach

> Revision of `programmatic_actors.md` replacing label templates (`bar_{i}`)
> with array-indexing syntax (`bars[i]`), reusing the existing `Expr::Index`
> pattern.

---

## 1. Motivation

Label templates (`bar_{i}: Rect`) introduce a new `{expr}` interpolation syntax
that feels foreign to the rest of Animatix. The language already has `Expr::Index`
for index access (`items[0]`). Reusing `[]` for array actor declarations is
consistent, familiar, and requires less new machinery.

```animatix
// Before (label templates — janky):
for mag, i in magnitudes {
  bar_{i}: Rect, size: (12, mag * 180)
}

// After (array indexing — clean):
for mag, i in magnitudes {
  bars[i]: Rect, size: (12, mag * 180)
}
```

---

## 2. Core Design

### 2.1 Declaration

`bars[i]: Type, props...` declares the i-th element of array actor `bars`.

```animatix
for item, i in items {
  bars[i]: Rect, size: (12, item * 180), color: accent.primary
  labels[i]: Text, text: item.name, font_size: 12
}
```

**Semantics:**
- `bars` is auto-created as an **array actor** — a lightweight container node in
  the scene graph.
- `bars[i]` during `for` expansion produces a concrete child with a stable
  generated label (e.g. `bars__0`, `bars__1`, ...).
- The array holds all generated children as a logical group.

### 2.2 Access Outside Loops

Generated elements are referenced as `bars[0]`, `bars[1]`:

```animatix
bars[0].color = accent.danger   // property assignment on one bar
fade-in bars[0] [300ms]         // action on one bar
pulse bars[3] [500ms]           // action on another
```

The array itself can be addressed:

```animatix
fade-in bars [500ms]            // fades all children
bars.position = (100, 200)      // moves the whole group
```

### 2.3 Mixed Generation

One datum → multiple generated actors is clean — just use separate arrays:

```animatix
for sample, i in points {
  dots[i]: Ellipse, size: (8, 8), at: (sample.x, sample.y), color: auto
  labels[i]: Text, text: sample.label, at: (sample.x, sample.y + 0.2)
}
```

### 2.4 Nested Loops (Matrix)

```animatix
for row, r in rows {
  for val, c in row {
    cells[r][c]: Rect, size: (20, 20), color: lerp(...)
  }
}
```

The inner `cells[r][c]` creates a two-level index. Internal labels:
`cells__0__0`, `cells__0__1`, `cells__1__0`, etc.

Access via `cells[0][1].color = red`.

### 2.5 Staggered Actions

```animatix
stagger [30ms] {
  for item, i in items {
    fade-in bars[i] [250ms]
  }
}
```

This works naturally — `for` inside `stagger` expands each iteration with the
stagger offset applied to each `fade-in` action.

### 2.6 Re-Declaration and Morphing

Array elements re-declared at a later keyframe morph like any other actor:

```animatix
#0s
for mag, i in zero_mags {
  bars[i]: Rect, size: (12, 0), color: accent.primary
}

#2s
for mag, i in full_mags {
  bars[i]: Rect, size: (12, mag * 180), color: accent.primary [800ms]
}
```

Each `bars[i]` at the new keyframe matches by index to the old `bars[i]` and
triggers path morphing for the rectangle height. Same-index = same actor.

### 2.7 Inside Layout Containers

```animatix
row: Row, gap: 4 {
  for mag, i in magnitudes {
    bars[i]: Rect, size: (12, mag * 180), color: accent.primary
  }
}
```

Each generated child participates in layout normally. The array actor is
transparent for layout purposes — the individual children are direct members
of the parent `Row`.

---

## 3. Syntax & Grammar

### 3.1 Parser-Level Desugaring

The array syntax `bars[i]: Type, props` is **desugared at parse time** into a
normal `ActorDecl` with a computed label name. This minimizes downstream impact.

**Rule:** When the parser sees an identifier followed by `[expr]` to the left
of `:`, it creates:

```rust
Stmt::ActorDecl {
    label: format!("{}__{}", array_name, placeholder_index),
    ty: ...,
    props: ...,
    array_meta: Some(ArrayMeta {
        name: "bars".to_string(),
        index_expr: expr,
    }),
    // ...
}
```

The `placeholder_index` is a sentinel (e.g. the literal index expression text)
used for AST formatting. During timeline build, the `for` expansion replaces
it with the actual evaluated index.

**Alternative:** Don't use a placeholder at all. Store `array_meta` separately
and compute the full label only during build expansion. The raw AST stores the
template form without a concrete label.

### 3.2 Grammar

```ebnf
actor_decl  := label_expr ":" type_expr ["," prop_list] [modifier_block] [children_block]
label_expr   := ident | ident "[" expr "]"
```

A label may be:
- `bars` — static (existing)
- `bars[i]` — indexed (new)

No other `{expr}` interpolation forms are introduced.

### 3.3 Target References

For assignment targets and action targets, `bars[0]` reuses `Expr::Index`:

```rust
// bars[0].color  →  Expr::Index(Expr::Ident("bars"), Expr::Num(0.0))
```

The assignment target path becomes:

```rust
// Current: Vec<String> → ["bars__0", "color"]
// New (if we keep Vec<String>): the build resolves Expr::Index to a concrete label

// Option: parse-time resolution
// For static index (i.e. bars[0]), resolve to "bars__0" at parse time
// For variable index (i.e. bars[i] inside for loop), resolve during for expansion
```

For action targets, `bars[0]` is similarly resolved to `"bars__0"` during
build expansion.

---

## 4. AST Changes

### 4.1 Minimal Change: `Stmt::ForLoop` Gets an Index

```rust
pub enum Stmt {
    // Existing — unchanged
    ActorDecl {
        label: String,      // For array actors, this will be "bars"
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
        span: Option<Span>,
    },

    // Updated — add optional index variable
    ForLoop {
        var: String,              // item
        index_var: Option<String>, // i  (NEW)
        iterable: Expr,           // items
        body: Vec<Stmt>,
        span: Option<Span>,
    },

    // Everything else unchanged
}
```

### 4.2 No New AST Nodes Needed

Array declarations are parsed as `ActorDecl` with an **optional metadata field**
that stores the index expression. The label field stores the array name during
parsing; during build, the `for` expansion produces concrete labels.

```rust
// Option A: store it on ActorDecl (simplest for MVP)
ActorDecl {
    label: String,
    array_index: Option<Expr>,  // Some(i) for bars[i], None for normal
    ty: String,
    props: Vec<Property>,
    modifiers: Vec<Modifier>,
    children: Vec<InlineItem>,
    span: Option<Span>,
}
```

### 4.3 `InlineItem` Also Needs Array Support

```rust
pub enum InlineItem {
    Labeled {
        label: String,
        array_index: Option<Expr>,
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
    },
    Anonymous { ty: String, props: Vec<Property>, ... },
}
```

### 4.4 Assignment/Action Targets

For **static** index references (`bars[0].color = red`), the target can be
desugared at parse time:

```
bars[0].color  →  target: ["bars__0", "color"]
```

The parser recognizes `Expr::Index(Ident("bars"), Num(0))` as a target head
and inlines the label as `"bars__0"`.

For **dynamic** index references inside `for` loops (`bars[i].color = red`),
the for expansion evaluates `i` and substitutes. The parser treats `bars[i]`
as a marker that the build-time expander resolves.

---

## 5. Timeline Build Impact

### 5.1 For-Loop Expansion

`Timeline::process_body()` gains index-aware iteration:

```rust
fn process_body(&mut self, body: &[Stmt], ...) {
    for stmt in body {
        match stmt {
            Stmt::ForLoop { var, index_var, iterable, body, .. } => {
                let values = eval_iterable(iterable, &self.env, diagnostics);
                for (idx, value) in values.into_iter().enumerate() {
                    // Bind item variable
                    self.env.set(var, value);
                    // Bind index variable if present
                    if let Some(iv) = index_var {
                        self.env.set(iv, Value::Num(idx as f64));
                    }
                    // Recursively process body
                    self.process_body(body, ...);
                }
            }
            Stmt::ActorDecl { label, array_index, ty, props, .. } => {
                let resolved_label = if let Some(index_expr) = array_index {
                    // Evaluate the index expression (should resolve to a number)
                    let idx_val = evaluate_expr(index_expr, &self.env)
                        .and_then(|v| v.as_num())
                        .unwrap_or(0.0) as usize;
                    format!("{}__{}", label, idx_val)
                } else {
                    label.clone()
                };
                // Proceed with normal actor declaration
                self.process_actor_decl(&resolved_label, ty, props, ...);
            }
            Stmt::Assignment { target, property, value, .. } => {
                // Resolve any indexed segments in the target
                let resolved_target = resolve_target(target, &self.env);
                self.process_assignment_statement(&resolved_target, ...);
            }
            Stmt::Action(action, span) => {
                // Resolve indexed targets in the action
                let resolved_action = resolve_action_targets(action, &self.env);
                self.process_action(&resolved_action, ...);
            }
            // ... existing cases
        }
    }
}
```

### 5.2 Internal Label Scheme

Generated elements use `__` as a separator. The `__` sequence is reserved —
user labels containing `__` are rejected with a diagnostic.

| Array | Index | Internal Label |
|-------|-------|---------------|
| `bars` | 0 | `bars__0` |
| `bars` | 1 | `bars__1` |
| `cells` | 0, 0 | `cells__0__0` |
| `cells` | 0, 1 | `cells__0__1` |

The `__` separator is chosen because:
- It's unlikely in user-written labels (rejected by validation)
- It's visually distinct for debugging
- It avoids ambiguity with `.` (which is path access)

### 5.3 Array Actor Node

When the first `bars[i]` is declared, the build creates a parent node `bars`
(a new `ActorKindId::Array` or similar). This node:
- Has no visual rendering of its own
- Serves as a scene graph parent for generated children
- Inherits transforms (children move/rotate/scale with the array)
- Can be targeted by actions: `fade-in bars`, `bars.position = ...`
- Is transparent for layout (children are direct members of the layout parent)

### 5.4 Label Uniqueness

- Two `bars[i]` with the same `i` in the same expansion pass → diagnostic error
- Generated labels (`bars__0`) live in the same namespace as hand-written labels
- A hand-written `bars__0` and a generated `bars__0` → collision diagnostic

---

## 6. Comparison: Array vs Label Templates

| Scenario | Array `bars[i]` | Template `bar_{i}` |
|---|---|---|
| Simple list | `bars[i]: Rect` | `bar_{i}: Rect` |
| Two actors per datum | `dots[i], labels[i]` (two arrays) | `dot_{i}, label_{i}` (two template sets) |
| Nested/matrix | `cells[r][c]` | `cell_{r}_{c}` |
| Action on all | `fade-in bars` | No ergonomic form |
| Action on one | `fade-in bars[0]` | `fade-in bar_0` |
| New syntax | Only `ident[expr]` before `:` | `{expr}` interpolation in strings |
| Reuses existing | `Expr::Index`, same as `items[0]` | Nothing reused |
| Parser complexity | Small — just one more pattern | Moderate — string interpolation |
| Mental model | "array of actors" | "string substitution" |

---

## 7. Implementation Plan

### Phase 1: AST & Parser (file list)

| File | Change |
|---|---|
| `crates/animatix-syntax/src/ast.rs` | Add `index_var: Option<String>` to `ForLoop`; add `array_index: Option<Expr>` to `ActorDecl` and `InlineItem` |
| `crates/animatix-syntax/src/parser/mod.rs` | Parse `for item, i in list`; parse `ident[expr]: Type` as indexed actor decl |
| `crates/animatix-syntax/src/parser/stmt.rs` | Actor decl grammar: accept `ident "[" expr "]" ":"` as label |
| `crates/animatix-syntax/src/ts_convert.rs` | Convert tree-sitter indexed label nodes |
| `tree-sitter-animatix/grammar.js` | Add `array_label`, `for_index_var` productions |
| `tree-sitter-animatix/queries/highlights.scm` | Add highlighting patterns |
| `tree-sitter-animatix/test/corpus/*.txt` | Add corpus tests |

### Phase 2: Timeline Build Expansion

| File | Change |
|---|---|
| `crates/animatix/src/timeline/build/process.rs` | Index-aware for-loop: bind `index_var`, evaluate iterable, iterate |
| `crates/animatix/src/timeline/build/actor.rs` | Resolve `array_index` to concrete label `name__idx` |
| `crates/animatix/src/timeline/build/container.rs` | Resolve `InlineItem` array indices |
| `crates/animatix/src/timeline/build/plot.rs` | Resolve action/assignment targets with indexed references |
| `crates/animatix/src/timeline/track.rs` | Add `ActorKindId::Array` for the parent array node |
| `crates/animatix/src/timeline/env.rs` | No change — indices use existing `Value::Num` binding |

### Phase 3: Diagnostics

| File | Change |
|---|---|
| `crates/animatix-syntax/src/diagnostics.rs` | Add `DuplicateGeneratedLabel`, `ReservedLabelPrefix`, `ArrayIndexOutOfBounds` |
| `crates/animatix/src/timeline/build/process.rs` | Emit diagnostics for duplicate generated labels, reserved `__` labels |
| `crates/animatix/src/timeline/build/actor.rs` | Validate `array_index` evaluates to a non-negative integer |

### Phase 4: Source Write-Back & GUI

| File | Change |
|---|---|
| `crates/animatix-syntax/src/to_source.rs` | Format indexed actor decls as `name[i]: Type, props`; format `for item, i in list` |
| `crates/animatix-gui/src/source_edit/apply.rs` | Route edits on `bars__0` back to `bars[i]` source |
| `crates/animatix-gui/src/source_edit/actor_edits.rs` | Support per-instance override assignments |
| `crates/animatix-gui/src/app/panels/inspector/mod.rs` | Show "generated from `for` loop" indicator |

### Phase 5: Analyzer & LSP

| File | Change |
|---|---|
| `crates/animatix-analyzer/src/symbol_table.rs` | Scope loop variables; register `bars__0` as generated symbol |
| `crates/animatix-analyzer/src/completions.rs` | Offer `bars[0]`, `bars[i]` completion |
| `crates/animatix-analyzer/src/diagnostics.rs` | Validate indexed references |

### Phase 6: Docs & Examples

| File | Change |
|---|---|
| `docs/spec.md` | Add array actor syntax section |
| `docs/primitives.md` | Add `Array` primitive note |
| `examples/21_programmatic_actors.amx` | New example: scatter plot, frequency bars, matrix demo |
| `examples/fft_explain.amx` | Update to use `for` + array syntax if applicable |
| `docs/roadmap.md` | Add/update items |

---

## 8. Migration

Existing code with `for item in items` (no index) continues to work unchanged:

```animatix
always {
  for i in (0, 1, 2, 3, 4, 5) {
    let offset_y = 80.0 * sin(t * 2.0 + i * 1.047)
    if i == 0 { p0.at = (240.0, 400.0 + offset_y) }
  }
}
```

The new `for item, i in items` form is a strict superset. No existing example
needs changes.

---

## 9. Edge Cases

| Case | Behavior |
|---|---|
| `for item, i in ()` | Empty iterable → no actors generated |
| `bars[i]` where `i` is not an integer | Diagnostic: array index must be a non-negative integer |
| `bars[-1]` | Diagnostic: array index must be ≥ 0 |
| `bars[i]` where `i` duplicates a previous value in same pass | Diagnostic: duplicate generated label `bars__N` |
| Hand-written `bars__0` conflicting with generated | Diagnostic: label `bars__0` conflicts with generated array element |
| `bars[i]: Rect` at top level (outside `for`) | Diagnostic: array index requires a bound variable |
| `bars[i]` with `i` from different loop scope | Diagnostic: unbound variable in array index |
| Nested arrays with same name | `cells[r][c]` in nested loops — the inner index uses the outer loop's scope, producing `cells__0__1` etc. |
| `fade-in bars` on array node | Fades all children (applies opacity to parent, inherited) |
| `bars[i]` inside `always` | Diagnostic: actor declarations are not allowed in always blocks |
| Very large arrays (1000+ elements) | Performance warning at build time; suggest BarChart or specialized primitive |
| `bars[computed_expr]` | Index can be any expression that evaluates to a number |
