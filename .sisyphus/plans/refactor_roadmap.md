# Internal Refactor Roadmap

This document is the cross-session execution plan for the internal Animatix refactor. It is intentionally scoped to architecture, ownership, naming, and internal abstraction cleanup.

It does **not** authorize grammar changes, shipped feature changes, or runtime-surface expansion.

---

## 1. Purpose

Use this document to coordinate a two-phase refactor that:

1. finishes cleaning up the remaining structural issues in the project, and then
2. migrates the runtime toward a trait-object primitive system.

This plan exists because the repository now has a coherent runtime direction but still carries structural drift in how that direction is represented in source layout and internal naming.

---

## 2. Non-Negotiable Invariants

These rules apply to **every** slice in both phases.

- **No exposed grammar changes.**
- **No shipped feature changes.**
- **No spec drift.** If behavior does not change, docs must not imply new behavior.
- **No big-bang rewrite.** Every slice must be reviewable, revertible, and parity-checked.
- **Preserve random-access evaluation.** Preview, scrubbing, image export, and video export must keep working as they do today.
- **Pause when unsure.** If a boundary decision would materially affect later slices, stop and resolve it before continuing.

---

## 3. Current Structural Problems This Plan Must Solve

The current project shape is workable, but several internal seams are still wrong or overly broad.

### Core runtime

- `crates/animatix/src/timeline/mod.rs` owns too much of the build-time and frame-time story.
- primitive handling is still driven by broad string/type branching and wide shared track mutation
- `crates/animatix/src/ir.rs` lives above its natural ownership boundary even though it serves the runtime/timeline pipeline
- renderer-facing types and timeline-owned responsibilities are not yet cleanly separated
- naming has drifted in ways that make ownership harder to reason about (`KurboShape_`, vague modules like `lookup`, overlapping `render` terminology)

### GUI

- `crates/animatix-gui/src/app.rs` is still a responsibility monolith

### Architectural mismatch

- the docs describe a unified vector-first pipeline, but the source tree still exposes mixed responsibilities across timeline, render, media, layout, and primitive handling

This roadmap is successful only when those structural mismatches are resolved without changing the language contract.

---

## 4. Strategy Overview

The refactor is split into two phases with a strict seam between them.

### Phase 1

Make the current architecture honest.

That means fixing ownership, file organization, naming, and internal boundaries **without** introducing trait-object primitive dispatch yet.

### Phase 2

Migrate to a trait-object primitive system only after primitive families and capabilities are explicit and the old god-module structure has been reduced.

### Why this split exists

The current codebase does not primarily suffer from “missing traits.” It suffers from wide ownership, mixed boundaries, and centralized branching. If trait objects are introduced before those seams are made explicit, the result is likely to be indirection without simplification.

---

## 5. Cross-Session Status Ledger

Keep this section current during execution.

- **Current phase:** Phase 2 preparation
- **Active slice:** P2-S1 planning — vector-shape trait surface
- **Last completed checkpoint:** P1-C7 — primitive family/capability seam landed
- **Open questions:**
  - Keep the initial vector-shape trait seam in `timeline::primitive.rs`, or split that file into a deeper subtree only after the pilot proves worthwhile?
- **Blocked items:** None
- **Next safe midpoint commit:** `refactor: record phase 2 vector-shape pilot plan`

When work starts, update this ledger before and after each slice.

---

## 6. Phase 1 — Structural Cleanup

### 6.1 Goal

Solve the remaining structural issues in the repository while keeping user-visible behavior unchanged.

### 6.2 Required outcomes

By the end of Phase 1:

- `timeline/mod.rs` no longer acts as the dominant implementation sink for unrelated concerns
- modifier IR/VM ownership is placed under an honest internal boundary
- renderer-facing types are owned at the correct layer
- GUI shell/runtime/workspace/editor concerns are no longer concentrated in a single large file
- naming is normalized around the new boundaries
- primitive families and primitive capabilities exist as internal structure, but not yet as trait-object runtime dispatch

### 6.3 Phase 1 slices

#### Slice 1 — Boundary map and naming freeze

Create and agree on the target ownership map before moving code.

Includes:

- identify what remains owned by timeline build, timeline evaluate, renderer, GUI shell, and modifier runtime
- define canonical names for primitive families, capabilities, render-facing types, and timing/environment concepts
- freeze naming targets before large moves or renames

