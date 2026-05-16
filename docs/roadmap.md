# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. Deferred Features

### 1.1 GUI Inspector: Animated Geometry Editing

**Status:** `Polygon.points` and `Path.commands` assignments work at source level. GUI widgets deferred.
**Location:** `crates/animatix-gui/src/app/panels/inspector/`.

No widget exists for editing variable-length lists of `Vec2` points or path commands. The inspector currently displays `"[N pts]"` / command string as read-only labels.

**Effort:** High (custom multi-point / command editor).

---





## 2. Deferred Architecture

### 2.1 Renderer `FontContext`

**Status:** System font discovery works (1.2 complete) but loads a temporary `fontdb::Database` on every text compile. ~45–60ms per call on a typical Linux system with 800+ fonts.
**Location:** `crates/animatix/src/renderer/text.rs`.

The current `SystemFontLoader` has no persistent state (good) but pays the full directory-scan cost every time `compile_text()` / `compile_math()` / `compile_code()` is called. A scene with 10 text elements incurs ~500ms of redundant font scanning per frame.

**Fix:** Introduce a `FontContext` struct that owns the `fontdb::Database` (metadata only, built once) and is threaded through the entire rendering pipeline:

```rust
pub struct FontContext {
    db: fontdb::Database, // built once at app startup
}

// compile_text now accepts context explicitly
pub fn compile_text(ctx: &FontContext, text: &str, ...) -> Frame

// Timeline owns the context
pub struct Timeline {
    font_context: FontContext,
    ...
}
```

**Scope of changes:**
- `Timeline::new()` → `Timeline::new(ctx: FontContext)`
- `Timeline::build()` → `Timeline::build(ast, ctx)`
- `TextCompiler::compile()` → `TextCompiler::compile(ctx, ...)`
- `BuildTarget::from_ast()` → `BuildTarget::from_ast(ast, namespaces, ctx)`
- ~200 call sites across tests, CLI, and GUI

**Effort:** Medium-High (touches ~15 files, mostly mechanical).
**Blocked until:** User demand justifies the refactor cost.

---

## 3. Architecture / Cleanup Debt

### 3.1 Dynamic Layout — Post-Migration Cleanup

**Location:** `docs/architecture.md` §Layout System.

- Richer `ContainerLayoutChild` entries than just labels.
- Reducing metadata duplication between `child_order` and `layout_children`.
- Retiring legacy `size` from non-layout subsystems if desired.

**Effort:** Low-Medium.

---

### 3.2 Randomness Determinism

**Status:** Documented caveat.
**Location:** `docs/architecture.md` §Reactive System.

Current `rand()` is not a deterministic function of time. Scenes depending on fresh randomness per evaluation break the random-access frame promise.

**Options:**
- Seed `rand()` from `t` + label hash for deterministic pseudo-randomness.
- Add `seeded_rand(t, seed)` builtin.

**Effort:** Low-Medium.

---

### 3.3 Plotting System — Per-Frame Sampling Cache

**Status:** Implemented. Procedural plots re-sample every frame regardless of whether the function references `t`.
**Location:** `crates/animatix/src/timeline/scene_eval.rs`.

`AnimationTrack::procedural_plot` is re-sampled on every call to `evaluate()` using `frame_eval_env`. For static plots (e.g., `func: (x) => x * x`), this is wasted work — the curve never changes.

**Fix:** Cache the sampled `Vec<VelloPath>` on the track and only re-sample when:
- The closure body references `t` or other time-varying variables, OR
- The cache key (hash of `t`, `x_domain`, `y_domain`, `t_domain`, `size`) changes.

For static plots, sample once at build time and skip re-sampling. For animated plots, cache by `t` rounded to the nearest frame.

**Effort:** Low-Medium.

---

### 3.4 Plotting System — `func` Signature Validation

**Status:** No validation. Users can pass wrong-arity or wrong-return-type closures without diagnostics.
**Location:** `crates/animatix/src/timeline/build/plot.rs` §`process_plot_actor`.

The `func` property accepts ad-hoc polymorphism:
- Cartesian: `(x) => Num`
- Polar: `(t) => Num` (radius)
- Parametric: `(t) => Vec2`
- Implicit: `(x, y) => Num`

Passing a scalar to parametric or a Vec2 to cartesian silently produces incorrect output.

**Fix:** At build time, evaluate the closure once with a test argument and verify the return type matches the plot type. Emit a diagnostic on mismatch.

**Effort:** Low.

---

### 3.5 Plotting System — Graph Axes Out-of-Domain Position

**Status:** When zero is outside the domain, axes are drawn at the plot boundary instead of being omitted.
**Location:** `crates/animatix/src/timeline/build/plot.rs` §`build_graph_axis_paths`.

For example, `x_domain: (2, 5)` draws the Y-axis at `x = -size[0]` (the left edge), which is visually misleading.

**Fix:** Omit the axis line entirely when zero is not in the domain, or clamp it to the visible edge with a clear visual indication.

**Effort:** Low.

---

## 4. Long-Term / Speculative

### 4.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 4.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 4.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 5. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Multi-Scene GUI transition blending (Phase 7 polish) | Medium | High |
| 2 | Randomness determinism | Low-Medium | Medium |
| 3 | Plotting: per-frame sampling cache | Low-Medium | Medium |
| 4 | Plotting: `func` signature validation | Low | Medium |
| 5 | Plotting: axes out-of-domain position | Low | Low |
| 6 | Dynamic layout cleanup | Low-Medium | Low (cleanup) |
| 7 | Cross-file analyzer | Medium-High | Medium |
| 8 | Green tree / trivia AST | Very High | Low (polish) |
