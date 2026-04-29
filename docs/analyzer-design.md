# animatix-analyzer: Shared Language Intelligence

## Problem

The GUI editor needs completion, diagnostics, hover, and go-to-definition. External editors (VS Code, Neovim) need the same via LSP. Without a shared core, we'd duplicate logic or force the GUI through LSP overhead.

## Solution

Extract language intelligence into `animatix-analyzer` — a pure computation crate with no I/O, no GUI, no LSP. Both the GUI and LSP server consume it as a library.

## Architecture

```
crates/
  animatix/              # parser, AST, timeline (existing)
  animatix-analyzer/     # NEW: shared language intelligence
  animatix-lsp/          # NEW: LSP server binary
  animatix-gui/          # egui app (existing, modified)
```

```
                    ┌──────────────────────┐
                    │    animatix (core)   │
                    │  parser, AST, module │
                    └──────────┬───────────┘
                               │
                    ┌──────────▼───────────┐
                    │  animatix-analyzer   │
                    │  SymbolTable         │
                    │  Completer           │
                    │  DiagnosticsProvider │
                    │  HoverProvider       │
                    │  DefinitionFinder    │
                    └────┬──────────┬──────┘
                         │          │
              ┌──────────▼──┐  ┌───▼──────────┐
              │ animatix-gui│  │ animatix-lsp │
              │ direct calls│  │ JSON-RPC     │
              │ egui popups │  │ tower-lsp    │
              └─────────────┘  └──────────────┘
```

## Design Principles

1. **No I/O in analyzer** — all functions take `&str` or `&[Stmt]`, return data
2. **Position-based API** — `(line, col)` inputs, matching LSP's `Position` type
3. **Incremental** — `Analyzer::update()` re-parses only when source changes
4. **Testable** — every function is a pure-ish computation, easy to unit test
5. **Type bridge** — analyzer uses its own types; LSP crate has `From` impls

## Crate: `animatix-analyzer`

### Public API

```rust
pub struct Analyzer {
    source: String,
    ast: Option<Vec<Stmt>>,       // None if parse failed
    parse_errors: Vec<ParseError>,
    tree: Option<Tree>,           // tree-sitter tree
    symbols: SymbolTable,
}

pub struct SymbolTable {
    /// All labels defined in the file (actor labels, component names)
    pub labels: HashMap<String, LabelInfo>,
    /// Built-in types: Text, Math, Circle, etc.
    pub types: HashSet<String>,
    /// Components defined in this file
    pub components: HashMap<String, ComponentInfo>,
    /// Properties available per type: "Text" → ["content", "position", ...]
    pub properties: HashMap<String, Vec<String>>,
    /// Keywords and built-in actions
    pub keywords: HashSet<String>,
    pub actions: HashSet<String>,
}

pub struct LabelInfo {
    pub name: String,
    pub kind: LabelKind,       // Actor, Component, Let, For
    pub line: usize,
    pub col: usize,
    pub ty: Option<String>,    // "Text", "Circle", etc.
}

pub struct ComponentInfo {
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub line: usize,
    pub col: usize,
}

pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

pub enum CompletionKind {
    Keyword,
    Type,
    Property,
    Label,
    Action,
    Value,
    Snippet,
}

pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub message: String,
    pub code: Option<String>,
}

pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

pub struct HoverInfo {
    pub contents: String,      // markdown
    pub range: Option<(usize, usize, usize, usize)>,
}

pub struct Location {
    pub file: Option<String>,  // None = same file
    pub line: usize,
    pub col: usize,
}
```

### Core Methods

```rust
impl Analyzer {
    /// Create from source text. Parses and builds symbol table.
    pub fn new(source: &str) -> Self;

    /// Update source text. Re-parses if changed.
    pub fn update(&mut self, source: &str);

    /// Completions at cursor position.
    pub fn completions_at(&self, line: usize, col: usize) -> Vec<CompletionItem>;

    /// All diagnostics (parse errors + semantic checks).
    pub fn diagnostics(&self) -> Vec<Diagnostic>;

    /// Hover information at position.
    pub fn hover_at(&self, line: usize, col: usize) -> Option<HoverInfo>;

    /// Go-to-definition at position.
    pub fn definition_at(&self, line: usize, col: usize) -> Option<Location>;

    /// Document symbols (outline view).
    pub fn document_symbols(&self) -> Vec<DocumentSymbol>;
}
```

