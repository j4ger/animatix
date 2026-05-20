# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Analyzer integration (7.1) | Medium | Low |
| 2 | Green tree / trivia AST (2.2) | Very High | Low (polish) |

---

## 2. Long-Term / Speculative

### 2.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 2.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 2.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 7. Deferred Work

### 7.1 Analyzer Integration

**Location:** `crates/animatix-analyzer/`

**Issue:** The analyzer crate has symbol table and diagnostics code, but it's not wired into the LSP or GUI. The analyzer collects symbols but nothing consumes them yet.

**Fix:** Wire the analyzer into the LSP for completions/diagnostics, or into the GUI for inline error display. The infrastructure exists but the integration layer is missing.

**Effort:** Medium. Not blocking any current work.
