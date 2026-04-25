# Animatix Master Implementation Plan

> **Status: Active** — replaces previous `implementation_plan.md` and `architecture_refactor_plan.md`
>
> This is the single authoritative plan for all development work. Every chunk is scoped to be executed by a smaller, simpler LLM coding agent. Chunks are ordered by dependency; run sequentially within each part. Run `cargo build` and `cargo test` after every chunk.

---

## Part A — Foundational Performance Fixes (Critical Path)

### Chunk A1 — Binary-Search Keyframe Evaluation

**Purpose:** Replace O(n) linear keyframe scan in `PropertyTrack::evaluate()` with O(log n) binary search, as `BTreeMap` keys can be traversed via range. This is the single highest-leverage perf fix — every property of every track hits this path every frame.

**Target files:**
- `crates/animatix/src/timeline/track.rs`

**Specific changes:**
1. In `PropertyTrack::evaluate()` (line 224), replace the linear `for (&t, (val, easing)) in &self.keyframes` loop with:
   - On an empty map: return `default_value`
   - Find the first keyframe at or after `time_ms` using `self.keyframes.range(time_ms..)`
   - If no such keyframe exists (time is past all keyframes): return `last_value()`
   - If it's the first keyframe in the map: return its value directly
   - Otherwise, find the previous keyframe using `self.keyframes.range(..time_ms).next_back()`
   - Interpolate between prev and next using the next keyframe's easing

2. Do the same for `evaluate_paths_with_options()` (line 385) — it has the same linear scan pattern.

3. Keep the existing `default_value` initialization logic for time before first keyframe.

4. Do NOT change the storage from `BTreeMap` to `Vec` yet — that's a larger change for a separate chunk. The `BTreeMap::range()` API already supports O(log n) lookups.

**Verification:**
- `cargo test -p animatix` — all existing tests pass with identical behavior
- Specifically `timeline_tests` must produce identical interpolation results
- Run `cargo test timeline_tests -- --nocapture` and verify no regressions

---

### Chunk A2 — Wire Up IR Modifier Path in Hot Loop

**Purpose:** The modifier evaluation hot path in `scene_eval.rs:396-404` walks the raw `Vec<Stmt>` AST per frame. The IR compiler (`lower_modifier_ir`) and evaluator (`execute_modifier_ir`) already exist and are tested but are never called from the hot path. This chunk connects them.

**Target files:**
- `crates/animatix/src/timeline/mod.rs` — add new field to `Timeline`
- `crates/animatix/src/timeline/build.rs` — compile modifiers at build time
- `crates/animatix/src/timeline/scene_eval.rs` — use IR evaluator at frame time
- `crates/animatix/src/timeline/runtime.rs` — may need minor adaptation

**Specific changes:**
1. Add `modifier_programs: Vec<ModifierIrProgram>` field to `Timeline` struct in `mod.rs`

2. In `build.rs::build_with_diagnostics()` (around line 85), after processing all statements, compile `self.modifiers`:
   ```rust
   use crate::timeline::modifier_runtime::ir::lower_modifier_ir;
   for chunk in self.modifiers.chunks(1) {
       if let Ok(program) = lower_modifier_ir(chunk) {
           timeline.modifier_programs.push(program);
       }
       // Fall back to AST interpretation if lowering fails
   }
   ```

3. In `scene_eval.rs::evaluate_with_debug()` (line 396), replace:
   ```rust
   for modifier in &self.modifiers {
       self.apply_modifier_stmt(modifier, ...);
   }
   ```
   with:
   ```rust
   for program in &self.modifier_programs {
       let _ = self.apply_modifier_ir_program(program, ...);
   }
   ```

4. Preserve `apply_modifier_stmt` as a fallback for modifiers that couldn't be lowered (the existing diagnostic path).

**Verification:**
- `cargo test -p animatix` — all existing tests pass
- `cargo test ir_tests` — IR tests still pass
- Run `cargo run -- render examples/showcase.amx` — output matches pre-change
- Verify the GUI preview still works: `cargo run -p animatix-gui`

---

### Chunk A3 — Sparse PropertyTrack Storage in AnimationTrack

**Purpose:** `AnimationTrack` currently allocates 22 fixed `PropertyTrack<T>` fields regardless of actor type. A Circle only uses ~4 of them. Replace with a `HashMap<String, PropertyTrackValue>` so only used properties allocate storage.

