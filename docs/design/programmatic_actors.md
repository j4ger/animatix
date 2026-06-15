# Programmatic Actor Generation Design

## 1. Problem Statement

Animatix can animate repeated structures, but it cannot declare repeated actors from data in a scalable way. Every actor label is currently a static identifier in `Stmt::ActorDecl`, and actor declarations must exist as concrete syntax before timeline build completes. The existing `for item in items { ... }` surface is useful, but its declaration use is limited by static labels: it can repeat assignments or statements, yet it cannot produce distinct actor labels such as `bar_0`, `bar_1`, or `point_17` from loop state.

This blocks data visualization and data-driven educational scenes where each datum needs normal Animatix composition, layout, actions, styles, and source-level identity.

Examples that cannot be written today:

```animatix
// 32 frequency bins should become 32 actors, but labels cannot be computed.
let magnitudes = (0.2, 0.7, 1.0, 0.55, 0.3)

#0s
row: Row, gap: 4 {
  for mag, i in magnitudes {
    bar_{i}: Rect,
      size: (12, mag * 180),
      color: if mag > 0.8 { accent.danger } else { accent.primary }
  }
}
```

```animatix
// Scatter data should become independent points with normal actor actions.
let samples = (
  Point { x: -2.0, y: 1.2, label: "A" },
  Point { x: 0.5, y: 2.0, label: "B" },
  Point { x: 1.8, y: -0.4, label: "C" }
)

#0s
plot: Graph, x_domain: (-3, 3), y_domain: (-2, 3), size: (700, 420) {
  for sample, i in samples {
    point_{i}: Ellipse,
      size: (10, 10),
      at: (sample.x, sample.y),
      color: auto
    label_{i}: Text,
      text: sample.label,
      at: (sample.x, sample.y + 0.2),
      font_size: 14
  }
}

#1s
stagger [40ms] {
  for sample, i in samples {
    fade-in point_{i} [250ms]
    fade-in label_{i} [250ms]
  }
}
```

```animatix
// A matrix heatmap made from ordinary Rect actors needs nested generation.
let cells = ((0.1, 0.4, 0.9), (0.3, 0.8, 0.2), (0.6, 0.2, 0.7))

#0s
matrix: Grid, cols: 3, gap: 2 {
  for row, y in cells {
    for value, x in row {
      cell_{y}_{x}: Rect,
        size: (32, 32),
        color: lerp_color(accent.primary, accent.danger, value)
    }
  }
}
```

A specialized `BarChart` primitive solves the FFT bar chart case, but the same gap reappears for scatter plots, custom legends, labeled points, icon grids, matrices, timeline markers, and repeated educational annotations.

## 2. Approach Comparison

| Approach | Summary | Pros | Cons |
|---|---|---|---|
| A. Specialized primitives | Add `ScatterPlot`, `Histogram`, `LineChart`, `HeatmapColumn`, etc. as self-contained primitives. | Best simple syntax for common chart types; straightforward renderer optimization; no label-generation or source-write-back complexity; good for large datasets that should not become thousands of scene nodes. | Infinite regress for every visualization variant; weak composability with `Rect`, `Text`, `Arrow`, components, slots, actions, and layout; every primitive needs custom data parsing, styling, hit-testing, inspector support, and docs. |
| B. Data-driven actor generation | Extend `for` declaration blocks to generate actors at timeline build time from evaluated list data, with computed labels. | General solution; composes with existing primitives/components/actions/layout; preserves per-actor selection and animation; reuses `Timeline::process_body` and existing property evaluation. | Requires AST label templates, parser grammar, label uniqueness diagnostics, loop scoping rules, analyzer updates, source-write-back policy, and careful distinction from frame-time `always`. |
| C. Hybrid container data sugar | Keep normal actor generation small and add `data:` sugar on containers or chart-like containers to create children. | Ergonomic for simple repeated layout; less syntax than explicit loops; can use build-time expansion internally; complements specialized primitives. | Hidden generation rules can become magical; harder to target generated actors in actions unless label policy is exposed; still needs computed labels or stable synthetic IDs; not enough for nested/custom structures by itself. |

## 3. Recommended Approach

Adopt Approach B as the core language feature, with a narrow Phase 1: build-time data-driven `for` expansion plus label templates. Keep Approach A for high-volume primitives and add Approach C later as sugar over the same expansion model.

