# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 4. Long-Term / Speculative

### 4.1 Per-Actor Updater with `dt`

**Issue:** No delta-time variable. Cannot do physics integration (velocity → position). No per-actor updater — only global `always`.

**Fix:** Inject `dt` into `frame_eval_env`. Introduce `updater actor { ... }` syntax for actor-local reactive logic.

**Effort:** High.

---

### 4.2 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 4.3 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 4.4 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Per-actor updater (4.1) | High | Medium |
| 2 | Green tree / trivia AST (4.3) | Very High | Low |
| 3 | Web Canvas (4.2) | Very High | Low |
