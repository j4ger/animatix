# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. GPU Memory Profiling

**Location:** `crates/animatix/src/renderer/`

Per-frame allocation tracking, staging belt growth monitoring, and renderer cache retention analysis. Needed to diagnose memory bloat during long preview sessions and large exports.

**Effort:** Medium

---

## 2. Architectural Cleanup

### 2.2 Generic `duration_seconds` Aggregation

**Location:** `crates/animatix/src/timeline/mod.rs`

`duration_seconds` manually aggregates max keyframe times from `tracks`, `background_color`, `child_orders`, and `variable_tracks`. Every new track-like collection requires a human to remember to update this function.

**Fix:** Store all duration-contributing collections behind a common trait (`HasDuration`) or iterate over a registry.

**Effort:** Low.

---

### 2.3 Primitive `default_props` Factory Helpers

**Location:** `crates/animatix/src/primitives/`

Every primitive defines `default_props()` by manually constructing `Property { name: ..., value: ..., value_span: None, trailing_comment: None }`. Only `plot.rs` has a `property()` helper. Adding any new field to `Property` requires touching ~67 construction sites.

**Fix:** Add a `Property::new(name, value)` constructor and migrate all primitive definitions to use it.

**Effort:** Low. Mechanical refactor.

---

## 3. Long-Term / Speculative

### 3.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 3.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 3.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 4. Design Notes

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | GPU memory profiling | Medium | Medium |
| 2 | Generic `duration_seconds` aggregation (2.2) | Low | Low (cleanup) |
| 3 | Primitive `Property` factory helpers (2.3) | Low | Low (cleanup) |
| 4 | Green tree / trivia AST (3.2) | Very High | Low (polish) |