**Target files:**
- `crates/animatix/src/timeline/track.rs` — restructure `AnimationTrack`
- `crates/animatix/src/timeline/build.rs` — update all track access sites (many)
- `crates/animatix/src/timeline/scene_eval.rs` — update evaluation
- `crates/animatix/src/timeline/runtime.rs` — update `inject_runtime_lookup_values`

**Specific changes:**
1. Define a new enum in `track.rs`:
   ```rust
   #[derive(Clone)]
   pub enum PropertyTrackValue {
       Float(PropertyTrack<f32>),
       Float2(PropertyTrack<[f32; 2]>),
       Float4(PropertyTrack<[f32; 4]>),
       U32(PropertyTrack<u32>),
       String(PropertyTrack<String>),
       TextPaths(PropertyTrack<Vec<TextPath>>),
       VelloPaths(PropertyTrack<Vec<VelloPath>>),
       Points(PropertyTrack<Vec<[f32; 2]>>),
       Image(PropertyTrack<Option<SceneImage>>),
       MorphOptions(PropertyTrack<MorphOptions>),
       PlacementMode(PropertyTrack<PlacementMode>),
       PositionBinding(PropertyTrack<PositionBinding>),
   }
   ```

2. Replace the 22 named fields in `AnimationTrack` with:
   ```rust
   pub struct AnimationTrack {
       pub label: String,
       pub properties: HashMap<String, PropertyTrackValue>,
       pub svg_paths: Vec<VelloPath>,  // static, not keyframed per-type
       pub first_seen_ms: u64,
   }
   ```

3. Add helper methods to `AnimationTrack` for common access patterns:
   ```rust
   impl AnimationTrack {
       pub fn get_float(&self, name: &str, time_ms: u64) -> f32 { ... }
       pub fn get_float2(&self, name: &str, time_ms: u64) -> Option<[f32; 2]> { ... }
       pub fn get_float4(&self, name: &str, time_ms: u64) -> Option<[f32; 4]> { ... }
       pub fn ensure_float(&mut self, name: &str, default: f32) { ... }
       pub fn ensure_float2(&mut self, name: &str, default: [f32; 2]) { ... }
       // ... similar for other types
       pub fn max_keyframe_time(&self) -> Option<u64>
   }
   ```

4. Update `max_keyframe_time()` to iterate `self.properties` instead of the hardcoded list of 22 field accesses.

5. Update `build.rs` — every place that does `track.position.add_keyframe(...)` becomes `track.ensure_float2("position", [0.0, 0.0])` then `track.set_float2("position", t, value, easing)`.

6. Update `scene_eval.rs::evaluate_node()` — replace direct field access with helper calls.

7. Update `runtime.rs::inject_runtime_lookup_values()` — iterate `properties` instead of accessing named fields.

**IMPORTANT:** This is the largest and most invasive chunk. Use the helper methods to minimize changes to `build.rs`. The build.rs file has ~70+ direct track field accesses — use ast-grep or search to find them all before starting.

**Verification:**
- `cargo test -p animatix` — all tests pass
- `cargo run -- render examples/showcase.amx` — identical output
- `cargo run -p animatix-gui` — preview works

**Dependencies:** Must run after A1 (binary search) since evaluation call patterns change.

---

### Chunk A4 — Frame Result Cache

**Purpose:** Add a simple frame result cache so paused preview doesn't re-evaluate everything. If the Timeline hasn't been rebuilt and the requested time matches the cached time, return the cached `vello::Scene`.

**Target files:**
- `crates/animatix/src/timeline/mod.rs` — add cache to Timeline
- `crates/animatix/src/timeline/scene_eval.rs` — cache check at top of evaluate

**Specific changes:**
1. Add to `Timeline` in `mod.rs`:
   ```rust
   pub struct Timeline {
       // ... existing fields ...
       frame_cache: Option<CachedFrame>,
   }

   struct CachedFrame {
       time_ms: u64,
       scene_dimensions: SceneDimensions,
       scene: vello::Scene,
       has_modifiers: bool,
       has_dynamic_layout: bool,
   }
   ```

2. Add `pub fn invalidate_cache(&mut self)` method to `Timeline`.

3. In `scene_eval.rs::evaluate_with_debug()`:
   - At the start, check cache: if `time_ms == cached.time_ms && dims == cached.dims && !self.has_modifiers() && !self.dynamic_layout`, return cloned cache
   - At the end, store result in cache
   - Call `invalidate_cache()` from build (which always creates a new Timeline anyway)

