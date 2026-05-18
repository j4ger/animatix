# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. GPU Memory Profiling

**Location:** `crates/animatix/src/renderer/`

Per-frame allocation tracking, staging belt growth monitoring, and renderer cache retention analysis. Needed to diagnose memory bloat during long preview sessions and large exports.

**Effort:** Medium

---

## 2. Language / Primitives

### 2.2 Per-Actor Reactive Blocks

**Location:** `crates/animatix/src/parser.rs`, `crates/animatix/src/timeline/`

Currently `always { ... }` is global — it must reference actors by label. Users want per-actor reactive logic (e.g. `animate tracker { at = expr }`) so individual actors can have independent frame-by-frame updates without polluting a global block.

See discussion in §Design Notes below.

**Effort:** Medium. Parser change + modifier compilation path.

---

### 2.3 Keyframe-Scoped Variables for Stateless `always`

**Location:** `crates/animatix/src/parser.rs`, `crates/animatix/src/timeline/`

Allow keyframes to declare variables (`#3s let freq = 1.7`) that are captured by `always` blocks. This gives state-like behavior without mutable state, preserving random-access frame rendering.

See discussion in §Design Notes below.

**Effort:** Medium. Requires variable tracks in the timeline + environment injection.

---

### 2.4 Reactive Binding Syntax (`:=`)

**Location:** `crates/animatix/src/parser.rs`, `crates/animatix/src/ast.rs`

Introduce `:=` (colon-equals) as syntactic sugar for single-property reactive bindings.

```amx
// Lightweight reactive property — evaluated every frame
tracker: Circle, radius: 10, at := (640 + 180 * cos(t), 360 + 180 * sin(t))

// Post-declaration reactive override
orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))
```

Desugars to `always { actor.prop = expr }` under the hood. Complements (does not replace) `always` blocks for multi-line logic and the future per-actor `drive` / `animate` keyword for multi-property reactive blocks.

**Why `:=` instead of a keyword:** No new lexical tokens needed; locality (intent attached to property); familiar from assignment-heavy languages.

**Open question:** Order dependence when `:=` bindings reference each other (e.g. `orbiter.at := tracker.at + ...`). May require dependency graph construction or simple left-to-right evaluation order.

**Effort:** Low–Medium. Parser token + AST variant + desugar pass.

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

### Per-Actor Reactive Syntax (2.2)

Options considered:

```amx
// Option A: keyword + target (mirrors fade-out actor)
drive tracker {
  at = (640 + 100 * cos(t), 360 + 100 * sin(t))
}

// Option B: always with target
always tracker {
  at = expr
}

// Option C: nested inside actor declaration
tracker: Circle, radius: 10 {
  animate {
    at = expr
  }
}
```

**Preference:** Option A (`drive` or `animate`) — it keeps actor declarations clean and reads like an imperative verb. Option B overloads `always` which currently means "global." Option C creates nesting depth issues.

### Keyframe-Scoped Variables (2.3)

Instead of mutable state inside `always`, allow keyframes to bind variables that `always` reads:

```amx
#0s let freq = 1.0
#3s let freq = 1.7

always {
  tracker.at = (640 + 100 * cos(freq * t), 360 + 100 * sin(freq * t))
}
```

This is stateless — `freq` is a piecewise-constant function of time, defined declaratively by keyframes. Random frame access requires only evaluating the variable track at that time, not simulating all previous frames.

**Effort:** Medium. Needs parser support for `let` in keyframe position, timeline variable tracks, and environment injection into `always` evaluation.

### Reactive Binding `:=` (2.4)

For single-property reactivity, `:=` is preferred over blocks:

```amx
// One-liner reactive property
tracker: Circle, radius: 10, at := (640 + 180 * cos(t), 360 + 180 * sin(t))

// Multi-line per-actor logic (future keyword, complement to :=)
drive tracker {
  let angle = freq * t
  at = (640 + 180 * cos(angle), 360 + 180 * sin(angle))
  rotation = angle * 180 / PI
}
```

`:=` desugars to `always { actor.prop = expr }`. `drive` (or `animate`) handles the multi-property case with implicit actor scoping.

**Effort:** Low (`:=` token); Medium (`drive` keyword).

---

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | GPU memory profiling | Medium | Medium |
| 2 | Reactive binding `:=` (2.4) | Low–Medium | Medium |
| 3 | Keyframe-scoped variables (2.3) | Medium | Medium |
| 4 | Per-actor reactive blocks / `drive` keyword (2.2) | Medium | Medium |
| 5 | Green tree / trivia AST (3.2) | Very High | Low (polish) |
