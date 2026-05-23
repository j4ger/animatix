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

### P2.16 — Skip Frame Environment When Unused

**Problem:** `evaluate_with_debug` builds `frame_env` on every frame even when no modifiers or procedural plots exist. The environment is then completely unused by the rendering path.

**Measured cost:** For 200 static actors, ~1.6 ms per frame (98% of total evaluate time).

**Fix:** Gate `frame_env` creation behind a `needs_frame_env` check:
```rust
let needs_frame_env = has_modifiers || tracks.any(|t| t.procedural_plot.is_some());
```

**Impact:** **Very High.** 40× speedup for static scenes. Brings 200-actor evaluation from ~1.7 ms down to ~40 μs.

---

### P2.17 — Static Actor Scene Fragment Cache

**Problem:** Actors with no animated properties are re-evaluated and re-encoded into the vello scene on every frame. Property sampling, path building, and scene encoding are all repeated.

**Fix:** After first evaluation, cache a `vello::Scene` fragment containing all static actors. On subsequent frames, blit the cached fragment instead of re-encoding. Invalidate on timeline rebuild.

**Impact:** **Very High.** For scenes where 80% of actors are static (common), this removes 80% of rendering work. Combined with P2.16, a 1000-actor scene with 800 static actors could evaluate in < 200 μs.

---

### P2.18 — Actor-Level Temporal Coherence (Dirty Tracking)

**Problem:** When scrubbing forward at constant frame rate, most actors land between the same keyframe bracket for many consecutive frames. Yet all ~20 properties are re-sampled every frame.

**Fix:** Cache per-actor `NodeTransform` and sampled properties keyed by `(time_ms / bucket_size)`. Only re-sample if the time bucket changes or keyframes were modified.

**Impact:** **High.** During smooth playback, 90%+ of actors don't change between 60 fps frames. This would reduce per-actor cost from ~8 μs to ~1 μs for non-animating actors.

---

### P2.19 — Viewport Culling for Off-Screen Actors

**Problem:** Actors positioned outside the viewport are fully evaluated (property sampling, path building, scene encoding, hit regions) even though they contribute nothing visible.

**Benchmark finding:** 100 off-screen actors cost ~850 μs — same as 100 visible actors. The rendering pipeline does not know they are off-screen.

**Fix:** After computing `evaluate_node_transform`, derive a conservative world-space bounding box. If it does not intersect `[0, scene_width] × [0, scene_height]`, skip `render_actor_node` entirely (but still recurse children with the same visibility check).

**Complications:** Effects (shadow, glow) extend beyond bounds; children may be visible when parent is not. Use a margin equal to max effect radius + child extent.

**Impact:** **High for multi-scene and pannable scenes.** During slide transitions, the outgoing scene is often 90% off-screen. Culling it would eliminate most of that scene's evaluation cost. For single scenes with all-visible actors, no-op.

---

### P2.20 — PropertyTrack Memoization

**Problem:** `PropertyTrack::evaluate` performs a BTreeMap range lookup on every access, even when called with the same `time_ms` repeatedly.

**Fix:** Add a `(last_time_ms, last_value)` cache to each PropertyTrack. Return cached value if `time_ms == last_time_ms`.

**Impact:** **Medium.** Eliminates ~3 μs of BTreeMap overhead per property for repeated time samples. Helps during cache-hit playback and scrubbing.

---

### P2.21 — Parallel Actor Evaluation

**Problem:** The `for root in &self.root_nodes` loop in `evaluate_with_debug` is entirely serial.

**Fix:** Use `rayon` to evaluate independent root subtrees in parallel. Collect per-thread scene fragments and merge, or evaluate transforms in parallel then encode serially.

**Impact:** **Medium.** 2–4× on multi-core machines for actor-heavy scenes. Diminishing returns once memory bandwidth is saturated.

---

### P2.22 — Arc-Based Environment Layer

**Problem:** `frame_eval_env` still copies ~90 base environment entries (stdlib + colorscheme) into a new HashMap every frame.

**Fix:** Restructure `Environment` to hold an `Option<Arc<HashMap<String, Value>>>` base layer. `get()` checks overrides first, then falls back to the shared base. `frame_eval_env` only allocates overrides.

**Impact:** **Medium.** Saves ~200–300 μs per frame for large scenes by eliminating 90 HashMap insertions + Value clones.

---

### P2.23 — Procedural Plot Frame Environment Reuse

**Problem:** `render_actor_node` creates a brand-new `frame_eval_env` for every actor that has a `procedural_plot`. If 10 actors have plots, the environment is rebuilt 11 times per frame.

**Fix:** Pass the top-level `frame_env` down through `evaluate_node` → `render_actor_node` and reuse it for plot sampling.

**Impact:** **Medium for plot-heavy scenes.** No-op for scenes without procedural plots.

---

### P2.24 — Lazy Hit Region Calculation

**Problem:** World-space bounding boxes for click-to-select are computed for every actor on every frame, but they are only needed when the user clicks the canvas.

**Fix:** Defer hit region computation until `hit_regions()` is called (e.g., on mouse down in the GUI).

**Impact:** **Low–Medium.** Saves a few microseconds per actor per frame.

---

### P2.25 — Vello Scene Buffer Reuse

**Problem:** `Timeline::evaluate()` creates a brand new `vello::Scene` via `Scene::new()` on every frame. This allocates fresh encoding buffers.

**Fix:** Maintain a `Cell<Option<vello::Scene>>` on Timeline and call `scene.reset()` between frames to reuse allocated buffers.

**Impact:** **Low–Medium.** Reduces allocator pressure during scrubbing.

---

### P2.26 — Text Path Clone on Cache Hit

**Problem:** `TextCompiler::compile()` clones the entire `Vec<TextPath>` on every cache hit. For text with many glyphs, this copies a large vector of `BezPath` objects.

**Fix:** Store cached text paths as `Arc<[TextPath]>` so cache hits are a single refcount increment.

**Impact:** **Low–Medium.** Significant for scenes with many text actors.

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