Rationale:

1. Build-time generation matches Animatix's current timeline architecture. `Timeline::process_body` already evaluates `Stmt::ForLoop` through `for_iter_values(iterable, &self.env)` and lowers repeated statements into normal tracks. The missing pieces are computed labels, index binding, and clearer loop semantics.
2. Runtime actor creation should stay out of `always`. The reactive system is stateless per-frame and currently writes property overrides only. Allowing it to create/destroy actors would destabilize scene graph membership, layout admission, source write-back, and render caching.
3. Generated actors should be first-class timeline actors. Once expanded, `bar_12: Rect` should behave like a hand-written actor: selectable, animatable, layout-managed, morphable, and usable by actions.
4. Specialized primitives still matter. `BarChart` remains appropriate when the data volume is high or when the semantic primitive can optimize rendering better than many independent actors.
5. Container sugar is valuable but should be a later desugaring layer. It should produce the same AST/build expansion and diagnostics as explicit `for`.

Recommended semantic boundary:

| Location | Actor declarations in `for` | Assignments/actions in `for` | Iterable evaluation |
|---|---:|---:|---|
| Top level / keyframe / container | Yes | Yes | Build time |
| `sequence` / `stagger` | No declarations, matching current rule | Yes | Build time for structural action expansion |
| `always` | No declarations | Yes | Frame time |
| Component body | Yes | Yes | Build time per component instance after parameter binding |

## 4. Syntax Design

### 4.1 Build-Time `for` with Item and Index

Extend `for` to optionally bind an index variable:

```animatix
for item, i in items {
  item_{i}: Rect, size: (20, item.value * 100)
}
```

Rules:

- `item` is bound to the current element.
- `i` is bound to a zero-based numeric index.
- `for item in items` remains valid and has no index binding.
- Iterables are evaluated once during timeline build for declaration contexts.
- The iterable must evaluate to a list-like value: `Value::List`, `Value::Vec2`, `Value::Vec3`, or `Value::Vec4`; scalar fallback should be deprecated for declaration loops because accidentally generating one actor from a scalar is likely a bug.

### 4.2 Label Templates

Add computed label templates for actor declarations and action/assignment targets:

```animatix
bar_{i}: Rect, size: (12, magnitude * 180)
bar_{i}.opacity = 1 [300ms]
fade-in bar_{i} [300ms]
```

Label template grammar:

```text
label_template := label_part+
label_part     := identifier_fragment | "{" expression "}"
```

Recommended restrictions:

- At least one static alphabetic prefix is required: `bar_{i}` is valid, `{i}` is invalid.
- Template expressions must evaluate to numbers, strings, or booleans.
- Rendered label fragments are sanitized to `[A-Za-z_][A-Za-z0-9_]*` compatible segments.
- Invalid characters in computed fragments become `_`.
- Empty fragments are an error.
- Generated labels beginning with `__` are reserved and rejected.

Examples:

```animatix
bar_{i}: Rect
freq_{sample.frequency}_hz: Text, text: format("{} Hz", sample.frequency)
cell_{row_index}_{col_index}: Rect
```

### 4.3 Generated Actor References

The same template syntax is allowed anywhere a static actor target is currently accepted:

```animatix
#1s
stagger [30ms] {
  for mag, i in magnitudes {
    fade-in bar_{i} [250ms]
  }
}

#2s
for mag, i in magnitudes {
  bar_{i}.color = if mag > 0.8 { accent.danger } else { accent.primary } [200ms]
}
```

Outside the lexical `for` that binds `i`, users can reference concrete expanded labels only:

```animatix
#3s
pulse bar_12 [500ms]
```

This avoids late-bound actor lookup and keeps label resolution deterministic.

### 4.4 Object-Like Data Access

The syntax examples assume existing `Expr::Construct` values can be evaluated into object-like values with field access, or a short-term tuple/index style can be used until object field lookup is fully implemented:

```animatix
let bins = (("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3))

#0s
row: Row, gap: 8 {
  for bin, i in bins {
    bar_{i}: Rect, size: (40, bin[1] * 180)
    label_{i}: Text, text: bin[0]
  }
}
```

### 4.5 Container Sugar as Later Desugaring

After explicit build-time `for` works, add optional sugar on containers:

