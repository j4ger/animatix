# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 1. Quick Wins

### ~~1.1 Document `always` Built-in Variables~~

**Status:** Fixed. Added "Built-in Variables" section to `spec.md` documenting `t` (scene-local time in seconds), `scene_width`, `scene_height`, and scene anchor points.

---

### ~~1.2 Add Missing Examples~~

**Status:** Fixed.
- `examples/arrow_demo.amx` — demonstrates Line with arrowheads via `size: (tip_len*2, tip_width*2)`
- `examples/plotting.amx` — extended with `VectorField` and `Heatmap`
- `examples/component_template_demo.amx` — demonstrates `pub component` as reusable parameterized templates

---

## 2. Language Features

### 2.1 NumberPlane Component

**Issue:** No dedicated math coordinate system. Drawing axes, grid lines, and tick labels requires ~20 manual `Line` actors. The existing `Grid` primitive is a UI layout container (CSS Grid semantics), not a math grid.

**Fix:** Add `NumberPlane` primitive with `x_range: (min, max, step)`, `y_range: (min, max, step)` that auto-generates axes + grid lines + tick labels. Support `fade-in grid` for bulk control.

**Effort:** Medium.

---

### 2.2 Math Coordinate Auto-Mapping

**Issue:** `Graph` has `x_domain`/`y_domain`, but child actors (e.g., `PlotCurve`, manual `Line` overlays) still use screen-pixel coordinates. Changing resolution breaks layout. Users must manually map `(2, 2)` → `(806, 180)`.

**Fix:** Make `Graph { ... }` a coordinate container. All children inside a `Graph` use math coordinates that are auto-mapped to screen pixels based on the graph's domain and size.

**Effort:** Medium. Requires coordinate transform propagation to child actors.

---

### 2.3 Full Affine Transform Matrix

**Issue:** `rotation: f32` and `scale: f32` are scalars. Cannot express shear, non-uniform scale, rotation about arbitrary points, reflection, or arbitrary 2×2 linear maps. `kurbo::Affine` supports full matrices, but the DSL only exposes 2 scalars.

**Fix:** Add `transform: [f64; 6]` property (full 2D affine matrix `[a, b, c, d, tx, ty]`). Coexists with existing `rotation`/`scale` as independent transform layers. Multiplication order: `parent × translate(position) × transform(matrix) × rotate(rotation) × scale(scale)`.

**Effort:** Medium. Requires property engine support for 6-element arrays and renderer integration.

---

### 2.4 Expand Modifier IR Expression Support

**Issue:** Build-time `evaluate_expr` supports conditionals, function calls, methods, indexing, and object construction. The `always`/modifier IR (`compile_expr`) only supports arithmetic, ternary conditionals, `Sin`/`Cos`/`Lerp`/`Format`. Writing the same expression in a keyframe vs. `always` produces different behavior — silent degradation to `ModifierExpr::Unsupported` with fallback to `evaluate_expr` (performance hit).

**Fix:** Extend modifier IR to support at least `Index` (array indexing), `Method` (method calls), and `Closure` (closure literals). Priority: `Index` > `Method` > `Closure`.

**Effort:** Medium. Requires new IR variants and evaluator branches.

---

### 2.5 `for` Loop in `always`

**Issue:** `lower_modifier_stmt` returns `UnsupportedStatement` for `ForLoop`. Cannot batch-update a group of objects (e.g., 10 particles) in a reactive block.

**Fix:** Add `For` instruction to modifier IR. Iterate over arrays or `Range` values.

**Effort:** Medium.

---

### 2.6 `always`/Keyframe Conflict Mechanism

**Issue:** When both `always` and keyframes write the same property, behavior is unspecified. `architecture.md` states "`always` overrides keyframes" as a composition rule, but there's no explicit priority system and no way for `always` to detect when a keyframe is actively animating a property.

**Fix:** Introduce explicit priority layers (e.g., `always` as base priority 0, keyframes as override priority 100). Alternatively, expose `is_animating(property)` predicate in `always` so reactive blocks can defer to keyframe interpolation.

**Effort:** Medium. Requires design decision on API shape.

---

### 2.7 Expand Built-in Function Library

**Issue:** Only 4 built-ins: `Sin`, `Cos`, `Lerp`, `Format`. Missing basic math: `tan`, `sqrt`, `exp`, `log`, `atan2`, `clamp`, `abs`, `min`/`max`, `random`, `floor`, `ceil`.

**Fix:** Add to `BuiltinFn` enum and `CallBuiltin` handler. ~20 lines per function.

**Effort:** Low.

---

### 2.8 Group Batch Operations

**Issue:** `ActorKindId::Group` exists for logical grouping and hierarchy, but actions cannot target a group. Winding scene exit requires 9 separate `fade-out` lines.

**Fix:** Allow `fade-in`/`fade-out`/`opacity`/`transform` to target a `Group`, recursively applying to all children. `Group` itself should support `transform` for bulk move/rotate/deform.

**Effort:** Medium.

---

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
| 1 | ~~Document `always` variables (1.1)~~ | Low | High |
| 2 | ~~Add missing examples (1.2)~~ | Low | High |
| 3 | Expand builtin functions (2.7) | Low | High |
| 4 | NumberPlane component (2.1) | Medium | High |
| 5 | Math coordinate mapping (2.2) | Medium | High |
| 6 | Affine transform matrix (2.3) | Medium | High |
| 7 | Group batch operations (2.8) | Medium | Medium |
| 8 | Modifier IR expression expansion (2.4) | Medium | Medium |
| 9 | `for` in `always` (2.5) | Medium | Medium |
| 10 | `always`/keyframe priority (2.6) | Medium | Medium |
| 11 | Reduce unwrap/panic (3.1) | High | Medium |
| 12 | video.rs unsafe cleanup (3.2) | Low | Low |
| 13 | Per-actor updater (4.1) | High | Medium |
| 14 | Green tree / trivia AST (4.3) | Very High | Low |
| 15 | Web Canvas (4.2) | Very High | Low |