Exit criteria:

- a target boundary map exists in this document or a linked implementation note
- the canonical naming vocabulary is explicit

QA scenario:

- **Tooling:** document review + targeted code reads
- **Steps:**
  1. confirm the target ownership map names concrete module homes for timeline build, timeline evaluate, modifier runtime, renderer-facing payloads, and GUI shell/runtime concerns
  2. confirm the naming vocabulary resolves current ambiguities called out in this roadmap
  3. confirm no source files were changed in ways that alter runtime behavior
- **Expected result:** a developer can name the intended home of each major concern before code motion begins, and no implementation ambiguity remains for Slice 2

#### Slice 2 — Decompose timeline build ownership

Reduce `crates/animatix/src/timeline/mod.rs` by extracting build-time responsibilities into narrower internal modules.

Expected focus:

- body processing
- declaration lowering
- assignment lowering
- plot/shape/media-specific lowering helpers
- shared track patch/keyframe insertion helpers

Guardrails:

- do not change build semantics
- do not mix structural extraction with new behavior

Exit criteria:

- `timeline/mod.rs` is materially smaller and more honest about what it owns
- extracted modules have clear ownership boundaries

QA scenario:

- **Tooling:** `cargo test`, `cargo run -- ast`, targeted image parity checks
- **Steps:**
  1. run `cargo test`
  2. run `cargo run -- ast examples/showcase.amx`
  3. run `cargo run -- image examples/showcase.amx --time 0.0 --output /tmp/phase1_slice2_frame0.png`
  4. run `cargo run -- image examples/showcase.amx --time 1.5 --output /tmp/phase1_slice2_frame1.png`
  5. inspect the extracted module layout and verify the old `mod.rs` responsibilities were actually reduced rather than just moved around indirectly
- **Expected result:** tests pass, AST output still succeeds, rendered frames remain visually equivalent, and timeline build concerns are visibly more isolated than before

#### Slice 3 — Re-home modifier IR and VM ownership

Move modifier compilation/execution infrastructure under a more honest internal boundary.

Expected focus:

- relocate `ir.rs` under a timeline/runtime-oriented internal area
- clarify VM placement relative to modifier execution
- remove duplicate or drifting expression-evaluation helpers when safe

Guardrails:

- do not broaden IR scope
- do not change supported modifier/runtime semantics

Exit criteria:

- IR/VM placement matches actual ownership
- modifier/runtime parity remains intact

QA scenario:

- **Tooling:** `cargo test`, targeted runtime frame checks, focused code review
- **Steps:**
  1. run `cargo test`
  2. run `cargo run -- image examples/reactive_runtime.amx --time 0.5 --output /tmp/phase1_slice3_reactive_a.png`
  3. run `cargo run -- image examples/reactive_runtime.amx --time 1.5 --output /tmp/phase1_slice3_reactive_b.png`
  4. confirm modifier/runtime code paths no longer depend on the old misplaced module ownership
  5. verify no supported modifier semantics changed during the move
- **Expected result:** modifier-driven scenes still render equivalently, and IR/VM ownership is cleaner without semantic drift

#### Slice 4 — Fix renderer/timeline ownership seams

Clarify which types are renderer-facing outputs and which are timeline-owned intermediates.

Expected focus:

- move or rename render-facing path/image payload types to the layer that actually owns them
- reduce renderer dependence on timeline internals
- keep language semantics in timeline/runtime, not renderer backends

Exit criteria:

- renderer uses stable render-facing types without depending on timeline internals more than necessary

QA scenario:

- **Tooling:** `cargo test`, targeted render commands, import-boundary code review
- **Steps:**
  1. run `cargo test`
  2. run `cargo run -- image examples/showcase.amx --time 0.0 --output /tmp/phase1_slice4_render.png`
  3. inspect renderer modules and confirm render-facing payload types are owned at the intended layer
  4. verify renderer code no longer reaches into timeline internals beyond the agreed boundary
- **Expected result:** render commands still work, and renderer/timeline ownership is easier to explain from file structure and imports alone

#### Slice 5 — Split GUI monolith ownership

Break up `crates/animatix-gui/src/app.rs` by responsibility.

Expected focus:

