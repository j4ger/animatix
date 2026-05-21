# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 2. Language Features

### ~~2.1 NumberPlane Component~~

**Status:** Fixed. Added `NumberPlane` primitive with `x_range`/`y_range` properties that auto-generate axes, grid lines, and tick marks at build time. See `examples/numberplane_demo.amx`.

---

### ~~2.2 Math Coordinate Auto-Mapping~~

**Status:** Fixed. Actors declared inside a `Graph` now have their `at`/`position`/`from`/`to` properties automatically mapped from math coordinates to screen pixels based on the parent's `x_domain`, `y_domain`, and `size`.

---

### ~~2.3 Full Affine Transform Matrix~~

**Status:** Fixed. Added `transform` property accepting a 6-element array `[a, b, c, d, tx, ty]` (full 2D affine matrix). Coexists with `rotation`/`scale` as independent transform layers. Multiplication order: `parent × translate(position) × transform(matrix) × rotate(rotation) × scale(scale)`.

---

### ~~2.4 Expand Modifier IR Expression Support~~

**Status:** Fixed. Modifier IR now compiles `Index` (`list[i]`, `vec[0]`) and `Method` (`str.length()`, `list.get(0)`, `num.abs()`) expressions into the fast bytecode path. `Closure` remains unsupported by design (would need statement-level IR support).

---

### ~~2.5 `for` Loop in `always`~~

**Status:** Fixed. `ForLoop` is now supported in `always` blocks via both the modifier IR fast path (bytecode VM with `BeginFor`/`CheckFor` instructions) and the runtime fallback path. Iterates over lists, arrays, and vec values.

---

### ~~2.6 `always`/Keyframe Conflict Mechanism~~

**Status:** Fixed. For every track property, an `_animating_{property}` boolean is injected into the frame environment (e.g., `circle._animating_opacity`). `always` blocks can check this flag to defer to active keyframe interpolation.

---

### ~~2.7 Expand Built-in Function Library~~

**Status:** Fixed. Added 11 new builtins to modifier IR: `tan`, `sqrt`, `exp`, `log`, `atan2`, `clamp`, `abs`, `min`, `max`, `floor`, `ceil`. Build-time environment already had these plus `asin`, `acos`, `round`, `log10`, `signum`, `fract`, `deg_to_rad`, `rad_to_deg`, `pow`, `hypot`, `rem`, `step`, `smoothstep`, `rand`, `seeded_rand`.

---

### ~~2.8 Group Batch Operations~~

**Status:** Fixed. Actions targeting a Group with children are automatically expanded to target all leaf descendants. Container-only actions (`reorder`, `swap`) skip expansion and target the container itself.

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
| 1 | ~~Expand builtin functions (2.7)~~ | Low | High |
| 2 | ~~NumberPlane component (2.1)~~ | Medium | High |
| 3 | ~~Math coordinate mapping (2.2)~~ | Medium | High |
| 4 | ~~Affine transform matrix (2.3)~~ | Medium | High |
| 5 | ~~Group batch operations (2.8)~~ | Medium | Medium |
| 6 | ~~Modifier IR expression expansion (2.4)~~ | Medium | Medium |
| 7 | ~~`for` in `always` (2.5)~~ | Medium | Medium |
| 8 | ~~`always`/keyframe priority (2.6)~~ | Medium | Medium |
| 9 | Reduce unwrap/panic (3.1) | High | Medium |
| 10 | video.rs unsafe cleanup (3.2) | Low | Low |
| 11 | Per-actor updater (4.1) | High | Medium |
| 12 | Green tree / trivia AST (4.3) | Very High | Low |
| 13 | Web Canvas (4.2) | Very High | Low |