4. In `Timeline::build_with_diagnostics()`, ensure the timeline returned has an empty cache (it's new, so this is automatic).

**Verification:**
- `cargo test -p animatix` — all tests pass
- Manual test: open GUI, load a file, play, pause, verify frame doesn't re-evaluate (add a log or counter)
- `cargo run -- render examples/showcase.amx` — identical output

**Dependencies:** Must run after A3 (sparse tracks change the evaluate internals).

---

## Part B — Build-Time Refactoring

### Chunk B1 — Replace Raw Shape Constants with ShapeType Enum

**Purpose:** Replace the raw `u32` constants in `shapes.rs` (`SHAPE_RECT`, `SHAPE_CIRCLE`, etc.) with a proper `ShapeType` enum, then update all call sites. This eliminates magic numbers and enables exhaustive match checking.

**Target files:**
- `crates/animatix/src/timeline/shapes.rs` — define enum
- `crates/animatix/src/timeline/track.rs` — replace `u32` in `shape_type` track
- `crates/animatix/src/timeline/build.rs` — update `shape_type_for_actor()`
- `crates/animatix/src/timeline/scene_eval.rs` — update shape matching
- `crates/animatix/src/renderer/types.rs` — update `SdfInstance` (or remove later)

**Specific changes:**
1. Define in `shapes.rs`:
   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum ShapeType {
       Rect = 0,
       Circle = 1,
       Line = 2,
       Ellipse = 3,
       Arc = 4,
       Polygon = 5,
       Path = 6,
       Arrow = 7,
       Graph = 8,
       Plot = 9,
   }

   impl Interpolate for ShapeType { ... }  // discrete: t < 0.5 returns self
   ```

2. Replace all `u32` shape constants with `ShapeType` variants. The old `pub(crate) const SHAPE_RECT: u32 = 0;` lines become deleted.

3. Update `shape_type_for_actor()` to return `ShapeType` instead of `u32`.

4. Update all `matches!(shape_type, SHAPE_GRAPH | SHAPE_PLOT)` patterns to match on `ShapeType::Graph | ShapeType::Plot`.

5. For backward compatibility in `PropertyTrack<u32>`, either:
   - Change `shape_type: PropertyTrack<ShapeType>` (preferred, if A3 is done), or
   - Add `Into<u32>` and `From<u32>` impls for ShapeType

**Verification:**
- `cargo build -p animatix` — compiles
- `cargo test -p animatix` — all tests pass
- No remaining reference to `SHAPE_RECT`, `SHAPE_CIRCLE`, etc. (search for `SHAPE_` prefix)

**Dependencies:** If A3 is done first, easier (PropertyTrack already generic). If not, add cast helpers.

---

### Chunk B2 — Deduplicate Text/Math/Code Declaration Processing

**Purpose:** `build.rs:process_body()` has three nearly identical match arms for `Stmt::Text`, `Stmt::Math`, and `Stmt::Code` that all call `process_text_like_statement`. Further, `process_text_actor_decl` handles the same types but via `ActorDecl`. Consolidate into one shared `process_text_declaration()` function parameterized by content type.

**Target files:**
- `crates/animatix/src/timeline/build.rs` — refactor
- `crates/animatix/src/timeline/declarations_text.rs` — existing shared code

**Specific changes:**
1. In `build.rs`, replace the three match arms:
   ```rust
   Stmt::Text { .. } | Stmt::Math { .. } | Stmt::Code { .. } => {
       self.process_text_like_statement(stmt, time_ms, parent_label, diagnostics)
   }
   ```

2. Audit `declarations_text.rs` — check what's currently shared vs duplicated. The file already exists; ensure it's the single entry point.

3. Merge `process_text_actor_decl` into the same pipeline so `ActorDecl { ty: "Text" | "Math" | "Code", ... }` shares logic with the shorthand `Stmt::Text { ... }` forms.

4. The only difference between Text/Math/Code should be the `property_name` for content ("text" vs "math" vs "code") and the compiler function. Everything else (timing, color, position, track creation) is identical.

**Verification:**
- `cargo test -p animatix` — all text/math/code tests pass
- Run `cargo run -- render examples/showcase.amx` — text renders identically
- Check that `build.rs` line count has decreased (fewer duplicate branches)

---

### Chunk B3 — Extract Plot Sampling from process_body

**Purpose:** The plot sampling logic (CartesianPlot, PolarPlot, ParametricPlot, ImplicitPlot) occupies ~200 lines inside `process_body()`. Extract it into a dedicated function so the main loop is less monolithic.

**Target files:**
- `crates/animatix/src/timeline/build.rs` — extract to `process_plot_actor()`
- `crates/animatix/src/timeline/plot.rs` — already exists, may receive helpers

**Specific changes:**
1. Create `fn process_plot_actor(&mut self, label: &str, ty: &str, props: &[Property], time_ms: f64, parent_label: Option<&str>)` in `build.rs`.

2. Move the plot property parsing (x_domain, y_domain, t_domain, func, tolerance, max_depth, resolution) into this function.

3. Move the plot path generation (build_implicit_plot_path, sample_recursive_cartesian, etc.) into this function.

4. Replace the ~200 lines in `process_body` with a single call.

**Verification:**
- `cargo test -p animatix` — plot tests pass
- Run the plotting demos: `cargo run -- render examples/plotting_demo.amx` and `examples/math_demo.amx`
- Output PNGs match pre-change output

---

### Chunk B4 — Extract Shape Actor Processing from process_body

**Purpose:** Shape actor processing (Circle, Rect, Line, Arrow, Polygon, etc.) occupies ~600+ lines inside `process_body()`. Extract to `process_shape_actor()`, following the same pattern as B3.

**Target files:**
- `crates/animatix/src/timeline/build.rs` — extract to `process_shape_actor()`
- `crates/animatix/src/timeline/shapes.rs` — may receive helpers

**Specific changes:**
1. Create `fn process_shape_actor(&mut self, ...)` in `build.rs`.

2. Move property parsing (position, size, color, stroke, radius, etc.) — the big match block on `prop.name.as_str()` — into dedicated `parse_shape_properties()` and `apply_shape_defaults()` helpers.

3. Move the vector shape build logic and keyframe insertion.

4. The goal: `process_body()` becomes a dispatcher (~50 lines total) that routes to focused handlers.

**Verification:**
- `cargo build -p animatix` — compiles
- `cargo test -p animatix` — all tests pass
- `cargo run -- render examples/showcase.amx` — identical output
- Check `build.rs` line count: target < 400 lines (down from ~1342)

---

### Chunk B5 — Clean Asset-Loading Boundaries

**Purpose:** Asset loading (SVG parsing, image loading, font/text compilation) is currently inline in `build.rs` and `declarations_text.rs`. Extract into a dedicated `assets.rs` module with a clean `AssetCache` that can be shared.

**Target files:**
- `crates/animatix/src/timeline/assets.rs` — new file
- `crates/animatix/src/timeline/mod.rs` — add module
- `crates/animatix/src/timeline/build.rs` — delegate to assets
- `crates/animatix/src/timeline/svg.rs` — refactor
- `crates/animatix/src/timeline/image.rs` — refactor
- `crates/animatix/src/timeline/declarations_text.rs` — refactor

**Specific changes:**
1. Create `assets.rs` with:
   ```rust
   pub struct AssetCache {
       svg_paths: HashMap<String, Vec<VelloPath>>,
       images: HashMap<String, SceneImage>,
       text_glyphs: HashMap<String, Vec<TextPath>>,
   }
   ```

2. Move SVG path parsing from inline in build to `AssetCache::get_or_load_svg(path)`.

3. Move image loading to `AssetCache::get_or_load_image(path)`.

4. Move text/math compilation (Typst calls) to `AssetCache::get_or_compile_text(content, style)`.

5. The cache key should be the source content (path or text string) so identical content isn't recompiled.

**Verification:**
- `cargo test -p animatix` — all tests pass
- Run media examples: SVG and image rendering works identically
- Text/math in showcase renders identically

---

### Chunk B6 — Introduce ActorKind Trait

**Purpose:** Replace string-based actor type dispatch with a proper trait system. Each actor kind (Circle, Row, CartesianPlot, Text, etc.) implements `ActorKind` with `build()` and `render()` methods. This is the foundation for extensibility — adding a new primitive means implementing a trait, not editing a dispatcher function.

**Target files:**
- `crates/animatix/src/timeline/actor_kind.rs` — new file with trait definition
- `crates/animatix/src/timeline/mod.rs` — add module
- `crates/animatix/src/timeline/build.rs` — dispatch via trait
- Actor-specific files (may already exist or be new):
  - `crates/animatix/src/timeline/actors/shape.rs`
  - `crates/animatix/src/timeline/actors/text.rs`
  - `crates/animatix/src/timeline/actors/plot.rs`
  - `crates/animatix/src/timeline/actors/container.rs`
  - `crates/animatix/src/timeline/actors/media.rs`

**Specific changes:**
1. Define the trait:
   ```rust
   pub trait ActorKind: Send + Sync {
       fn name(&self) -> &'static str;
       fn build(&self, ctx: &mut ActorBuildContext, decl: &ActorDeclStmt) -> Result<()>;
       fn is_layout_container(&self) -> bool { false }
       fn is_plot(&self) -> bool { false }
       fn is_graph_host(&self) -> bool { false }
   }
   ```

2. Implement for each actor type. Most build logic already exists in the extracted functions from B3-B4. Move those functions into the trait impls.

3. Create a registry in `build.rs`:
   ```rust
   fn actor_registry() -> HashMap<&'static str, Box<dyn ActorKind>> {
       // Returns one instance per primitive type
   }
   ```

4. In `process_body()`, replace the massive string-match block with:
   ```rust
   if let Some(kind) = actor_registry().get(ty.as_str()) {
       kind.build(&mut ctx, &decl)?;
   }
   ```

**Verification:**
- `cargo test -p animatix` — all tests pass
- `cargo run -- render examples/showcase.amx` — identical output
- `cargo run -p animatix-gui` — preview works
- Key metric: `build.rs` is now a thin dispatcher

**Dependencies:** Must run after B3 and B4 (extracted processing already done).

---

## Part C — Runtime Evaluation Refactoring

### Chunk C1 — Lazy Environment Evaluation

**Purpose:** `inject_runtime_lookup_values()` eagerly evaluates every property of every track on every frame. Replace with lazy evaluation: only compute property values when they're actually queried by a modifier expression or runtime lookup.

**Target files:**
- `crates/animatix/src/timeline/runtime.rs` — restructure env building
- `crates/animatix/src/timeline/env.rs` — add lazy resolution support

**Specific changes:**
1. Add a `LazyEnvironment` wrapper around `Environment`:
   ```rust
   pub struct LazyEnvironment {
       base: Environment,
       timeline_ref: *const Timeline,  // or Rc<Timeline>
       time_ms: u64,
       overrides: HashMap<String, HashMap<String, Value>>,
       cached_properties: RefCell<HashMap<String, Value>>,
   }
   ```

2. Override `get()` to intercept lookups like `"ball.color"`:
   - Check overrides first
   - Check computed cache
   - If not found and it looks like a track property path, compute just that one value and cache it

3. Remove the eager `inject_runtime_lookup_values()` call from `frame_eval_env()`. The `t`, `scene_width`, `scene_height`, and anchor lookups should still be injected eagerly (they're constant per frame).

4. Remove the repeated `frame_eval_env()` call from `apply_modifier_stmt()`. Instead, only rebuild when the modifier actually changes overrides.

**Verification:**
- `cargo test -p animatix` — all tests pass
- Benchmark: for a simple scene with many actors but no `always` blocks, frame evaluation time should be measurably lower
- GUI preview: scrubbing feels snappier

**Dependencies:** Must run after A3 (sparse tracks change property access patterns).

---

### Chunk C2 — Unify Scene Node and AnimationTrack into Actor

**Purpose:** Currently `nodes: BTreeMap<String, SceneNode>` and `tracks: BTreeMap<String, AnimationTrack>` are separate maps keyed by label. Every operation requires dual lookups. `SceneNode` is anemic (just label + children). Merge into a unified `Actor` struct.

**Target files:**
- `crates/animatix/src/timeline/mod.rs` — new `Actor` struct, update `Timeline`
- `crates/animatix/src/timeline/track.rs` — `AnimationTrack` becomes `ActorTrack` (or gets merged)
- `crates/animatix/src/timeline/build.rs` — update node creation
- `crates/animatix/src/timeline/scene_eval.rs` — update traversal
- `crates/animatix-gui/src/document.rs` — update access patterns
- `crates/animatix-gui/src/app.rs` — update access patterns

**Specific changes:**
1. Define:
   ```rust
   pub struct Actor {
       pub id: ActorId,            // opaque id, replaces string label
       pub label: String,
       pub actor_type: String,     // "Circle", "Row", etc.
       pub track: AnimationTrack,  // property data
       pub parent: Option<ActorId>,
       pub children: Vec<ActorId>,
   }

   #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
   pub struct ActorId(u32);
   ```

2. Replace `nodes: BTreeMap<String, SceneNode>` and `tracks: BTreeMap<String, AnimationTrack>` with `actors: BTreeMap<ActorId, Actor>` and `root_actors: Vec<ActorId>`.

3. Add helper methods: `get_track(id)`, `get_children(id)`, `get_parent(id)`.

4. Update `scene_eval.rs::evaluate_node()` to take `ActorId` instead of `&str`.

5. Update GUI crate to use the new API. This may be the most work — trace all usages of `timeline.tracks`, `timeline.nodes`, `timeline.root_nodes`.

**Verification:**
- `cargo build -p animatix -p animatix-gui` — both compile
- `cargo test -p animatix` — all tests pass
- `cargo run -p animatix-gui` — GUI functions identically
- Run a full render cycle: `cargo run -- render examples/showcase.amx`

**Dependencies:** Must run after A3 (sparse tracks), B6 (actor kind trait), and C1 (lazy env). This is the single largest architectural change.

---

### Chunk C3 — Replace Rc\<RefCell\> with Arc + CoW for Environment

**Purpose:** The `Environment` uses `Rc<RefCell<Environment>>` for parent scoping, which is not `Send + Sync` and incurs runtime borrow-checking overhead. Replace with `Arc<HashMap<String, Value>>` and copy-on-write semantics.

**Target files:**
- `crates/animatix/src/timeline/env.rs` — rewrite storage
- `crates/animatix/src/timeline/runtime.rs` — update env construction
- `crates/animatix/src/timeline/build.rs` — update env usage
- Any file that calls `Environment::new()`, `Environment::child()`, or uses `Rc<RefCell<>>`

**Specific changes:**
1. Redesign `Environment`:
   ```rust
   #[derive(Clone)]
   pub struct Environment {
       values: Arc<HashMap<String, Value>>,
       parent: Option<Arc<Environment>>,
   }
   ```

2. `set()` creates a new `Arc<HashMap>` with the updated entry (CoW).

3. `get()` walks the parent chain, combining current scope + parent values.

4. `child()` creates a new Environment pointing to the current one as parent (shallow clone of Arc).

5. Remove all `Rc<RefCell<>>` usage throughout the codebase. Update `Environment::new()` to not return `Rc<RefCell<>>`.

6. Update `load_standard_library()` to work with the new API.

**Verification:**
- `cargo build -p animatix -p animatix-gui` — both compile
- `cargo test -p animatix` — all tests pass
- Verify no remaining `Rc<RefCell<Environment>>` in the codebase: `grep -r "Rc<RefCell" crates/`

**Dependencies:** Should run after C1 (lazy env already restructured some env usage). Can run before or after C2.

---

### Chunk C4 — Clean API Boundary Between Core and GUI

**Purpose:** The GUI crate directly accesses internal `Timeline` fields (`tracks`, `nodes`, `root_nodes`, `modifiers`). Create a clean public API surface on `Timeline` so internal refactoring doesn't break the GUI.

**Target files:**
- `crates/animatix/src/timeline/mod.rs` — add public API methods
- `crates/animatix-gui/src/app.rs` — use public API
- `crates/animatix-gui/src/document.rs` — use public API
- `crates/animatix-gui/src/preview_surface.rs` — use public API

**Specific changes:**
1. Add public methods to `Timeline`:
   ```rust
   impl Timeline {
       pub fn actor_labels(&self) -> Vec<&str> { ... }
       pub fn root_actor_labels(&self) -> Vec<&str> { ... }
       pub fn has_actor(&self, label: &str) -> bool { ... }
       pub fn track_for(&self, label: &str) -> Option<&AnimationTrack> { ... }
       pub fn background_color_at(&self, time_ms: u64) -> [f32; 4] { ... }
       pub fn keyframe_times(&self) -> Vec<f64> { ... }
       pub fn text_glyphs(&self) -> Vec<TextPath> { ... }
   }
   ```

2. Make the internal fields `pub(crate)` or private where possible.

3. In `animatix-gui`, replace all direct field access with the public API calls. Search for `timeline.tracks`, `timeline.nodes`, `timeline.root_nodes`, `timeline.modifiers`, `timeline.colorscheme`.

4. The `derive_runtime_context()` method in `document.rs` (or wherever it ends up) should be a method on `Timeline` in the core crate, not duplicated in the GUI.

**Verification:**
- `cargo build -p animatix -p animatix-gui` — both compile
- `cargo run -p animatix-gui` — all GUI functionality intact
- Search for direct field access from GUI crate: `grep -r "timeline\." crates/animatix-gui/src/`

**Dependencies:** Must run after C2 (unified actor changes the internal structure the GUI accesses).

---

### Chunk C5 — Remove Dead Code

**Purpose:** Remove vestigial types, unused rendering paths, and dead code accumulated from prior architectural iterations. This simplifies the codebase and reduces confusion for new contributors.

**Target files:**
- `crates/animatix/src/renderer/types.rs` — `SdfInstance`, `TextInstance`, `Vertex`, `INDICES`, `CameraUniform`
- Any files referencing the above
- Search for `#[allow(dead_code)]` and unused functions