- app shell
- workspace/file tree
- editor/document state
- preview/runtime wiring
- UI panels and transport controls

Guardrails:

- no GUI behavior redesign in this phase
- no editor workflow expansion in this phase

Exit criteria:

- GUI responsibilities are split into honest modules
- user-visible GUI behavior remains equivalent

QA scenario:

- **Tooling:** `cargo test`, GUI launch, manual smoke test
- **Steps:**
  1. run `cargo test`
  2. launch the GUI entrypoint the project currently uses
  3. open a representative example file, verify the workspace/file tree appears, verify editor content loads, and verify preview wiring still functions
  4. confirm shell/runtime/workspace/editor concerns now live in separate modules instead of one monolith
- **Expected result:** the GUI still opens and performs the same basic workflow, while the source layout reflects separated responsibilities

#### Slice 6 — Naming and module normalization

After boundaries are stabilized, normalize naming to match the new ownership layout.

Examples that should be revisited:

- `KurboShape_`
- vague modules like `lookup`
- overloaded `render` naming
- inconsistent environment/time naming (`frame_env`, `eval_env`, mixed `*_ms` representation)

Guardrails:

- rename only after structural intent is clear
- avoid broad cosmetic renames that do not reinforce a boundary

Exit criteria:

- naming reflects ownership and reduces ambiguity
- rename churn is bounded and justified

QA scenario:

- **Tooling:** `cargo test`, targeted code review, grep-based sanity checks
- **Steps:**
  1. run `cargo test`
  2. confirm renamed types/modules now follow the canonical vocabulary from Slice 1
  3. grep for intentionally replaced ambiguous names and confirm only justified legacy references remain
  4. verify rename churn did not accidentally widen into feature work
- **Expected result:** the codebase uses clearer names that match ownership boundaries, with no behavior change and no unnecessary rename blast radius

#### Slice 7 — Introduce primitive families and capabilities

Create the internal seam that Phase 2 will depend on.

This slice should **not** introduce trait-object dispatch yet.

Define a stable internal family model, likely something close to:

- text-like
- vector-shape
- media/image
- plot/graph
- layout/container
- grouping

Define internal capabilities, such as:

- path generation
- text path generation
- image payload support
- layout size reporting
- action target support
- morph support
- runtime lookup/property exposure

Exit criteria:

- primitive families are explicit in internal code
- capability checks exist without depending on broad ad hoc branching
- Phase 2 can begin without first restructuring the project again

QA scenario:

- **Tooling:** `cargo test`, AST/image parity checks, focused internal API review
- **Steps:**
  1. run `cargo test`
  2. run `cargo run -- ast examples/showcase.amx`
  3. run `cargo run -- image examples/showcase.amx --time 1.5 --output /tmp/phase1_slice7_capabilities.png`
  4. inspect the new family/capability layer and confirm it expresses current primitive distinctions without introducing trait-object dispatch yet
  5. confirm broad ad hoc branching has been reduced in the targeted internal paths
- **Expected result:** internal primitive structure is explicit and reusable, current behavior is unchanged, and Phase 2 has a real seam to build on

### 6.4 Phase 1 checkpoints

- **P1-C1:** boundary map and naming vocabulary agreed — **done**
- **P1-C2:** timeline build ownership extracted from `mod.rs` — **done** via `timeline/build.rs`
- **P1-C3:** IR/VM placement made honest — **done** via `timeline/modifier_runtime/` with crate-root compatibility shims
- **P1-C4:** renderer/timeline boundary clarified — **done** by moving render-facing path payloads to `renderer::types` while preserving compatibility re-exports
- **P1-C5:** GUI monolith split by responsibility — **done** via `app/runtime.rs`, `app/workspace.rs`, and `app/persistence.rs`
- **P1-C6:** naming normalized around final boundaries — **done** for the high-value targets: `KurboShape`, `property_lookup`, `scene_eval`, and `current_build_time_ms`
- **P1-C7:** primitive family/capability seam landed — **done** via `timeline/primitive.rs`

### 6.5 Phase 1 exit criteria

Phase 1 is done only when:

- structural issues called out in this plan are resolved or explicitly deferred with reasons
- public grammar/features remain unchanged
- spec-facing docs do not imply new behavior
- examples and validation commands still prove parity
- the repository is ready for a narrow trait-object migration pilot instead of another architectural cleanup pass

