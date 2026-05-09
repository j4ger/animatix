# Diagnostics Overhaul Plan

> Created: 2026-05-09
> Status: Completed (2026-05-09)

## Problem Statement

The current diagnostics pipeline discards almost all useful information from chumsky's `Rich` parse errors before it reaches the user. A syntax error like `actor Foo { color: , }` produces a verbose debug-formatted blob with no line number, no source context, and no actionable guidance. The GUI then displays this as a static banner message and an unreadable overlay.

## Current Data Loss at Each Stage

| Stage | Type | Spans | Line/Col | Expected Tokens | File Path | Multiple Errors |
|-------|------|-------|----------|-----------------|-----------|-----------------|
| Parser | `Rich<'src, char>` | ✅ byte offsets | ❌ | ✅ | ❌ | ✅ |
| ModuleGraph | `ModuleError::ParseError(String)` | ❌ | ❌ | ❌ (in debug str) | ❌ | ❌ (joined) |
| Document | `Diagnostic` | ❌ | ❌ | ❌ | ✅ entry file only | ❌ (single) |
| GUI | `&'static str` banner + `String` overlay | ❌ | ❌ | ❌ | ❌ | ❌ (count only) |

## Phase 1: Structured Error Capture (parser → module)

### 1.1 Add location fields to `DiagnosticLocation`

```rust
pub struct DiagnosticLocation {
    pub path: Option<PathBuf>,
    pub subject: Option<String>,
    pub line: Option<usize>,       // NEW
    pub column: Option<usize>,     // NEW
    pub span: Option<Range<usize>>, // NEW: byte offsets
}
```

### 1.2 Create a `ParseError` struct

```rust
pub struct ParseError {
    pub message: String,
    pub span: Range<usize>,
    pub line: usize,
    pub column: usize,
    pub expected: Vec<String>,
    pub found: Option<String>,
    pub context: Vec<String>,
}
```

Build a `from_rich()` formatter that converts `Rich<'src, char>` → `ParseError`, converting byte offsets to line:col and formatting expected/found tokens for humans.

### 1.3 Update `ModuleError`

Change `ModuleError::ParseError(String)` → `ModuleError::ParseErrors(Vec<ParseError>)`. Preserve structured data instead of flattening to a debug string.

### 1.4 Preserve multiple errors as separate `Diagnostic`s

In `DocumentSession::rebuild()`, map each `ParseError` to its own `Diagnostic` with location info, instead of collapsing all errors into a single `SourceLoadFailure` diagnostic.

## Phase 2: Enrich Parser Error Messages

### 2.1 Add `.labelled()` and `.as_context()` to major parser combinators

Transform error messages from:
```
expected '-', '!', '(', '{', '"', digit, identifier
```
to:
```
expected expression
  while parsing property value
  while parsing actor declaration
```

Key sites: `expr`, `property`, `actor_decl`, `keyframe_decl`, `import_decl`, `config_decl`, `colorscheme_decl`.

### 2.2 Add error recovery

Add `recover_with(skip_then_retry_until(...))` at statement boundaries so the parser reports multiple errors per file instead of stopping at the first one.

## Phase 3: GUI Error Display Overhaul

### 3.1 Replace static banner with actual error messages

Show the first error message (truncated) in the banner instead of a hardcoded string.

### 3.2 Unify error channels

`preview.error` (String) and `document.diagnostics` (Vec<Diagnostic>) are duplicate channels. Remove `preview.error`; make the overlay draw from `combined_diagnostics()`.

### 3.3 Improve preview overlay

- Move to top-left or top-right (doesn't obscure timeline)
- Multi-line capable with wrapping
- Show first line + "... (+N more)" for multiple errors
- Click to expand diagnostics panel

### 3.4 Add expandable diagnostics panel

A scrollable panel showing:
- Error count by phase
- Individual error list with line:col, message, severity icon
- Expandable source context (source line with underline)
- Click-to-jump to editor line

### 3.5 Transport bar enhancements

Show count: "⚠ 3 errors, 1 warning" or "⚠ Parse: 2 errors". Make it a button that opens the diagnostics panel.

## Phase 4: Cell Editor Integration

### 4.1 Per-cell validation

When a cell loses focus, attempt to parse just that cell's content. Show a subtle indicator (red border or `!` icon) on cells with errors.

### 4.2 Map parse errors back to cells

When the full parser reports errors by line number, map those lines back to the offending cell and highlight it.

### 4.3 Cell editor error display

Show error text below the cell body in small red text when a keyframe body has a parse error.

## Related Improvements

- **Cell editor `parse_cells()`**: Currently has no error reporting. Add `Result<Vec<Cell>, CellParseError>`.
- **`DocumentSession::rebuild()`**: Returns `Result<(), String>` where the String is redundant with `self.diagnostics`. Change to `Result<(), ()>`.
- **`format_diagnostic()`**: Exists in `diagnostics.rs` but is never used in the GUI. Use it as the base for all error display.
- **`clear_render_error()` guard**: Fragile string comparison between `preview.error` and `render_diagnostics`. Unify on `Vec<Diagnostic>`.

## Implementation Order

| Priority | Task | Files | Effort |
|---|---|---|---|
| P0 | Add `line`, `column`, `span` to `DiagnosticLocation` | `diagnostics.rs` | Small |
| P0 | Create `ParseError` struct and `from_rich()` formatter | `parser.rs` (new) | Medium |
| P0 | Change `ModuleError::ParseError(String)` → `ParseErrors(Vec<ParseError>)` | `module.rs`, callers | Medium |
| P0 | Preserve multiple errors as separate `Diagnostic`s in `DocumentSession` | `document.rs` | Small |
| P1 | Add `.labelled()` / `.as_context()` to major parser combinators | `parser.rs` | Medium |
| P1 | Add error recovery (`recover_with()`) at statement level | `parser.rs` | Medium |
| P1 | Replace `preview.error` overlay with `combined_diagnostics()` | `workspace.rs`, `app.rs` | Small |
| P1 | Make diagnostics banner show actual error message | `app.rs` | Small |
| P2 | Add expandable diagnostics panel with error list | `workspace.rs` | Large |
| P2 | Add click-to-jump from error to editor line | `workspace.rs`, `editor.rs` | Medium |
| P2 | Transport bar shows error count by phase | `transport_bar.rs` | Small |
| P3 | Cell editor: per-cell validation and error indicators | `cell_editor/` | Large |
| P3 | Cell editor: map parse errors back to cells by line number | `cell_editor/`, `document.rs` | Medium |

## Success Criteria

1. A syntax error like `actor Foo { color: , }` shows: **"line 1, col 23: expected expression while parsing property"** instead of a debug blob
2. Multiple syntax errors in one file produce **separate diagnostics** with individual locations
3. The GUI banner shows the **actual first error message**, not a static string
4. The preview overlay is **readable** (multi-line capable, positioned out of the way)
5. Users can **see a list of all errors** in a scrollable panel
6. Clicking an error **scrolls the editor to that line**
7. The cell editor shows **which cell contains an error** (border/icon indicator)
8. Parser reports **multiple errors per file** (not stopping at the first)