**Specific changes:**
1. Remove `SdfInstance` struct — verified unused (only defined, never constructed or referenced).

2. Remove `Vertex`, `INDICES` — quad mesh for previous rendering path, replaced by Vello.

3. Remove `TextInstance` — replaced by Vello text path rendering.

4. Remove `CameraUniform` — unused in current rendering path.

5. Search for and remove unused imports exposed by these deletions.

6. Run `cargo check` to catch any remaining references. Fix or remove those too.

**Verification:**
- `cargo build -p animatix` — compiles cleanly, no warnings
- `cargo run -- render examples/showcase.amx` — identical output
- No `SdfInstance`, `TextInstance`, `Vertex`, `INDICES`, `CameraUniform` in codebase

---

## Part D — New Features

### Chunk D1 — Text/Media Property Assignment (Gap #1)

**Purpose:** Currently, changing text content or media source requires redeclaring the entire actor at a new keyframe. Implement `tagline.text = "New Text" [1s]` syntax so property assignments work for text/media, consistent with all other property assignments.

**Target files:**
- `crates/animatix/src/timeline/build.rs` — handle text/media property assignments
- `crates/animatix/src/timeline/assignments.rs` — extend assignment processing
- `crates/animatix/src/timeline/scene_eval.rs` — dynamic text re-compilation at render time
- `crates/animatix/src/timeline/declarations_text.rs` — add render-time text compilation