Completion note:

- Phase 1 has been implemented. The remaining work belongs to Phase 2 unless a later review finds a regression.
- The exposed grammar and shipped feature surface were intentionally left unchanged.
- `cargo test` passed after the Phase 1 implementation.

---

## 7. Midpoint Commit Strategy

Commits are acceptable during execution, but they must be narrow and sane.

### Commit rules

- commit only at stable parity points
- each commit should be reviewable and revertible
- behavior-changing refactor commits must land with their parity checks
- avoid combining structural moves, naming churn, and behavioral cleanup in one commit unless the changes are inseparable

### Recommended midpoint commit shapes

1. `refactor: document target ownership boundaries for runtime cleanup`
2. `refactor: extract timeline build helpers from mod`
3. `refactor: relocate modifier ir and vm under runtime internals`
4. `refactor: separate renderer-facing payload types from timeline internals`
5. `refactor: split gui shell responsibilities`
6. `refactor: normalize runtime naming around primitive boundaries`
7. `refactor: introduce primitive family and capability descriptors`

If a slice does not yet have clean parity evidence, do **not** commit it.

---

## 8. Phase 2 — Trait-Object Primitive System

### 8.1 Goal

Replace centralized primitive branching with primitive-owned behavior behind a trait-object system, while preserving the current grammar, features, and runtime semantics.

### 8.2 Required outcomes

By the end of Phase 2:

- at least one primitive family has been migrated away from broad central dispatch to trait-backed internal dispatch
- the capability seam from Phase 1 remains the organizing abstraction
- broader rollout is guided by measured complexity, not ideology

### 8.3 Migration principles

- start with one low-risk family first
- prove parity before broadening the migration
- keep trait-object boundaries internal
- do not use Phase 2 to redesign layout semantics, plotting semantics, or public action/property coverage

### 8.4 Recommended pilot family

Start with the **vector-shape** family.

Why:

- current shape handling already passes through a narrower geometry/path generation story
- the family has less special-case external integration than text-like primitives
- it is a lower-risk proving ground than text, math, code, plots, or containers

### 8.4.1 Phase 2 start package

The current repository state suggests that the vector-shape pilot should reduce **two parallel primitive dispatch systems** rather than introducing a trait surface on top of both.

Current hotspots to shrink first:

- `timeline/build.rs` still contains the largest concentration of raw `ty == "..."` shape branching
- `timeline/shapes.rs` still maps actor type strings into `shape_type` integers through `shape_type_for_actor(...)`
- `timeline/scene_eval.rs` and `timeline/assignments.rs` still branch on `shape_type` to decide whether to rebuild or reuse vector paths
- `timeline/runtime.rs` still contains an Arrow-only `shape_type` special case

The pilot should stay explicitly scoped to vector-shape primitives only:

- `Rect`, `Square`
- `Circle`, `Dot`
- `Line`, `Arrow`
- `Ellipse`, `Arc`
- `Polygon`, `RegularPolygon`
- `Path`

The pilot should **not** touch these families unless the vector-shape pilot proves itself first:

- text-like (`Text`, `Math`, `Code`)
- media (`Svg`, `Image`)
- plot/graph
- containers/grouping

Recommended starting files for the pilot:

- `crates/animatix/src/timeline/primitive.rs`
- `crates/animatix/src/timeline/shapes.rs`
- `crates/animatix/src/timeline/build.rs`
- `crates/animatix/src/timeline/scene_eval.rs`
- `crates/animatix/src/timeline/assignments.rs`
- `crates/animatix/src/timeline/runtime.rs`

Recommended baseline parity set before implementation:

- `examples/primitive_breadth_demo.amx`
- `examples/arrow_demo.amx`
- `examples/line_and_ellipse_demo.amx`
- `examples/arc_polygon_path_demo.amx`
- `examples/shape_morph_demo.amx`

### 8.4.2 Minimal pilot trait boundary

The initial trait surface should stay internal and vector-shape-specific.

Recommended shape of the seam:

- classification still begins from `PrimitiveDescriptor`
- vector-shape routing resolves to an internal vector-shape primitive implementation
- the trait should model only real vector-shape responsibilities:
  - apply shape-specific geometry properties into shared vector-shape state
  - build vector paths from sampled vector-shape state
  - rebuild/restyle vector paths in the redraw/assignment paths

