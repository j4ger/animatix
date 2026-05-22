# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## 1. Long-Term / Speculative

### 1.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 1.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 1.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 2. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Green tree / trivia AST (1.2) | Very High | Low |
| 2 | Web Canvas (1.1) | Very High | Low |
| 3 | Trivia-inspired AST (1.3) | High | Low |