```animatix
bars: Row, data: magnitudes, gap: 4 {
  item_{i}: Rect, size: (12, item * 180)
}
```

Desugars to:

```animatix
bars: Row, gap: 4 {
  for item, i in magnitudes {
    item_{i}: Rect, size: (12, item * 180)
  }
}
```

This should be documented as sugar, not as a separate generation mechanism.

## 5. Implementation Plan

1. Design AST label templates in `crates/animatix-syntax/src/ast.rs` by adding `LabelExpr` and replacing static declaration labels where needed; expected outcome: AST can represent static and computed labels; verify with `cargo test -p animatix-syntax ast` or targeted parser tests.
2. Extend parser support in `crates/animatix-syntax/src/parser/mod.rs` for `for item, i in expr` and label templates in actor declarations, assignments, and action targets; expected outcome: syntax parses without changing existing examples; verify with `cargo test -p animatix parser_tests` and `cargo test -p animatix-syntax`.
3. Extend Tree-sitter support in `tree-sitter-animatix/grammar.js`, `tree-sitter-animatix/queries/highlights.scm`, and corpus tests; expected outcome: editor highlighting and `ts_convert.rs` can convert template nodes; verify with the tree-sitter corpus test command used by the repo.
4. Add build-time expansion helpers in `crates/animatix/src/timeline/build/process.rs` for loop item/index binding and label-template resolution; expected outcome: generated actor labels become ordinary timeline tracks; verify with new timeline tests in `crates/animatix/src/timeline/tests.rs`.
5. Update container inline handling in `crates/animatix/src/timeline/build/container.rs` to process `InlineItem` label templates and nested `for` blocks inside children; expected outcome: generated children participate in `Row`, `Col`, `Grid`, `Stack`, and `Group` ordering; verify with layout-focused timeline tests.
6. Add diagnostics in `crates/animatix-syntax/src/diagnostics.rs` and build validation for duplicate generated labels, invalid fragments, empty iterables where required, and declarations inside frame-time `always`; expected outcome: bad generation fails clearly; verify with parser/build diagnostic tests.
7. Update formatting and write-back in `crates/animatix-syntax/src/format_core.rs`, `crates/animatix-syntax/src/to_source.rs`, and `crates/animatix-gui/src/source_edit/*`; expected outcome: templates round-trip and GUI edits mutate generator source rather than materialized tracks; verify with round-trip and source-edit tests.
8. Update analyzer and LSP in `crates/animatix-analyzer/src/symbol_table.rs`, `crates/animatix-analyzer/src/diagnostics.rs`, and `crates/animatix-lsp`; expected outcome: loop variables are scoped and generated-label templates are understood for hover/completion diagnostics; verify with analyzer tests.
9. Document syntax in `docs/spec.md`, add examples in `examples/15_for_loop.amx` or a new `examples/21_programmatic_actors.amx`, and mention the relationship to `BarChart` in `docs/primitives.md`; expected outcome: users have a runnable pattern; verify with `cargo run -p animatix -- check examples/21_programmatic_actors.amx`.
10. Add optional container `data:` sugar after explicit `for` stabilizes by desugaring in parser or build lowering; expected outcome: no separate runtime semantics; verify by asserting sugar and explicit form build equivalent tracks.

## 6. AST Impact

Current relevant AST shape:

```rust
pub enum Stmt {
    ActorDecl {
        is_pub: bool,
        is_anonymous: bool,
        label: String,
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
        span: Option<Span>,
    },
    Assignment {
        target: Vec<String>,
        property: String,
        value: Expr,
        modifiers: Vec<Modifier>,
        easing: Option<Easing>,
        value_span: Option<ByteSpan>,
        span: Option<Span>,
    },
    ForLoop {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
        span: Option<Span>,
    },
}

pub enum InlineItem {
    Labeled { label: String, ty: String, props: Vec<Property>, ... },
    Anonymous { ty: String, props: Vec<Property>, ... },
}
```