The pilot should **not** use the trait seam to introduce:

- a universal primitive API
- plugin/extensibility semantics
- layout behavior
- plot sampling behavior
- text/media ownership
- public property or grammar changes

Important restraint:

- keep `AnimationTrack.shape_type` during the pilot as an internal bridge
- reduce behavior branching first
- only consider removing or shrinking `shape_type` after the pilot proves its value

### 8.5 Suggested Phase 2 slices

#### Slice 1 — Define the trait-object surface

Create the internal trait surface that corresponds to the Phase 1 family/capability model.

Possible responsibilities to model:

- lower/build-time primitive contribution
- runtime evaluation contribution
- render payload production
- capability reporting

Guardrails:

- keep the trait surface minimal
- do not commit to extension/plugin semantics

Execution note:

- define the trait/registry/shared vector-shape state first, but do **not** route build/runtime behavior through it yet
- keep the first landing small enough to prove the seam without changing dispatch behavior

QA scenario:

- **Tooling:** design review + compile/test pass
- **Steps:**
  1. review the proposed trait surface against the Phase 1 capability model
  2. confirm each trait responsibility corresponds to a real existing primitive concern rather than a hypothetical extension point
  3. run `cargo test`
- **Expected result:** the trait surface is small, internal, and grounded in already-proven capabilities rather than speculative abstraction

#### Slice 2 — Migrate the pilot family

Migrate vector-shape primitives to the new trait-backed dispatch path while preserving parity.

Exit criteria:

- pilot family no longer depends primarily on broad central branching
- parity checks pass

Execution note:

- migrate `build.rs` first because it contains the densest actor-type branching
- migrate `scene_eval.rs` and `assignments.rs` second because they duplicate the same `shape_type` path-selection logic
- migrate the Arrow-only `runtime.rs` special case last unless it blocks parity

QA scenario:

- **Tooling:** `cargo test`, targeted image/video parity checks, focused code review
- **Steps:**
  1. run `cargo test`
  2. render representative vector-shape scenes at multiple times with `cargo run -- image ...`
  3. if motion or transition behavior is involved, run `cargo run -- video examples/showcase.amx --output /tmp/phase2_slice2_pilot.mp4 --fps 30`
  4. inspect the migrated family and confirm behavior now flows primarily through trait-backed dispatch rather than the old central branch path
- **Expected result:** vector-shape primitives preserve visible/runtime parity while central dispatch code for that family shrinks materially

#### Slice 3 — Evaluate the result before broadening

Do not automatically migrate other families.

At this checkpoint, decide whether the trait-object design is clearly paying for itself.

Questions to answer:

- did central branching shrink meaningfully?
- did ownership get clearer?
- did testing/debugging stay tractable?
- did the trait surface remain honest, or is it forcing unrelated primitive families into a false abstraction?

If the answer is mixed, pause and reassess instead of continuing on momentum.

QA scenario:

- **Tooling:** checkpoint review + parity evidence review
- **Steps:**
  1. compare the pre-pilot and post-pilot code paths for the migrated family
  2. review collected parity evidence from tests and render checks
  3. explicitly answer the checkpoint questions in this section in writing
  4. decide go / pause / rollback for broader rollout
- **Expected result:** the decision to continue is evidence-based rather than momentum-based

#### Slice 4 — Incremental family rollout

Only after a successful pilot, migrate additional families one by one.

Recommended order after vector-shapes:

1. media/image
2. plot/graph
3. text-like
4. containers/grouping only if truly justified

Text-like and container families are intentionally later because they carry more complex build/runtime/layout implications.

QA scenario:

- **Tooling:** per-family tests, targeted image/video parity checks, checkpoint review
- **Steps:**
  1. migrate only one additional family at a time
  2. run `cargo test`
  3. run representative AST/image checks for that family's existing examples or focused fixtures
  4. for families with visible animation behavior, run a targeted video export check
  5. repeat the Slice 3 checkpoint review before starting the next family
- **Expected result:** each migrated family keeps parity and does not force unrelated abstractions onto the remaining runtime

### 8.6 Phase 2 checkpoints

