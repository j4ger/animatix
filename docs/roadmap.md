# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 architecture first — everything above it collapses if the foundation is shaky.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge.

---

## P2 — Performance Optimizations (Complete)

All P2 items are implemented. Ordered by impact for high-actor-count scenes.

| Item | What it does | Impact |
|------|-------------|--------|
| **P2.16** | Skip `frame_env` creation when no modifiers/procedural plots exist | 40× speedup for static scenes |
| **P2.17** | Cache `vello::Scene` encoding for fully-static subtrees after first evaluate | 97% faster for static scenes |
| **P2.18** | Per-actor transform cache keyed by `(time_ms, parent_transform_coeffs)` | High for scrubbing back-and-forth |
| **P2.19** | Viewport culling — skip rendering for off-screen actors | High for multi-scene transitions |
| **P2.20** | PropertyTrack memoization — cache `evaluate()` result by `time_ms` | Medium, eliminates BTreeMap overhead |
| **P2.22** | Arc-based environment layer — share base env via `Arc` instead of copying | Medium, saves ~200–300 μs per frame |
| **P2.23** | Reuse top-level `frame_env` for procedural plots (was per-actor) | Medium for plot-heavy scenes |
| **P2.24** | Lazy hit region calculation — compute only when `compute_hit_regions: true` | Low–Medium |
| **P2.25** | Reuse `vello::Scene` buffer via `scene.reset()` instead of `Scene::new()` | Low–Medium, reduces allocator pressure |
| **P2.26** | TextCompiler cache stores `Arc<[TextPath]>` instead of `Vec<TextPath>` | Low–Medium for text-heavy scenes |

**Measured:** 200 static actors evaluate in ~64 μs (was ~1.7 ms). 200 animated actors evaluate in ~188 μs.

**Not pursued:** Parallel actor evaluation (P2.21) — Timeline contains `RefCell`/`Cell` fields making it non-Sync. Restructuring for thread safety is a large refactor with marginal returns given current performance is already well within 60 fps budgets.

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
