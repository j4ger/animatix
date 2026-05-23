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

> Findings from oracle audit of all crates. Ordered by impact for high-actor-count scenes.

---

### ✅ P2.16 — Skip Frame Environment When Unused

**Status:** Complete. Gated `frame_env` creation behind `needs_frame_env()`. Also fixed build process to skip compiling empty modifier programs.

**Impact:** 40× speedup for static scenes.

---

### ✅ P2.17 — Static Actor Scene Fragment Cache

**Status:** Complete. `PropertyTrack::is_effectively_static()` detects tracks with 0 keyframes or 1 keyframe at time 0. Fully-static subtrees cache their `vello::Scene` encoding after first evaluate; subsequent frames append directly.

**Impact:** 97% faster for static scenes (200 actors: 1.7 ms → 64 μs).

---

### ✅ P2.18 — Actor-Level Temporal Coherence

**Status:** Complete. Per-actor transform cache keyed by `(time_ms, parent_transform_coeffs)` avoids re-sampling ~8 properties on repeated evaluations.

**Impact:** High for scrubbing back-and-forth over same frames.

---

### ✅ P2.19 — Viewport Culling for Off-Screen Actors

**Status:** Complete. Conservative world-space bounding box check after `evaluate_node_transform`. Skips rendering, effects, and hit regions for off-screen actors while still recursing children.

**Impact:** High for multi-scene transitions and pannable scenes.

---

### ✅ P2.20 — PropertyTrack Memoization

**Status:** Complete. Added `last_evaluated: RefCell<Option<(u64, T)>>` to `PropertyTrack`. `evaluate()` returns cached value when called with the same `time_ms` repeatedly, skipping BTreeMap lookups.

**Impact:** Medium. Eliminates ~3 μs of BTreeMap overhead per property for repeated time samples.

---

### ⏸️ P2.21 — Parallel Actor Evaluation

**Status:** Deferred. Attempted with rayon but `Timeline` contains `RefCell`/`Cell` fields (text_compiler, caches) that make it non-Sync. Correct implementation requires either:
- Restructuring Timeline to be Send+Sync (large refactor), or
- Cloning per-thread data (expensive for large timelines)

**Impact:** Would be 2–4× on multi-core for multi-root scenes. Not justified at current complexity.

---

### ✅ P2.22 — Arc-Based Environment Layer

**Status:** Complete. `Environment` now holds `overrides: HashMap<String, Value>` and `base: Option<Arc<HashMap<String, Value>>>`. `get()` checks overrides first, then falls back to the shared base.

`frame_eval_env` creates `Environment::with_base(Arc::clone(&self.env_base))` instead of copying ~90 stdlib entries. The base Arc is frozen at the end of `Timeline::build`.

**Impact:** Medium. Saves ~200–300 μs per frame for large scenes with modifiers.

---

### ✅ P2.23 — Procedural Plot Frame Environment Reuse

**Status:** Complete (done as part of P2.16). Top-level `frame_env` is now passed down through `evaluate_node` → `render_actor_node` and reused for plot sampling instead of creating a new env per actor.

---

### ✅ P2.24 — Lazy Hit Region Calculation

**Status:** Complete. Added `compute_hit_regions: bool` to `DebugRenderOptions`. Hit regions are only computed when this flag is true. The GUI sets it to true since it needs click-to-select data. Benchmarks and exports skip it.

**Impact:** Low–Medium. Saves bounding-box computation for frames where click-to-select is not needed.

---

### ✅ P2.25 — Vello Scene Buffer Reuse

**Status:** Complete. Added `scene_buffer: RefCell<Option<vello::Scene>>` to Timeline. `evaluate_with_debug` calls `scene.reset()` on the reused buffer instead of `Scene::new()` on every frame.

**Impact:** Low–Medium. Reduces allocator pressure during scrubbing.

---

### ✅ P2.26 — Text Path Clone on Cache Hit

**Status:** Complete. `TextCompiler` cache now stores `Arc<[TextPath]>` instead of `Vec<TextPath>`. Cache hits return `Arc::clone()` — a single refcount increment instead of cloning all `BezPath` objects.

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
