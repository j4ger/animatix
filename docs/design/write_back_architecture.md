# GUI Inspector Write-Back Architecture

## Overview

The Animatix GUI inspector provides bidirectional sync between the visual preview and the `.amx` source text. When the user edits a property in the inspector or drags an actor in the preview, the change is persisted back to source.

## Architecture Evolution

### v1: Byte-Span Surgery (Deprecated)

The original approach used [`source_index.rs`](../../crates/animatix/src/source_index.rs) to map `(actor, property)` pairs to `ByteSpan` offsets in the raw source text. Edits did string surgery:

```
source[..start] + replacement + source[end..]
```

**Problems:**
- Every edit invalidated subsequent spans → required full re-parse after each edit
- Parser `.padded()` consumed trailing whitespace → `trim_span()` hack needed
- No AST round-trip → ad-hoc `serialize_property_value()` was an inverse parser
- Keyframe insertion used `format!()` string templating
- Comments/formatting preserved only by luck
- Anonymous actors were unaddressable

### v2: AST Mutation + Re-serialization (Current)

The new approach makes the AST the source of truth for write-back:

```
source_text ──parse──► AST (Vec<Stmt>)
      ▲                    │
      │                    │ GUI edits mutate AST directly
      │                    ▼
   write back         to_source::stmts_to_source()
      │                    │
      └────────────────────┘
```

**Components:**

1. **`to_source::ToSource`** (`crates/animatix/src/to_source.rs`) — Serializes every AST node back to `.amx` syntax. This is the inverse of the parser.
2. **`source_edit_v2`** (`crates/animatix-gui/src/source_edit_v2.rs`) — Semantic edit API operating on `Vec<Stmt>`:
   - `SetProperty` — updates existing property on `ActorDecl` or `Assignment`
   - `InsertProperty` — adds new property to an actor declaration
   - `InsertKeyframe` — inserts `Stmt::RelativeKeyframe` with an assignment
3. **`PropertyValue -> Expr`** (`crates/animatix-gui/src/app/workspace.rs`) — Converts inspector widget values to AST expressions.

**Edit flow:**
1. User edits widget → `PropertyEdit` created
2. In-memory `Timeline` updated for live preview
3. `source_edit_v2::apply_edit()` mutates `raw_statements`
4. `to_source::stmts_to_source()` serializes full AST
5. Editor text replaced with new source
6. `SourceIndex` rebuilt directly from mutated AST (no re-parse)

**Benefits:**
- No span invalidation → no re-parsing after edits
- No `trim_span()` hacks
- Adding properties = push to `Vec<Property>`
- Editing values = replace `Expr` node
- Keyframe insertion = push `Stmt::RelativeKeyframe` to body
- Property aliasing (`position` ↔ `at`) in one place
- Much more robust for future AST extensions

**Trade-offs:**
- Formatting is normalized (extra spaces, blank lines collapsed)
- Inline comments after properties are lost
- These are acceptable for a creative coding tool where semantic correctness matters more than hand-formatting

## Comment Grammar (Implemented)

Comments in `.amx` are restricted to two well-defined positions. Any other placement produces a parse error.

### Valid comment placements

1. **Statement-level line comments** — on their own line, anywhere a statement is expected:
   ```amx
   // Background layer
   backdrop: Rect, color: scene.background
   ```
   Parsed as `Stmt::Comment(String)`.

2. **Trailing line comments on properties** — immediately after a property value, before the comma or end of declaration:
   ```amx
   btn: Rect, size: (100, 200) // half-extents, in scene space
   ```
   Captured in `Property.trailing_comment: Option<String>` and re-emitted by `ToSource`.

### Invalid comment placements (rejected by parser)

| Placement | Example | Result |
|-----------|---------|--------|
| Block comments (`/* */`) | `/* comment */ btn: Rect` | Parse error: *"block comments (/* */) are not supported; use // line comments instead"* |
| Inside expressions | `size: (100, // width
200)` | Parse error: `//` is not valid expression syntax |
| Between arbitrary tokens | `btn : // comment
Rect` | Parse error: `//` interrupts the declaration |

### AST representation