**Specific changes:**
1. In the assignment processing path (`process_assignment_statement` or its current location), when the target is a known text/math/code actor and the property is "text" (or "math", "code"), store the string value in the track.

2. In `scene_eval.rs::evaluate_node()`, when a text actor has a text track with keyframes, call the Typst compiler at render time (not just build time) to generate paths from the current string value.

3. Use the `AssetCache` (from B5) to avoid recompiling the same text content across frames or actors.

4. For media (Svg, Image), handle `icon.url = "new.svg"` similarly — update the source and trigger a re-load (again, cache via AssetCache).

**Verification:**
- Write a test `.amx` file: `tagline.text = "New Text" [1s]` works with proper interpolation/cross-fade
- Text morphing between different strings produces reasonable visual results
- `cargo test -p animatix` — new assignment test passes

**Dependencies:** B5 (AssetCache provides the text compilation cache infrastructure).

---

### Chunk D2 — Coordinate System Unification (Gap #2)

**Purpose:** Fix the friction between absolute coordinates (`at: (x, y)`), scene-relative anchors (`anchor: scene.center`), and layout-managed positioning. Allow containers to accept relative placement without breaking internal layout.

**Target files:**
- `crates/animatix/src/timeline/position.rs` — centralize position resolution
- `crates/animatix/src/timeline/layout.rs` — update layout contract
- `docs/layout_design.md` — update design doc
- `docs/dynamic_layout_design.md` — update or mark as merged

