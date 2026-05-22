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
| P0 | CI/CD & Linting Infrastructure | Low | Very High |
| P0 | Error Handling Hardening | Medium | High |
| P0 | GPU Renderer Performance | Low | High |
| P1 | Dependency & Feature Flag Cleanup | Medium | High |
| P1 | Structural Refactoring (Megafiles, God Objects) | Medium | Medium |
| P1 | API Surface Cleanup | Low | Medium |
| P2 | Documentation & Doc Tooling | Low | Medium |
| P2 | Test Infrastructure Gaps | Medium | Medium |
| P2 | Benchmarks & Profiling | Medium | Low |

---

## P0 — Critical (Fix Now)

### P0.1 Add CI/CD Pipeline

**Gap:** No automated testing, linting, or formatting checks. Code quality relies entirely on manual verification.

**Work:**
- Add `.github/workflows/ci.yml` with: test, clippy (`-D warnings`), fmt (`--check`), doc build, and `cargo audit`.
- Add `rustfmt.toml` with standard formatting rules.
- Add `clippy.toml` or `#![deny(warnings)]` in crate roots (`animatix`, `animatix-syntax`, `animatix-analyzer`).
- Fix existing 30+ clippy warnings (e.g. `clone_on_copy`, `unnecessary_map_or`, collapsed `if`s).

**Refs:** Explorer audit §2–4. Clippy fails on `-D warnings` today.

**Effort:** 2–4 hours.

---

### P0.2 Harden Production Code Against Panics

**Gap:** `unwrap()`, `expect()`, and `panic!()` are used in production code paths that can fail at runtime (font loading, Typst compilation, expression evaluation, SVG import, path morphing).

**Files to fix:**
- `crates/animatix/src/renderer/text.rs:111` — `panic!` on font load failure. Should return `Result<Font, FontLoadError>`.
- `crates/animatix/src/renderer/text.rs:255–325` — `.expect()` on Typst compilation. Should propagate `Result`.
- `crates/animatix/src/timeline/utils.rs:672, 844, 866, 877` — `.expect("Evaluation failed")` and `panic!("Expected Object")`. Should return `Result<EvalError>`.
- `crates/animatix/src/timeline/svg_import.rs:1155, 1171` — `panic!("Expected ActorDecl")`. Use existing `SvgImportError`.
- `crates/animatix/src/timeline/morph.rs:671, 730` — `panic!("Expected MoveTo")`. Should return `Result`.
- `crates/animatix/src/timeline/actions/mod.rs:433, 435, 499, 600, 602, 682` — `.unwrap()` on `child_orders.get("row")`. Should use `Option` handling.

**Refs:** Explorer audit §1. ~45 unwraps / ~40 expects in `animatix` src alone.

**Effort:** 1–2 days.

---

### P0.3 Cache Render Texture in Window Renderer

**Gap:** `crates/animatix/src/renderer/window.rs:136–153` allocates a new `wgpu::Texture` every frame. This thrashes GPU memory and is a clear performance bug.

**Work:**
- Cache the render texture and only recreate on resize.
- Add a `resize()` method to `RendererCore` instead of inline recreation in `render()`.

**Refs:** Oracle audit §6.

**Effort:** 1 hour.

---

### P0.4 Standardize Error Types

**Gap:** Mixed error handling — some APIs return custom error types, others return `Result<T, String>`. Several custom errors lack `std::error::Error` impl.

**Work:**
- Introduce `thiserror` (already in dependency tree) for all library error types.
- Add `std::error::Error` impl for: `ParseError`, `IrLowerError`, `ExportError`, `ModuleError`.
- Replace all `Result<T, String>` in public APIs with concrete error types.
- Use `anyhow` for application-level errors in `animatix-gui` and CLI.

**Refs:** Explorer audit §6. `RenderError`, `EvalError`, `SvgImportError` already exist but are inconsistent.

**Effort:** 1–2 days.

---

## P1 — High (Fix Next)

### P1.1 Add Feature Flags to `animatix`

**Gap:** `animatix` unconditionally pulls in heavy dependencies (`wgpu`, `vello`, `typst`, `rsmpeg`, `naga`). Consumers like the LSP pay compile-time cost for GPU rendering and video encoding they never use.

**Proposed flags:**
```toml
default = ["render", "video", "text", "svg"]
render = ["wgpu", "vello", "naga", "bytemuck", "pollster"]
video = ["rsmpeg", "image"]
text = ["typst", "typst-layout", "mitex", "fontdue", "fontdb", "ttf-parser"]
svg = ["usvg", "roxmltree"]
```