- **P2-C1:** minimal trait-object surface defined
- **P2-C2:** vector-shape pilot migrated with parity
- **P2-C3:** pilot review completed and go/no-go decided
- **P2-C4+:** each additional primitive family migrated one at a time

### 8.6.1 Current slice records

### Slice record

- **Phase:** 2
- **Slice name:** P2-S1 planning — vector-shape trait surface
- **Intent:** Freeze the vector-shape pilot boundary, trait responsibilities, and parity inventory before implementation begins
- **Files expected to change:** `.sisyphus/plans/refactor_roadmap.md`
- **Behavioral risk:** low
- **Parity checks planned:** document review only
- **Docs expected to change:** this roadmap only
- **Safe midpoint commit target:** `refactor: record phase 2 vector-shape pilot plan`

### Slice record

- **Phase:** 2
- **Slice name:** P2-S2 implementation start — vector-shape trait seam
- **Intent:** Add the minimal internal vector-shape trait/registry/shared-state seam without routing behavior yet
- **Files expected to change:** `timeline/primitive.rs`, `timeline/shapes.rs`, targeted tests
- **Behavioral risk:** medium
- **Parity checks planned:** `cargo test`
- **Docs expected to change:** roadmap ledger only unless the internal architecture description changes materially
- **Safe midpoint commit target:** `refactor: add vector-shape primitive trait seam`

### 8.7 Phase 2 exit criteria

Phase 2 is done only when:

- the trait-object system reduces central branching in a meaningful way
- migrated families preserve runtime parity
- the abstraction is not forcing unrelated families into leaky common behavior
- public grammar/features remain unchanged

---

## 9. Validation Ladder For Every Slice

Each refactor slice should use the narrowest validation that proves parity, then widen as needed.

### Minimum validation expectations

1. relevant unit/integration tests
2. `cargo test`
3. targeted CLI parity checks grounded in the current contributor workflow

### CLI parity workflow

Parser/boundary confidence:

```bash
cargo run -- ast examples/showcase.amx
```

Runtime/frame parity:

```bash
cargo run -- image examples/showcase.amx --time 0.0 --output /tmp/frame0.png
cargo run -- image examples/showcase.amx --time 1.5 --output /tmp/frame1.png
cargo run -- image examples/reactive_runtime.amx --time 0.5 --output /tmp/reactive_a.png
cargo run -- image examples/reactive_runtime.amx --time 1.5 --output /tmp/reactive_b.png
```

End-to-end parity when appropriate:

```bash
cargo run -- video examples/showcase.amx --output /tmp/check.mp4 --fps 30
```

Tree-sitter grammar validation is only required if syntax/highlighting files are touched. This roadmap should normally avoid that.

---

## 10. Documentation Update Rules

This roadmap should update related docs only when the refactor creates a real documentation need.

### Update `docs/architecture.md` when

- ownership boundaries materially change
- the documented compile/build/evaluate boundary becomes out of date
- primitive-family/capability structure becomes part of the accurate internal story

### Update `docs/implementation_plan.md` when

- the internal refactor becomes an active near-term execution priority that affects roadmap sequencing
- the status of this refactor roadmap changes materially

### Update `docs/development.md` when

- validation workflow changes
- new debug/inspection commands appear

### Do **not** update `docs/spec.md` unless

- shipped behavior changes, which this roadmap is specifically trying to avoid

---

## 11. Per-Slice Execution Template

Fill this out for every executed slice.

### Slice record

- **Phase:**
- **Slice name:**
- **Intent:**
- **Files expected to change:**
- **Behavioral risk:** low / medium / high
- **Parity checks planned:**
- **Docs expected to change:**
- **Safe midpoint commit target:**

### Completion record

- **Files changed:**
- **Tests/commands run:**
- **Docs updated:**
- **Checkpoint reached:**
- **Commit SHA:**
- **Open follow-up risks:**

---

## 12. Stop Conditions

Pause and reassess immediately if any of the following happens:

- the refactor appears to require grammar or feature changes to proceed
- a trait-object abstraction starts forcing unrelated primitive families into the same contract
- parity cannot be demonstrated for a completed slice
- boundary decisions remain unclear after code/doc inspection
- a slice starts mixing structural cleanup with opportunistic feature work

When paused, record the uncertainty in the status ledger before continuing in a later session.