Recommended AST changes:

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum LabelExpr {
    Static(String),
    Template(Vec<LabelPart>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LabelPart {
    Text(String),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopBinding {
    pub item: String,
    pub index: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TargetExpr {
    Static(Vec<String>),
    Template(Vec<LabelExpr>),
}

pub enum Stmt {
    ActorDecl {
        is_pub: bool,
        is_anonymous: bool,
        label: LabelExpr,
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
        span: Option<Span>,
    },
    Assignment {
        target: TargetExpr,
        property: String,
        value: Expr,
        modifiers: Vec<Modifier>,
        easing: Option<Easing>,
        value_span: Option<ByteSpan>,
        span: Option<Span>,
    },
    ForLoop {
        binding: LoopBinding,
        iterable: Expr,
        body: Vec<Stmt>,
        span: Option<Span>,
    },
}

pub enum InlineItem {
    Labeled {
        label: LabelExpr,
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
    },
    Anonymous { ty: String, props: Vec<Property>, ... },
    ForLoop {
        binding: LoopBinding,
        iterable: Expr,
        body: Vec<InlineItem>,
        span: Option<Span>,
    },
}
```

Compatibility option:

```rust
impl LabelExpr {
    pub fn as_static(&self) -> Option<&str> { ... }
    pub fn resolve(&self, env: &Environment) -> Result<String, LabelError> { ... }
}
```

This allows most existing code to keep accepting static labels initially while build-time generation calls `resolve()`.

## 7. Parser Impact

Parser files:

- `crates/animatix-syntax/src/parser/mod.rs`
- `crates/animatix-syntax/src/ts_convert.rs`
- `tree-sitter-animatix/grammar.js`
- `tree-sitter-animatix/queries/highlights.scm`
- `tree-sitter-animatix/test/corpus/control_flow.txt`
- `tree-sitter-animatix/test/corpus/statements.txt`

Chumsky grammar additions:

```rust
let label_part = choice((
    ident_fragment.map(LabelPart::Text),
    expr.clone()
        .delimited_by(just('{'), just('}'))
        .map(LabelPart::Expr),
));

let label_expr = label_part
    .repeated()
    .at_least(1)
    .collect::<Vec<_>>()
    .try_map(validate_label_template)
    .labelled("label");

let for_binding = ident
    .then(just(',').padded().ignore_then(ident).or_not())
    .map(|(item, index)| LoopBinding { item, index });
```

Actor declaration change:

```rust
let actor_decl = just("pub").or_not()
    .then(label_expr.clone())
    .then_ignore(just(':').padded())
    .then(ident.or(str_val))
    .then(props)
    .then(modifiers)
    .then(children_block)
    .map(|...| Stmt::ActorDecl { label, ... });
```

Assignment/action target change:

```rust
let target_expr = label_expr
    .separated_by(just('.'))
    .at_least(1)
    .collect::<Vec<_>>()
    .map(TargetExpr::Template);
```

Tree-sitter grammar additions:

```javascript
label_template: $ => repeat1(choice(
  $.identifier_fragment,
  seq('{', field('expr', $._expression), '}')
)),

for_block: $ => seq(
  'for',
  field('item', $.identifier),
  optional(seq(',', field('index', $.identifier))),
  'in',
  field('iterable', $._expression),
  $.block
),

actor_declaration: $ => seq(
  field('label', choice($.identifier, $.label_template)),
  ':',
  field('type', choice($.identifier, $.string)),
  optional(seq(',', $.property_list)),
  optional($.modifier_block),
  optional($.children_block)
)
```

Parser diagnostics:

- Reject `{expr}: Rect` because generated labels need a static prefix.
- Reject `bar_{i: Rect` as unterminated template.
- Reject declaration templates outside build-time expansion if they reference unknown variables.
- Reject `for item, item in items` because item and index bindings collide.

## 8. Timeline Impact

Timeline files:

- `crates/animatix/src/timeline/build/process.rs`
- `crates/animatix/src/timeline/build/actor.rs`
- `crates/animatix/src/timeline/build/container.rs`
- `crates/animatix/src/timeline/env.rs`
- `crates/animatix/src/timeline/modifier_exec.rs`
- `crates/animatix/src/timeline/modifier_runtime/ir/lower.rs`
- `crates/animatix/src/timeline/modifier_runtime/vm.rs`

Build-time expansion should happen inside `Timeline::process_body`, before `process_actor_decl`, `process_assignment_statement`, and `process_action` receive labels.

Pseudo-Rust:

```rust
fn process_body(&mut self, time_ms: f64, body: &[Stmt], parent: Option<&str>, diagnostics: &mut Vec<Diagnostic>) {
    for stmt in body {
        match stmt {
            Stmt::ForLoop { binding, iterable, body, .. } => {
                let values = eval_iterable_once(iterable, &self.env, diagnostics);
                for (index, value) in values.into_iter().enumerate() {
                    let scope = self.env.push_scope();
                    self.env.set(&binding.item, value);
                    if let Some(index_name) = &binding.index {
                        self.env.set(index_name, Value::Num(index as f64));
                    }
                    self.process_body(time_ms, body, parent, diagnostics);
                    self.env.pop_scope(scope);
                }
            }
            Stmt::ActorDecl { label, ty, props, modifiers, children, .. } => {
                let resolved_label = resolve_label(label, &self.env, diagnostics)?;
                let resolved_children = resolve_inline_generation(children, &self.env, diagnostics);
                self.process_actor_decl(&resolved_label, ty, props, modifiers, &resolved_children, time_ms, parent, diagnostics);
            }
            Stmt::Assignment { target, property, value, modifiers, easing, .. } => {
                let resolved_target = resolve_target(target, &self.env, diagnostics)?;
                self.process_assignment_statement(&resolved_target, property, value, modifiers, *easing, time_ms, diagnostics);
            }
            Stmt::Action(action, span) => {
                let resolved_action = resolve_action_targets(action, &self.env, diagnostics)?;
                process_action(&resolved_action, time_ms, self, diagnostics, *span);
            }
            Stmt::Always { body, .. } => {
                reject_actor_decls_in_always(body, diagnostics);
                self.modifiers.extend(body.clone());
            }
            _ => { /* existing cases */ }
        }
    }
}
```

Label uniqueness:

- Use the final resolved full label as the key in `Timeline::tracks` and `Timeline::nodes`.
- If two generated declarations create the same label at the same time and type, treat it as a re-declaration only if it follows existing morph/re-declaration rules outside the same expansion pass.
- If two generated declarations create the same label within one expansion pass, emit a duplicate generated label error with the template and index values.
- Nested generated children use the same global label namespace as hand-written actors, preserving current actor lookup behavior.

Layout impact:

- Generated children inside containers should be appended to `AnimationTrack.children` in expansion order.
- `Row`, `Col`, `Grid`, and `Stack` layout metadata should see generated children exactly as if they were written manually.
- Dynamic layout remains membership-static: changing the source data and rebuilding can change membership, but `always` cannot add/remove children per frame.

Reactive impact:

- `always { for item in runtime_list { actor.prop = ... } }` remains frame-time assignment expansion only.
- `always` must reject actor declarations, computed or static, because frame-time actor creation is out of scope.
- Existing `modifier_exec.rs` loop execution can keep frame-time iterable evaluation for assignments.
- IR/bytecode lowering should support `ForLoop { binding.index }` for assignments, or fall back to AST interpretation until implemented.

## 9. Source Write-Back Impact

Generated actors should not be materialized back into source by default. The GUI source of truth must remain the generator template, not the expanded actor list.

Policy:

1. Selection and inspector display: generated actors appear in the scene graph with resolved labels such as `bar_12`, but metadata records `GeneratedOrigin { template_span, loop_index_path, label_template }`.
2. Editing shared properties: if a user edits a property whose source came from the generator template, mutate the template property expression in the `for` body.
3. Editing one generated instance: if a user edits only `bar_12`, offer an explicit conversion path rather than silently editing generated source.
4. Persisting per-instance overrides: write a normal post-generation assignment after the generator block:

```animatix
#0s
row: Row, gap: 4 {
  for mag, i in magnitudes {
    bar_{i}: Rect, size: (12, mag * 180), color: accent.primary
  }
}

// GUI-generated per-instance override
bar_12.color = accent.danger
```

5. Reordering generated children: default to editing the input data order, not expanded child order. If the data expression is not editable, disable drag reorder or ask to materialize.
6. Materialization command: provide an explicit GUI command, `Materialize Generated Actors`, that replaces a `for` block with concrete actors for users who want hand-editable output.

Files impacted:

- `crates/animatix-gui/src/source_edit/apply.rs` — route edits through origin metadata.
- `crates/animatix-gui/src/source_edit/actor_edits.rs` — support generated actor edits and materialization.
- `crates/animatix-gui/src/source_edit/ast_utils.rs` — find enclosing generator block and label template.
- `crates/animatix-gui/src/app/document/active_timeline.rs` — retain generated-origin metadata from build output.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs` — display generated status and edit limitations.
- `crates/animatix-syntax/src/source_index.rs` — index template labels and generated origin spans.

Build output metadata:

```rust
pub struct GeneratedOrigin {
    pub source_span: Option<Span>,
    pub label_template: LabelExpr,
    pub loop_bindings: Vec<GeneratedLoopBinding>,
    pub iteration_indices: Vec<usize>,
}

pub struct GeneratedLoopBinding {
    pub item_name: String,
    pub index_name: Option<String>,
    pub iterable_source: Expr,
}
```

`Timeline` can expose:

```rust
pub generated_origins: BTreeMap<String, GeneratedOrigin>;
```

This keeps source write-back deterministic without storing generated syntax in the AST.

## 10. Migration Strategy

Existing `for` syntax continues to parse and build:

```animatix
always {
  for i in (0, 1, 2, 3, 4, 5) {
    let offset_y = 80.0 * sin(t * 2.0 + i * 1.047)
    if i == 0 { p0.at = (240.0, 400.0 + offset_y) }
  }
}
```

No migration is required for `examples/15_for_loop.amx`. It remains a frame-time reactive loop that assigns properties to existing actors.

A future migrated version can use build-time generation for declarations and frame-time template targets for assignments:

```animatix
config { colorscheme: "editorial-dark", resolution: (1280, 720) }

let colors = (accent.primary, accent.success, accent.warning, accent.danger, accent.primary, accent.success)

#0s
for color, i in colors {
  p_{i}: Ellipse,
    size: (50, 50),
    color: color,
    at: (240 + i * 120, 400)
}

always {
  for color, i in colors {
    let offset_y = 80.0 * sin(t * 2.0 + i * 1.047)
    p_{i}.at = (240.0 + i * 120.0, 400.0 + offset_y)
  }
}

#1s
stagger [70ms] {
  for color, i in colors {
    fade-in p_{i} [400ms]
  }
}
```

Migration stages:

1. Stage 1 keeps `Stmt::ForLoop { var, iterable, body }` source-compatible and treats missing index as `None`.
2. Stage 2 adds label templates while preserving static `String` labels through `LabelExpr::Static`.
3. Stage 3 updates examples to use generation where it improves readability.
4. Stage 4 optionally adds container `data:` sugar after explicit generation is stable.
5. Stage 5 considers warnings for declaration-time loops over scalar values and frame-time declarations in `always`.

## Roadmap Items to Add

Add these under `docs/roadmap.md` `## Planned` when implementation begins:

- **Programmatic actors phase 1: AST and parser** — Add `LabelExpr`, index-aware `ForLoop`, label-template syntax, parser tests, formatter round-trips, and Tree-sitter highlighting.
- **Programmatic actors phase 2: timeline build expansion** — Resolve build-time loop bindings, generated labels, duplicate diagnostics, generated container children, and action/assignment target templates.
- **Programmatic actors phase 3: analyzer and GUI write-back** — Track generated origins, scope loop variables in analyzer/LSP, constrain inspector edits, and add explicit materialization support.
- **Programmatic actors phase 4: docs and examples** — Document build-time vs frame-time `for`, update `15_for_loop.amx` or add `21_programmatic_actors.amx`, and explain when to prefer `BarChart` or other specialized primitives.
- **Container data sugar** — Add `data:` container sugar only after explicit programmatic actor generation has stable semantics and tests.

## Risks

- Label templates can make diagnostics harder because source spans point to a generator, not a concrete actor line.
- Duplicate labels may occur only for certain data values, so error messages must include rendered iteration context.
- Generated children can destabilize GUI reorder semantics if the source data order is not editable.
- Large datasets should not become thousands of actors by default; specialized primitives remain the better path for dense charts.
- `always` must not create actors or mutate scene graph membership, or dynamic layout and write-back become frame-dependent.
- Component expansion and generated labels can interact badly if nested component labels are rewritten before loop labels resolve; define ordering as component expansion first, generator resolution second.
- Existing analyzer symbol tables assume labels are static strings; hover/completion for generated labels will initially be approximate unless build metadata is fed back into the analyzer.