### Completion Logic

The completer uses tree-sitter's `LookaheadIterator` for syntax-aware suggestions and the `SymbolTable` for semantic suggestions.

```
Cursor context → Completion strategy:

1. Top-level (no node context)
   → keywords (let, import, if, for, ...)
   → labels from SymbolTable
   → types (Text, Math, Circle, ...)

2. After ":" in "label: " (type_identifier position)
   → types only

3. Inside property block { ... }
   → properties for the parent actor's type
   → values: numbers, strings, booleans, tuples

4. After action verb (fade-in, move, ...)
   → labels (actor names)

5. After "." in path expression
   → properties of the left-hand type

6. Inside modifier list [ ... ]
   → modifier names (delay, easing, ...)
   → duration values
```

### Diagnostics

```
Source → Diagnostic pipeline:

1. tree-sitter ERROR/MISSING nodes
   → "Syntax error" at node range

2. chumsky parse errors (if tree-sitter also fails)
   → More detailed error messages

3. Semantic checks (future phase):
   → Undefined label references
   → Unknown property for type
   → Type mismatches in expressions
   → Duplicate label definitions
```

## Crate: `animatix-lsp`

Thin wrapper around `animatix-analyzer` using `tower-lsp`.

```rust
struct Backend {
    analyzer: Mutex<Analyzer>,
}

// ~200 lines total
// Each LSP method delegates to analyzer:
//   completion()      → analyzer.completions_at()
//   hover()           → analyzer.hover_at()
//   definition()      → analyzer.definition_at()
//   document_symbol() → analyzer.document_symbols()
//   diagnostics       → published on change via analyzer.diagnostics()
```

### Type Conversion Layer

```rust
// animatix-lsp/src/convert.rs (~150 lines)
// From<analyzer::CompletionItem> for lsp_types::CompletionItem
// From<analyzer::Diagnostic> for lsp_types::Diagnostic
// From<analyzer::HoverInfo> for lsp_types::Hover
// etc.
```

## Crate: `animatix-gui` (modifications)

### New: `completion.rs`

```rust
pub struct CompletionPopup {
    analyzer: Analyzer,
    items: Vec<CompletionItem>,
    visible: bool,
    selected: usize,
}

impl CompletionPopup {
    /// Trigger completion at cursor position.
    pub fn trigger(&mut self, source: &str, line: usize, col: usize);

    /// Render popup below cursor using egui::Area.
    pub fn show(&mut self, ui: &mut egui::Ui, cursor_rect: egui::Rect) -> Option<String>;

    /// Handle keyboard navigation.
    pub fn handle_input(&mut self, ctx: &egui::Context) -> bool;
}
```

### Modified: `editor.rs`

```rust
// In EditorBuffer::show():
// 1. On text change, update analyzer
// 2. On cursor move, trigger completion (debounced)
// 3. Show CompletionPopup if visible
// 4. Show diagnostic squiggles via LayoutJob background colors
```

### Modified: `highlighting.rs`

```rust
// Add diagnostic squiggles to LayoutJob:
// For each diagnostic, set background color on the affected range
// Error: red tint, Warning: yellow tint
```

## Implementation Phases

### Phase 1: Foundation (analyzer crate + symbol table)

**Goal**: Create `animatix-analyzer` with `SymbolTable` extraction.

- [ ] Create `crates/animatix-analyzer/` crate
- [ ] Make `parser::parse_program()` public in `animatix`
- [ ] Implement `SymbolTable::build_from_ast(&[Stmt])`
- [ ] Implement `Analyzer::new()` and `Analyzer::update()`
- [ ] Unit tests for symbol extraction

