# Animatix Source Formatting Specification

> Version: 1.0  
> Scope: serializer output (`animatix::to_source`) and GUI write-back  
> Goal: deterministic, readable `.amx` source that matches hand-authored style.

---

## 1. General Principles

1. **Consistency** — Re-serializing already-formatted code produces byte-identical output.
2. **Readability** — Vertical space separates logical units; horizontal space is conserved.
3. **Determinism** — Formatting is entirely structural; no width heuristics or line-length limits.

---

## 2. Indentation

| Item | Value |
|---|---|
| Indent unit | 2 spaces (U+0020) |
| Tab characters | Never emitted |
| Increase | One level per nested block or child list |
| Decrease | One level when closing a block or child list |

---

## 3. Top-Level Layout

- Each **top-level** statement is separated by **one blank line** (`\n\n`).
- Inside a block (e.g. `sequence { … }`) statements are separated by a **single newline** (`\n`) only.
- Keyframe blocks (`#2s`, `#+500ms`) are top-level statements, so they are separated by blank lines from neighbours.

---

## 4. Actor Declarations

### 4.1 Without children (flat)

```amx
label: Type, prop1: value1, prop2: value2 [mod1, mod2]
```

Rules:
- Label and type are separated by `: ` (colon + space).
- Properties are comma-separated on the **same line**.
- Modifiers follow properties in `[…]` brackets.
- No trailing comma after the last property.

### 4.2 With children (container)

```amx
label: Type, prop1: value1 {
  child1: Type, prop1: value1
  child2: Type, prop2: value2
}
```

Rules:
- All properties and modifiers stay on the **header line**.
- Opening `{` is preceded by a single space and follows the last modifier.
- Each child gets its **own line**, indented +1 level.
- Closing `}` gets its **own line** at the parent's indentation level.
- No trailing comma after the last child.

### 4.3 Anonymous inline items

Anonymous items (no label) follow the same rule:

```amx
Col, gap: 16 {
  Rect, size: (100, 50)
  Text, text: "hello"
}
```

---

## 5. Container Children (`InlineItem`)

- Every `InlineItem` child renders on its **own line**.
- Children are separated by newline, **not** comma.
- Nested children follow the rule recursively (each level adds 2 spaces).

### Slot fills

```amx
@header {
  title: Text, text: "Welcome", font_size: 48
  subtitle: Text, text: "Subtitle", font_size: 24
}
```

Rules identical to actor children: one item per line, indented inside braces.

---

## 6. Block Statements

### 6.1 `sequence`, `stagger`, `always`, `for`

```amx
sequence {
  fade-in a [400ms]
  fade-in b [400ms]
}
```

```amx
stagger [100ms] {
  fade-in label [400ms]
  fade-in a [400ms]
}
```

- Header stays on one line.
- Body statements each get their own line, indented +1 level.
- Closing brace on its own line at the parent's indentation level.

### 6.2 `if` / `else`

```amx
if condition {
  then_stmt1
  then_stmt2
} else {
  else_stmt1
}
```

- `else` is separated from the closing `}` of the `if` by a single space.
- No newline between `}` and `else`.

### 6.3 `component` definitions and `action`

```amx
pub component Card(title, color) {
  bg: Rect, size: (200, 100), color: color
  label: Text, text: title
}
```

Body follows the same block rules as above.

---

## 7. Keyframe Blocks

```amx
#2s
stmt1
stmt2

#+500ms
stmt3
```

- Time marker on its own line.
- Body statements each on their own line, **not indented** (they sit at the same level as the marker).
- A blank line separates consecutive keyframe blocks at the top level.

---

## 8. Properties

- Properties within an actor declaration stay on a **single line**, comma-separated.
- Property values that are complex expressions (tuples, method calls, conditionals) are emitted inline.

```amx
btn: Rect, size: (100, 200), color: accent.primary
```

---

## 9. Comments

- **Trailing comments** (`// comment`) are preserved with exactly **2 spaces** before `//`.
- No blank lines are injected around comments.

```amx
btn: Rect, size: (100, 200)  // half-extents
```

> Note: Block comments (`/* … */`) are not supported by the parser and therefore never emitted.

---

## 10. Expression Formatting

Expressions are always emitted **inline**; they never contain newlines.

```amx
(a + b) * c
rgb(255, 128, 0)
"Say \"hello\""
```

---

## 11. Config Statements

`config` keeps its settings inline (it is typically short):

```amx
config { resolution: (1280, 720), dynamic_layout: true }
```

---

## 12. Formatting Fit in the Write-Back Pipeline

The GUI inspector mutates the AST via `source_edit::apply_edit`, then the entire file is re-serialized. Formatting is applied at the **serialization layer only** (`animatix::to_source`). No formatting state is carried through the edit; the serializer is the single source of truth for layout.

```
Inspector edit
      ↓
source_edit::apply_edit  (semantic AST mutation)
      ↓
animatix::to_source::stmts_to_source  (formatting applied here)
      ↓
editor.replace_text(new_source)
```

Because the serializer is deterministic, repeated edits converge to the same layout regardless of the original hand-formatting.

---

## 13. Future Extensions (Non-Goals for v1)

- Soft line-length limits (e.g. wrap property lists when > 80 cols).
- Preserving user-supplied blank lines between statements.
- Sorting properties alphabetically.
- Collapsing single-child containers to inline form.

These may be added later if they prove useful, but they are intentionally out of scope for the initial spec to keep complexity low.