**Also:**
- Remove direct `wgpu` dependency from `animatix-gui/Cargo.toml`; use `animatix::renderer` types.
- Verify if `naga` is actually needed at runtime or only build-time. Remove from `[dependencies]` if unused.
- Pin `vello` git dependency to a specific `rev` instead of floating `branch = "main"`.

**Refs:** Oracle audit §1, §8, §9.

**Effort:** 2–3 hours.

---

### P1.2 Finish or Revert AnimationTrack Tiered Migration

**Gap:** `AnimationTrack` in `crates/animatix/src/timeline/track.rs:418–535` contains BOTH old flat fields (`position`, `color`, `shape_type`, …) AND new tiered types (`ActorHeader`, `GeometryTier`, `StyleTier`, `ActorPayload`). The new types are defined but not used. This is the worst of both worlds: struct bloat plus stalled migration.

**Work:**
- Pick a window to migrate `build.rs`, `runtime.rs`, and `scene_eval.rs` call sites to the tiered API.
- Delete the ~35 flat `Option<PropertyTrack<T>>` fields once migrated.
- Alternatively, revert the tiered types if the migration is no longer desired.

**Refs:** Oracle audit §2, §5. `TrackFieldRef`/`TrackFieldMut`/`ActorField` enums are workarounds for the flat model.

**Effort:** 1–2 days.

---

### P1.3 Split Megafiles

**Gap:** Several files exceed 500 lines, violating single-responsibility and making review/testing painful.

**Files to split:**
- `crates/animatix-gui/src/source_edit.rs` (2,033 lines) → `actor_edits.rs`, `keyframe_edits.rs`, `scene_edits.rs`, `apply.rs`.
- `crates/animatix-gui/src/preview_canvas.rs` (2,093 lines) → `input.rs`, `render.rs`, `overlay.rs`.
- `crates/animatix-gui/src/app/mod.rs` (1,489 lines, ~80 fields) → extract `DocumentController`, `PreviewController`, `ExportController`, `SelectionController`.
- `crates/animatix/tests/timeline_tests.rs` (8,027 lines) → move tests into `#[cfg(test)]` modules in `track.rs`, `build.rs`, `scene_eval.rs`, etc.
- `crates/animatix/src/timeline/build/mod.rs` (1,461 lines) → split by build phase.
- `crates/animatix/src/timeline/modifier_runtime/ir.rs` (1,142 lines) → split by IR construct.

**Refs:** Oracle audit §2.

**Effort:** 4–6 hours.

---

### P1.4 Encapsulate Timeline Public API

**Gap:** `Timeline` has all fields `pub` (tracks, background_color, root_nodes, env, modifiers, frame_cache, text_compiler, hit_regions). No encapsulation boundary — any module can mutate internals directly.

**Work:**
- Make fields `pub(crate)` at minimum.
- Provide accessor methods for read-only access.
- `frame_cache`, `text_compiler`, and `hit_regions` should never be public.
- Remove wildcard re-exports in `animatix/src/lib.rs` (`pub use animatix_syntax::*`). Consumers should depend on `animatix-syntax` directly.
- Replace wildcard re-exports in `animatix-analyzer/src/lib.rs` with explicit `pub use` of intended types.

**Refs:** Oracle audit §3.

**Effort:** 3–4 hours.

---

### P1.5 Fix Broken Doc Links and Enforce Missing Docs

**Gap:** `cargo doc` produces 5 warnings about unresolved intra-doc links. No crate enforces `missing_docs`.

**Work:**
- Fix 5 broken links in `animatix-syntax/src/ast.rs`, `parser.rs`, `source_index.rs` (escape brackets or use backticks).
- Add `#![warn(missing_docs)]` to `animatix`, `animatix-syntax`, `animatix-analyzer` crate roots.
- Document public APIs that currently lack doc comments.

**Refs:** Explorer audit §7.

**Effort:** 2–3 hours.

---

### P1.6 Add `// SAFETY:` Comments to Unsafe Blocks

**Gap:** `crates/animatix/src/renderer/video.rs:74` has an `unsafe` block with no safety invariant documentation. `tree-sitter-animatix` FFI is acceptable but could also use comments.

**Work:**
- Add `// SAFETY:` comments explaining preconditions for all `unsafe` blocks.

**Refs:** Explorer audit §5.

**Effort:** 30 minutes.

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
