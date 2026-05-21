# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 3. Code Quality

### 3.1 Reduce unwrap/expect/panic Usage

**Issue:** ~403 unwrap/expect/panic instances across the codebase (293 unwrap + 89 expect + 21 panic). CLI crashes on bad input; long renders can fail mid-way losing all progress.

**Fix:** Layered error handling — parser uses `Diagnostic` for syntax errors; build uses `BuildReport` for semantic errors; runtime uses `Result` for frame errors. Prioritize high-frequency modules: `renderer/*`, `timeline/build.rs`, `parser.rs`.

**Effort:** High. Large refactor.

---

### 3.2 video.rs Unsafe Code Cleanup

**Issue:** Duplicated `rgba.as_ptr() as *mut u8` pointer casts in video export. Potential aliasing violations with `rsmpeg::ffi`.

**Fix:** Extract shared `fill_rgba_frame(ptr, w, h)` helper. Audit whether `rsmpeg::AVFrame::fill_arrays` truly requires mutable pointer. Add `// SAFETY:` comments.

**Effort:** Low.

---

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
| 1 | Reduce unwrap/panic (3.1) | High | Medium |
| 2 | video.rs unsafe cleanup (3.2) | Low | Low |
| 3 | Per-actor updater (4.1) | High | Medium |
| 4 | Green tree / trivia AST (4.3) | Very High | Low |
| 5 | Web Canvas (4.2) | Very High | Low |
