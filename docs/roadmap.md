# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge.

---

## P2 — Maintainability & Code Quality (Post-Audit)

> Findings from oracle audit of all crates. Ordered by impact.

### P2.16 — Environment Setup for Frame Evaluation

**Problem:** `frame_eval_env` rebuilds the entire environment for every frame evaluation during scrubbing. For scenes with many actors, this involves thousands of HashMap insertions and String allocations.

**Current mitigations (applied):**
- Pre-size Environment HashMap capacity
- Reuse String buffer for property key formatting in `inject_property_into_env`
- Skip per-track property injection when no modifiers are present

**Remaining opportunity:** Use an `Arc<HashMap>` layer for the base environment so that `frame_eval_env` only needs to allocate the frame-specific overrides, not copy the ~90 base entries.

**Impact:** Medium. Would save ~200–300 μs per frame for scenes with 50+ actors.

---

### P2.17 — Procedural Plot Frame Environment Reuse

**Problem:** `render_actor_node` creates a brand-new `frame_eval_env` for every actor that has a `procedural_plot`. If 10 actors have plots, the environment is rebuilt 11 times per frame (once at the top level + once per actor).

**Fix:** Pass the top-level `frame_env` down through `evaluate_node` → `render_actor_node` and reuse it for plot sampling.

**Impact:** Medium–High for scenes with procedural plots. No-op for scenes without.

---

### P2.18 — Lazy Hit Region Calculation

**Problem:** World-space bounding boxes for click-to-select are computed for every actor on every frame, but they are only needed when the user clicks the canvas.

**Fix:** Defer hit region computation until `hit_regions()` is called (e.g., on mouse down in the GUI).

**Impact:** Low–Medium. Saves a few microseconds per actor per frame.

---

### P2.19 — Vello Scene Buffer Reuse

**Problem:** `Timeline::evaluate()` creates a brand new `vello::Scene` via `Scene::new()` on every frame. This allocates fresh encoding buffers.

**Fix:** Maintain a `Cell<Option<vello::Scene>>` (or similar) on Timeline and call `scene.reset()` between frames to reuse allocated buffers.

**Impact:** Low–Medium. Reduces allocator pressure during scrubbing.

---

### P2.20 — Text Path Clone on Cache Hit

**Problem:** `TextCompiler::compile()` clones the entire `Vec<TextPath>` on every cache hit. For text with many glyphs, this copies a large vector of `BezPath` objects.

**Fix:** Store cached text paths as `Arc<[TextPath]>` so cache hits are a single refcount increment.

**Impact:** Low–Medium. Significant for scenes with many text actors.

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

## Deferred / Blocked

None currently.
