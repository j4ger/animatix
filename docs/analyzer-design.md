# animatix-analyzer: Shared Language Intelligence

## Problem

The GUI editor needs completion, diagnostics, hover, and go-to-definition. External editors (VS Code, Neovim) need the same via LSP. Without a shared core, we'd duplicate logic or force the GUI through LSP overhead.

## Solution

Extract language intelligence into `animatix-analyzer` — a pure computation crate with no I/O, no GUI, no LSP. Both the GUI and LSP server consume it as a library.

## Architecture

```
crates/
  animatix/              # parser, AST, timeline (existing)
  animatix-analyzer/     # shared language intelligence
  animatix-lsp/          # LSP server binary
  animatix-gui/          # egui app (existing, modified)
  tree-sitter-animatix/  # tree-sitter grammar (existing)
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
5. **Type bridge** — analyzer uses its own types; LSP crate has inline conversions

## Crate: `animatix-analyzer`

### Public API

```rust
pub struct Analyzer { /* Clone */ }
pub struct SymbolTable { /* labels, types, components, properties, keywords, actions */ }
pub struct LabelInfo { name, kind, line, col, ty }
pub struct ComponentInfo { name, params, line, col }
pub struct CompletionItem { label, kind, detail, documentation, insert_text }
pub enum CompletionKind { Keyword, Type, Property, Label, Action, Value, Snippet }
pub struct Diagnostic { severity, line, col, end_line, end_col, message, code }
pub enum DiagnosticSeverity { Error, Warning, Info, Hint }
pub struct HoverInfo { contents, range }
pub struct Location { file, line, col }
pub struct DocumentSymbol { name, kind, line, col, detail }
pub enum SymbolKind { Actor, Variable, Component, Block }
```

### Core Methods

```rust
impl Analyzer {
    pub fn new(source: &str) -> Self;
    pub fn update(&mut self, source: &str);
    pub fn completions_at(&self, line: usize, col: usize) -> Vec<CompletionItem>;
    pub fn diagnostics(&self) -> Vec<Diagnostic>;
    pub fn hover_at(&self, line: usize, col: usize) -> Option<HoverInfo>;
    pub fn definition_at(&self, line: usize, col: usize) -> Option<Location>;
    pub fn document_symbols(&self) -> Vec<DocumentSymbol>;
}
```

### Completion Contexts

| Context | Trigger | Suggestions |
|---------|---------|-------------|
| TopLevel | Start of file | Keywords + snippets + labels + types + actions |
| TypePosition | After `:` in declaration | Types only |
| PropertyBlock | Inside `{ }` | Properties for actor type + values |
| ActionTarget | After verb like `move` | Labels (actor names) |
| ModifierList | Inside `[ ]` | delay, ease, duration |
| PropertyValue | After `=` or `:` in property | Context-specific values |

### Diagnostic Checks

| Check | Severity | Code |
|-------|----------|------|
| Tree-sitter ERROR/MISSING nodes | Error | `syntax` |
| Chumsky parse errors | Error | `parse-N` |
| Unknown action verb | Warning | `unknown-action` |
| Undefined label reference | Warning | `undefined-label` |
| Unknown type name | Warning | `unknown-type` |
| Unknown property for type | Info | `unknown-property` |
| Duplicate label definition | Warning | `duplicate-label` |

### Hover Information

Hover provides markdown documentation for:
- **Labels**: kind (Actor/Variable/Component) + type
- **Types**: description (e.g., "Text element with content and styling")
- **Actions**: description + usage example
- **Keywords**: description + syntax
- **Literals**: value display

## Crate: `animatix-lsp`

Thin wrapper around `animatix-analyzer` using `tower-lsp`.

### Implemented Capabilities

| LSP Method | Analyzer Call | Status |
|------------|---------------|--------|
| `textDocument/completion` | `completions_at()` | ✅ |
| `textDocument/hover` | `hover_at()` | ✅ |
| `textDocument/definition` | `definition_at()` | ✅ |
| `textDocument/documentSymbol` | `document_symbols()` | ✅ |
| `textDocument/didOpen` | `update()` | ✅ |
| `textDocument/didChange` | `update()` | ✅ |

### Trigger Characters

- `:` — trigger type completion after label
- `.` — trigger property completion in paths
- ` ` — trigger general completion

### Usage

```bash
# Run directly (communicates via stdin/stdout)
animatix-lsp
```

**VS Code** (`.vscode/settings.json`):
```json
{
  "amx.languageServer": {
    "command": "animatix-lsp",
    "args": []
  }
}
```

**Neovim** (nvim-lspconfig):
```lua
require('lspconfig').animatix.setup {
  cmd = { 'animatix-lsp' },
  filetypes = { 'amx' },
  root_dir = require('lspconfig').util.root_pattern('.git', 'Cargo.toml'),
}
```

## Crate: `animatix-gui` (modifications)

### New: `completion_popup.rs`

Completion popup widget with:
- Keyboard navigation (Up/Down/Tab/Esc)
- Filtering by trigger text
- Color-coded icons per completion kind
- Scroll support for long lists
- Click outside to dismiss

### New: `highlighting.rs`

Tree-sitter syntax highlighting with diagnostic squiggles:
- 14 highlight groups (keyword, type, string, number, comment, operator, etc.)
- Gruvbox-inspired dark/light themes
- Diagnostic background tints (red=error, yellow=warning, blue=info)
- Cached highlighting (invalidated on text change)

### Modified: `editor.rs`

Editor buffer with integrated language intelligence:
- `Analyzer` instance for completions, diagnostics, hover
- `CompletionPopup` for auto-complete UI
- Ctrl+Space to trigger completion
- Auto-trigger on `:`, `.`, ` ` characters
- Hover tooltip on mouse hover
- Ctrl+Click go-to-definition
- Diagnostic squiggles via highlighting

## Implementation Status

### Phase 1: Foundation ✅

- [x] Create `crates/animatix-analyzer/` crate
- [x] Make parser public in `animatix`
- [x] Implement `SymbolTable::build_from_ast(&[Stmt])`
- [x] Implement `Analyzer::new()` and `Analyzer::update()`
- [x] Unit tests for symbol extraction

### Phase 2: Completion ✅

- [x] Implement `completions_at()` with context detection
- [x] Add semantic completions from `SymbolTable`
- [x] Add snippet completions (actor, keyframe, component, if, for, etc.)
- [x] Unit tests for each completion context

### Phase 3: Diagnostics ✅

- [x] Implement `diagnostics()` from tree-sitter ERROR nodes
- [x] Add chumsky error conversion
- [x] Add semantic checks (undefined labels, unknown actions/types/properties)
- [x] Unit tests

### Phase 4: GUI Integration ✅

- [x] Create `completion_popup.rs` with `CompletionPopup`
- [x] Wire into `editor.rs` (trigger on Ctrl+Space, insert on select)
- [x] Add diagnostic squiggles to `highlighting.rs`
- [x] Keyboard navigation (Up/Down/Tab/Esc)

### Phase 5: Hover + Go-to-Definition ✅

- [x] Implement `hover_at()` — type info, docs for labels/types/properties
- [x] Implement `definition_at()` — jump to label/component definition
- [x] GUI: hover tooltip overlay
- [x] GUI: Ctrl+Click go-to-definition

### Phase 6: LSP Server ✅

- [x] Create `crates/animatix-lsp/` crate
- [x] Implement `Backend` with `tower-lsp`
- [x] Type conversion (analyzer types → lsp-types inline)
- [x] stdio transport
- [x] Completion, hover, definition, document symbols

### Phase 7: Cross-file Analysis (future)

- [ ] Extend `Analyzer` to accept multiple files
- [ ] Use `ModuleGraph` for import resolution
- [ ] Cross-file symbol table
- [ ] LSP: `workspace/symbol`, `textDocument/references`

## Dependencies

### `animatix-analyzer/Cargo.toml`

```toml
[dependencies]
animatix = { path = "../animatix" }
chumsky = "0.12"
tree-sitter = "0.26"
tree-sitter-animatix = { path = "../tree-sitter-animatix" }
```

### `animatix-lsp/Cargo.toml`

```toml
[dependencies]
animatix-analyzer = { path = "../animatix-analyzer" }
tower-lsp = "0.20"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

### `animatix-gui/Cargo.toml` (additions)

```toml
animatix-analyzer = { path = "../animatix-analyzer" }
```

## Testing

- **animatix-analyzer**: 18 unit tests (symbol extraction, completions, diagnostics)
- **animatix-lsp**: Compiles, no unit tests (manual testing with editors)
- **animatix-gui**: 383 total tests pass across all crates

## Design Decisions

1. **Tree-sitter for position lookup, chumsky AST for semantic info** — tree-sitter has `descendant_for_point_range()` for cursor context; chumsky AST has richer type information for completions.

2. **Analyzer owns both parsers** — `Analyzer::new()` parses with both chumsky and tree-sitter, builds symbol table from chumsky AST.

3. **No separate type conversion layer** — LSP crate converts analyzer types to lsp-types inline (~50 lines), avoiding a separate conversion module.

4. **Completion popup is pure egui** — no external widget library, custom rendering with keyboard navigation.

5. **Diagnostic squiggles via LayoutJob background** — integrates with existing syntax highlighting, no separate overlay layer.