**Spec changes (design first, implement second):**
1. Define a unified positioning model:
   - Every actor has exactly one positioning mode at a given time
   - `position: absolute` → `at: (x, y)` in scene coordinates
   - `position: relative` → `at: (x%, y%)` relative to parent container
   - `position: anchor` → `anchor: scene.*` + `offset: (dx, dy)`
   - `position: layout` → managed by parent container

2. Update `PositionBinding` enum to reflect this model.

3. When a layout container child has an explicit `at`, treat it as an override of the container's placement for that child (already partially implemented).

4. Add percent-based positioning within containers: `at: (50%, 50%)` centers a child even inside a Row/Col.

**Verification:**
- Write tests for each positioning mode and combinations
- Existing showcase and all examples render identically (no regression)
- `cargo test -p animatix` — new position tests pass

---

### Chunk D3 — Parser Cleanup (Gap #3)

**Purpose:** Clean up parser leniency issues (trailing braces, inconsistent syntax) and audit examples for canonical syntax.

**Target files:**
- `crates/animatix/src/parser.rs` — tighten error handling
- `tree-sitter-animatix/grammar.js` — tighten grammar
- `examples/` — audit all `.amx` files

**Specific changes:**
1. In the Chumsky parser, add proper error recovery for trailing braces on inline items instead of silently ignoring them. Either reject with a clear error message, or accept with a warning diagnostic.

