# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge again.

---

## Priority Summary

| Priority | Theme | Effort | Impact |
|----------|-------|--------|--------|
| P2 | Documentation & Doc Tooling | Low | Medium |
| P2 | Test Infrastructure Gaps | Medium | Medium |
| P2 | Benchmarks & Profiling | Medium | Low |

---

## P2 — Medium (Fix Soon)

### P2.1 Add Benchmark Infrastructure

**Gap:** No `benches/` directory, no `criterion` dependency, no performance regression testing.

**Work:**
- Add `criterion` to `[dev-dependencies]`.
- Add benchmarks for hot paths: timeline evaluation per frame, modifier runtime, property interpolation.
- Add benchmark CI job to track regressions.

**Refs:** Explorer audit §6.

**Effort:** 2–3 hours.

---

### P2.2 Profile and Reduce Clone Hot Paths

**Gap:** Heavy `.clone()` usage in `animatix-gui/src/app/mod.rs` (~50+ clones) and `timeline/modifier_runtime/ir.rs` (~33 clones). May or may not be performance-critical — needs measurement.

**Work:**
- Profile with `cargo flamegraph` or `perf` to identify actual hot paths.
- Consider `Arc<String>` for source text, `Arc<Timeline>` for shared timeline data.
- Use `Cow` for read-only environment access in modifier evaluation.

**Refs:** Explorer audit §3.

**Effort:** 2–4 hours (after benchmarks exist).

---

### P2.3 Add LSP and GUI Tests

**Gap:** `animatix-lsp` has no tests. `animatix-gui` has minimal testing.

**Work:**
- Add LSP integration tests using `lsp-types` mock client.
- Consider `eframe` headless testing or screenshot regression for GUI.

**Refs:** Explorer audit §8.

**Effort:** 1–2 days.

---

### P2.4 Clean Up Dead Code

**Gap:** Several `#[allow(dead_code)]` annotations. Some may be intentional (future API), others may be genuinely unused.

**Files:**
- `animatix-analyzer/src/lib.rs:41, 43` — `FileEntry.source` and `FileEntry.ast` fields.
- `animatix/src/timeline/shapes/primitives.rs` — helper functions.
- `animatix/src/timeline/property_lookup.rs:172` — `set_lookup_scalar`.

**Work:**
- Run `cargo clippy` to identify actual unused items.
- Remove genuinely dead code; document why kept for intentional cases.

**Refs:** Explorer audit §2.

**Effort:** 1 hour.

---

### P2.5 Track and Remove TODO Comments

**Gap:** TODO comments exist but are not tracked as issues.

**Work:**
- `crates/animatix-gui/src/app/shell/nl_command_bar.rs:63` — "Send command to agent for processing".
- `crates/animatix/src/timeline/scene_eval.rs:513` — backdrop blur full implementation.
- Create GitHub issues for each TODO, then remove comments once tracked.

**Refs:** Explorer audit §4.

**Effort:** 30 minutes.

---

### P2.6 Add MSRV and Version Constraints

**Gap:** No `rust-version` field in any `Cargo.toml`. Edition 2024 is very new.

**Work:**
- Add `rust-version = "1.85"` (or appropriate) to all crate `Cargo.toml` files.
- Document FFmpeg 7.1 requirement prominently in `README.md` and `CONTRIBUTING.md`.

**Refs:** Explorer audit §8.

**Effort:** 30 minutes.

---

## 3. Long-Term / Speculative

### 3.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 3.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation.

**Effort:** Very High. 3–6 month project. Not justified at current scale.

---

### 3.3 Trivia-Inspired AST

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

### 3.4 Split `animatix` into Core + Render Crates

**Location:** Oracle audit §1.

The `animatix` crate mixes timeline evaluation, primitive dispatch, and GPU rendering. Consider splitting:
- `animatix-core` — parser, AST, timeline, evaluation
- `animatix-render` — Vello/WGPU renderer, text rasterizer, video encoder

**Effort:** High. Would simplify feature flags and reduce LSP compile times significantly.

---

## Deferred / Blocked

None currently.