```rust
pub struct Property {
    pub name: String,
    pub value: Expr,
    pub value_span: Option<ByteSpan>,
    /// Trailing line comment after this property value.
    /// Only `//` comments immediately following the value are captured.
    pub trailing_comment: Option<String>,
}
```

**Rationale:** Restricting comments to these two positions makes the AST round-trip deterministic. When the GUI inspector re-serializes the AST, statement-level comments stay in order and trailing property comments stay attached to their property. No silent comment loss.

**Pros:**
- Minimal parser changes: capture `//...` after `:` or `,` during property parsing
- Minimal serializer changes: emit `trailing_comment` after property value
- Preserves the most common comment pattern: `prop: value // note`

**Cons:**
- Doesn't handle comments *between* properties on the same line
- Doesn't handle block comments `/* ... */`
- Adds complexity to every AST node that might carry comments

**Implementation sketch:**

```rust
// In parser: after parsing a property value, check for trailing whitespace + //
let trailing_comment = parse_optional_inline_comment(&extra);

// In to_source:
impl ToSource for Property {
    fn to_source(&self) -> String {
        let mut s = format!("{}: {}", self.name, self.value.to_source());
        if let Some(comment) = &self.trailing_comment {
            s.push_str(&format!("  //{}", comment));
        }
        s
    }
}
```

### Strategy 2: Trivia-Inspired AST (Medium Term)

Inspired by Roslyn/Rust Analyzer, add a general-purpose trivia system:

```rust
pub struct Stmt {
    pub kind: StmtKind,
    pub leading_trivia: Vec<Trivia>,
    pub trailing_trivia: Vec<Trivia>,
}

pub enum Trivia {
    Comment(String),     // //...
    BlockComment(String), // /*...*/
    Newline,
    Whitespace(String),
}
```

**Pros:**
- General solution: handles comments anywhere
- Preserves blank lines between statements
- Preserves indentation style

**Cons:**
- Massive parser rewrite: every combinator must produce/collect trivia
- Serializer becomes trivia-aware (must emit trivia in correct order)
- All AST consumers (timeline builder, analyzer, etc.) must skip trivia
- ~3-4 weeks of work

**Verdict:** Overkill for current needs. The language is small and formatting normalization is acceptable.

### Strategy 3: Comment Attachment Map (Alternative)

Keep the AST clean but maintain a side table of comment locations:

```rust
pub struct CommentMap {
    /// Maps (statement_index, kind) → comment text
    /// kind: "before", "after", "inline"
    comments: HashMap<(usize, String), String>,
}
```

**Pros:**
- AST stays clean
- Comments are preserved independently

**Cons:**
- Requires synchronizing two data structures on every edit
- Statement indices shift during mutation → comment map must be updated
- More complex than trivia slots for the common case

**Verdict:** More complexity than benefit. Trivia slots are simpler.

### Strategy 4: Lossless Syntax Tree (Long Term)

Adopt a green-tree architecture (like `rowan`):

```
GreenNode (untyped, token-level)  →  RedNode (typed AST with trivia)
```

**Pros:**
- Full fidelity: every space, newline, comment preserved exactly
- Incremental re-parsing possible
- Industry standard (Rust Analyzer, Swift, etc.)

**Cons:**
- Massive architectural change: entire parser, serializer, and all AST consumers
- New dependency (`rowan` or custom green tree)
- 3-6 month project for a small team

**Verdict:** Not justified at current scale. The AST is well-designed and the language is small.

## Current State

- ✅ `ToSource` trait implemented for all AST nodes
- ✅ `source_edit_v2` provides semantic edit API
- ✅ GUI inspector uses AST mutation + re-serialization
- ✅ `Property.trailing_comment` captures inline `//` comments after property values
- ✅ Parser rejects block comments (`/* */`) with a diagnostic
- ✅ Comment grammar is restricted to two valid positions (statement-level and trailing property)
- ✅ 386 tests pass (16 to_source round-trips, 5 edit mutations, 365 existing)

## File Reference

| File | Purpose |
|------|---------|
| `crates/animatix/src/to_source.rs` | `ToSource` trait + `stmts_to_source()` |
| `crates/animatix/src/parser.rs` | Property trailing comment capture + block comment rejection |
| `crates/animatix/src/ast.rs` | `Property.trailing_comment` field |
| `crates/animatix-gui/src/source_edit_v2.rs` | Semantic edit API (`SetProperty`, `InsertProperty`, `InsertKeyframe`) |
| `crates/animatix-gui/src/app.rs` | `handle_property_edit` + `handle_keyframe_edit` using AST mutation |
| `crates/animatix-gui/src/app/workspace.rs` | `PropertyValue -> Expr` conversion |