2. In the Tree-sitter grammar, ensure the grammar definition matches what the parser actually accepts.

3. Audit every `examples/*.amx` file:
   - Remove any trailing braces that are currently silently accepted
   - Ensure consistent indentation and style
   - Verify each example parses cleanly with no warnings

4. Add a CI check: `cargo run -- check examples/` that parses all examples and reports any diagnostics.

**Verification:**
- All examples parse cleanly: `for f in examples/*.amx; do cargo run -- ast "$f" > /dev/null; done`
- Parser tests still pass: `cargo test parser_tests`
- Tree-sitter tests still pass: `cd tree-sitter-animatix && tree-sitter test`

---

### Chunk D4 — Diagnostic UX Improvements

**Purpose:** Improve the diagnostic feedback in both CLI and GUI. Better error messages, clearer phase attribution, and actionable suggestions.

**Target files:**
- `crates/animatix/src/diagnostics.rs` — enhance formatting
- `crates/animatix-gui/src/app.rs` — display diagnostics in GUI
- `crates/animatix/src/timeline/build.rs` — improve diagnostic quality
- `crates/animatix/src/parser.rs` — improve parse error messages

**Specific changes:**
1. Add source location (line/column) to diagnostics where possible. The `Span` struct exists in `ast.rs` but isn't populated; start populating it during parsing.