**Deliverable**: `Analyzer` that parses source and extracts labels, types, components, properties.

### Phase 2: Completion

**Goal**: Context-aware completions.

- [ ] Implement `completions_at()` with context detection
- [ ] Add tree-sitter `LookaheadIterator` for syntax completions
- [ ] Add semantic completions from `SymbolTable`
- [ ] Unit tests for each completion context

**Deliverable**: `analyzer.completions_at(line, col)` returns relevant items.

### Phase 3: Diagnostics

**Goal**: Parse error diagnostics.

- [ ] Implement `diagnostics()` from tree-sitter ERROR nodes
- [ ] Add chumsky error conversion
- [ ] Add semantic checks (undefined labels, unknown properties)
- [ ] Unit tests

**Deliverable**: `analyzer.diagnostics()` returns all errors/warnings.

### Phase 4: GUI Integration

**Goal**: Completion popup and diagnostic squiggles in the editor.

- [ ] Create `completion.rs` with `CompletionPopup`
- [ ] Wire into `editor.rs` (trigger on cursor move, insert on select)
- [ ] Add diagnostic squiggles to `highlighting.rs`
- [ ] Keyboard navigation (Up/Down/Tab/Esc)

**Deliverable**: Working auto-complete and inline diagnostics in the GUI.

### Phase 5: Hover + Go-to-Definition

**Goal**: Hover tooltips and jump-to-definition.

- [ ] Implement `hover_at()` — type info, docs for labels/types/properties
- [ ] Implement `definition_at()` — jump to label/component definition
- [ ] GUI: hover tooltip overlay
- [ ] GUI: Ctrl+Click go-to-definition

**Deliverable**: Hover shows info, Ctrl+Click jumps to definition.

### Phase 6: LSP Server

**Goal**: External editor support.

- [ ] Create `crates/animatix-lsp/` crate
- [ ] Implement `Backend` with `tower-lsp`
- [ ] Type conversion layer (`analyzer` ↔ `lsp-types`)
- [ ] stdio transport
- [ ] Test with VS Code / Neovim

**Deliverable**: `animatix-lsp` binary that external editors can connect to.

### Phase 7: Cross-file Analysis (future)

**Goal**: Completions across imported files.

- [ ] Extend `Analyzer` to accept multiple files
- [ ] Use `ModuleGraph` for import resolution
- [ ] Cross-file symbol table
- [ ] LSP: `workspace/symbol`, `textDocument/references`

**Deliverable**: Completions include symbols from imported files.

## Dependencies

### `animatix-analyzer/Cargo.toml`

```toml
[dependencies]
animatix = { path = "../animatix" }
tree-sitter = "0.26"
tree-sitter-animatix = { path = "../tree-sitter-animatix" }
```

### `animatix-lsp/Cargo.toml`

```toml
[dependencies]
animatix-analyzer = { path = "../animatix-analyzer" }
tower-lsp = "0.20"
lsp-types = "0.97"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

### `animatix-gui/Cargo.toml` (additions)

```toml
animatix-analyzer = { path = "../animatix-analyzer" }
```

## Testing Strategy

- **analyzer**: Pure unit tests, no I/O. Each method tested independently.
- **lsp**: Integration tests using `tower-lsp` test harness.
- **gui**: Manual testing + snapshot tests for completion popup rendering.

## Open Questions

1. **Parser public API**: Should `parse_program()` live in `animatix` or `animatix-analyzer`?
   - Recommendation: Keep in `animatix`, make `pub`. Analyzer re-exports.

2. **Tree-sitter vs chumsky for completions**: Which parser drives completion?
   - Recommendation: Tree-sitter for position lookup (it has `descendant_for_point`), chumsky AST for semantic info.

3. **Completion trigger**: On every keystroke or debounced?
   - Recommendation: Debounced (150ms) for GUI, immediate for LSP.

4. **Diagnostic refresh**: On every keystroke or on save?
   - Recommendation: Debounced (300ms) for GUI, on save for LSP (standard behavior).