2. Format diagnostics in the CLI with:
   - File path and line number
   - Source context (the offending line)
   - Colored output (error = red, warning = yellow)
   - Suggestion for fixes (e.g., "Did you mean `color`?")

3. In the GUI, add a diagnostics panel (bottom of editor or separate tab) that:
   - Lists all diagnostics with severity filtering
   - Clicking a diagnostic scrolls the editor to the relevant line
   - Shows a summary count (N errors, M warnings)

4. Record phase information on each diagnostic and display it (`[build]`, `[runtime]`, `[parse]`).

**Verification:**
- `cargo test -p animatix` — diagnostic tests pass
- Manual: introduce an error in an `.amx` file, verify the CLI shows clear error with location
- Manual: open the GUI with an error, verify diagnostic panel shows it

---

### Chunk D5 — Extended Authoring Surface

**Purpose:** Ship the remaining practical surface enhancements requested in the Roadmap Phase 6. These are smaller, independent features that improve the authoring experience.

**Target files:** Various — each sub-feature is independent.

**Sub-features (each can be its own chunk):**

**D5a — Animated container properties:** Allow `gap`, `align`, `cols` on containers to be animated: `row.gap = 20 [1s]`. Currently these are static at declaration time. Implement by storing them in the track and reading from track at render time.

**D5b — Text property completion:** Allow `text` property on `Text` actors to reference runtime variables: `label.text = format("Value: {}", t)` where `t` changes per frame.

**D5c — More easing curves:** Add `ease-out`, `ease-in-out`, `bounce`, `elastic` to the easing system. These are simple additions to `easing.rs`.

**D5d — Action chaining:** Support `then` keyword for sequential actions: `fade-in btn [1s] then move btn to (100, 0) [1s]`. Implement as AST sugar that expands to two keyframed actions.

**D5e — Better examples:** Create a "getting started" tutorial `.amx` file, improve inline comments, add a `tutorial/` directory.

**Verification:** Per sub-feature: write an example `.amx` file, render it, verify visually.

---

## Execution Order Summary

```
Part A (Performance):  A1 → A2 → A3 → A4
Part B (Build-Time):   B1 → B2 → B3 → B4 → B5 → B6
Part C (Runtime):      C1 → C2 → C3 → C4 → C5
Part D (Features):     D1 → D2 → D3 → D4 → D5(a-e)
```

**Parallelizable:** B1 and B2 are independent. D3, D4, and D5 sub-features are independent. Otherwise, run sequentially.

**Critical path:** A1→A2→A3→A4→B1→B5→C1→C2→C4→D1 is the minimum chain that must be sequential.

**Total estimated chunks:** 20 (A:4, B:6, C:5, D:5 including sub-features).

---

## Verification Checklist (After Every Chunk)

```bash
cargo build -p animatix
cargo test -p animatix
cargo build -p animatix-gui                # after chunks that touch API surface
cargo run -- render examples/showcase.amx   # visual regression check
```

After Part C is complete, add:
```bash
cargo clippy -- -D warnings                # no warnings allowed
cargo test --all                           # all crates
```
