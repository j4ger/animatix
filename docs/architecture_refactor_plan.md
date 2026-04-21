# Animatix Architecture Refactor Plan

> **Status: internal engineering support lane with partial extraction already landed**
>
> This document tracks the staged refactor work needed to improve the current code architecture without changing the public DSL contract. It is intentionally separate from `docs/implementation_plan.md`, which is the product/runtime roadmap.
>
> **Explicitly out of scope:** the IR/VM workstream in `ir.rs` / `vm.rs`. That will be handled later.

> **Current planning role:** this document supports the roadmap in `docs/implementation_plan.md`. It should reduce delivery risk and improve internal boundaries, but it does not redefine product priority on its own.

---

## 1. Purpose

This plan exists so the architecture cleanup can be executed across multiple sessions without losing intent or accidentally turning into a rewrite.

## 1.1 Current execution snapshot

This plan started as a forward-looking refactor sequence, but parts of it have already landed in the codebase. In particular, `crates/animatix/src/timeline/` already contains extracted modules such as `build.rs`, `declarations_text.rs`, `colorscheme.rs`, `plot.rs`, `layout.rs`, `media.rs`, `svg.rs`, `image.rs`, `runtime.rs`, `scene_eval.rs`, `property_lookup.rs`, `timing.rs`, `assignments.rs`, and `position.rs`.

That means this document should now be read as a **support-lane status + remaining-work plan**, not as a pristine greenfield sequence.

Practical rule: if roadmap work can ship cleanly through the existing extracted seams, prefer shipping it over reopening broad refactor phases.

The main goals are:

1. reduce the monolithic responsibility load in `crates/animatix/src/timeline/mod.rs`
2. tighten internal boundaries between build-time lowering and runtime evaluation
3. remove obviously duplicated declaration-processing paths
4. move GUI-owned derived logic back into the core crate where appropriate
5. improve internal typing and asset-loading seams

This plan should preserve the existing language/runtime contract unless a future phase explicitly says otherwise.

---

## 2. Current Problem Summary

The original architecture review identified these non-IR/VM issues as the highest-value refactor targets. Some related extraction work has landed since then, but these remain the guiding problem statements for the support lane:

### 2.1 `timeline/mod.rs` is a god module

`crates/animatix/src/timeline/mod.rs` currently mixes too many responsibilities:

- timeline building from AST
- actor/content declaration handling
- modifier parsing
- plotting/math helpers
- geometry/path helpers
- layout logic
- asset-loading coordination
- frame evaluation and render-scene assembly
- assorted diagnostics helpers

This is the central architectural problem. Most other design smells are downstream of it.

### 2.2 Text / Math / Code processing is duplicated or insufficiently consolidated

The declaration-processing paths for `Stmt::Text`, `Stmt::Math`, and `Stmt::Code` historically shared nearly identical logic for:

- timing modifier parsing
- color resolution including `color: auto`
- position binding resolution
- track creation and insertion
- diagnostics and default handling

That duplication makes changes higher-risk than they should be.

### 2.3 Build-time and runtime boundaries are too blurry

`Timeline` currently acts as both:

- the output of the build/lowering pipeline, and
- the runtime evaluator used by preview/export/rendering

The code works, but the internal ownership boundary is still less explicit than the docs imply, even after partial extraction.

### 2.4 GUI duplicates runtime-derived logic

`crates/animatix-gui/src/document.rs` currently computes or extracts information that should be owned by the core library, including:

- timeline duration
- scene dimensions/config-derived resolution
- some track/keyframe traversal knowledge

This is not catastrophic, but it creates drift risk.

### 2.5 Asset-loading concerns are mixed into timeline processing

Timeline processing still contains direct asset-loading coordination, including inline filesystem-backed SVG handling. That muddies the separation between:

- source/program lowering,
- asset acquisition/parsing, and
- timeline construction.

### 2.6 Some internal typing is weaker than it should be

The strongest example is shape typing: raw shape constants are still standing in for a proper enum-backed internal model.

---

## 3. Refactor Guardrails

These guardrails are mandatory for every phase.

### 3.1 Do not change the public DSL contract

Unless a separate feature task explicitly asks for a language/runtime behavior change, this refactor should not change:

- `.amx` syntax
- public statement/expression semantics
- the shipped colorscheme contract
- animation behavior that users rely on

### 3.2 Do not mix this with the IR/VM workstream

This plan must not attempt to redesign:

- `ir.rs`
- `vm.rs`
- modifier bytecode strategy
- compiled-expression architecture

The refactor may improve boundaries around those systems, but it should not restructure them.

### 3.3 Prefer staged extraction over rewrite

Each phase should:

1. compile on its own
2. preserve behavior
3. keep tests runnable
4. leave the codebase in a better state than before

No big-bang rewrite.

### 3.4 Keep `timeline/mod.rs` shrinking, not growing

If a phase adds new logic to `timeline/mod.rs`, it is probably going in the wrong direction.

---

## 4. Recommended Phase Order

The work should happen in this order:

1. shape typing cleanup
2. Text/Math/Code declaration deduplication
3. extract helper modules from `timeline/mod.rs`
4. clarify build-time vs runtime boundary
5. asset-loading cleanup
6. GUI/runtime ownership cleanup
7. final orchestration cleanup and documentation sync

This order is intentionally conservative: low-risk structural wins first, boundary clarification second, integration cleanup after the core is calmer.

## 4.1 Status against the current codebase

The phase order remains useful as an organizing principle, but the repository is no longer at phase-zero:

- **Partially landed / visibly underway:** helper extraction from `timeline/mod.rs`, text-like declaration deduplication, build/runtime separation, asset-related seams
- **Needs refreshed status review before more refactor work:** exact shape-typing completion state, remaining GUI/core ownership duplication, final orchestration cleanup

Before starting a new refactor session, the first step should be to update this document against the current `timeline/` directory rather than assume every phase is still untouched.

---

## 5. Phase 1 — Replace Weak Shape Typing

**Goal:** remove raw shape constants and replace them with proper internal typing.

### Target areas

- `crates/animatix/src/timeline/mod.rs`
- `crates/animatix/src/timeline/track.rs`
- any helper functions currently switching on raw shape constants

### Work

- introduce a `ShapeType` enum
- replace raw shape constants with enum variants
- change `AnimationTrack` / `PropertyTrack` usage accordingly
- update shape-related helpers and comparisons

### Why first

This is a contained, high-signal cleanup that improves type safety before larger module extraction begins.

### Validation

- shape-related tests still pass
- no raw shape constant comparisons remain
- no behavior changes in rendered primitives

---

## 6. Phase 2 — Deduplicate Text / Math / Code Declaration Processing

**Goal:** merge the repeated declaration-processing logic into a shared internal pipeline.

### Target areas

- `crates/animatix/src/timeline/mod.rs`
- extracted/shared declaration module (currently aligned more closely with `crates/animatix/src/timeline/declarations_text.rs` than the older `content.rs` placeholder name)

### Work

- create a shared content-declaration processor for text-like declarations
- parameterize only the true differences:
  - content property name
  - default size/style defaults
  - content compiler/render-path builder
- keep diagnostics and timing/color/position handling unified

### Expected result

One internal codepath for the common behavior of `Text`, `Math`, and `Code`, instead of three near-copies or only partially unified paths.

### Validation

- text/math/code tests still pass unchanged
- no user-visible behavior change
- diff clearly removes duplicated logic from `timeline/mod.rs`

---

## 7. Phase 3 — Extract Internal Helper Modules from `timeline/mod.rs`

**Goal:** turn `timeline/mod.rs` into an orchestrator instead of a kitchen sink.

### Likely extraction candidates

- further narrowing of `timeline/mod.rs` by leaning on already-present seams such as `declarations_text.rs`, `timing.rs`, `property_lookup.rs`, `plot.rs`, `media.rs`, `svg.rs`, `image.rs`, `position.rs`, and `layout.rs`
- any still-missing focused modules only where a real responsibility cluster remains and no existing extracted module is the natural home

### Extraction rules

- move pure helpers and tightly-related logic first
- do not change semantics during extraction
- prefer moving complete responsibility clusters, not random functions

### Recommended extraction order inside the phase

1. modifier parsing helpers
2. plotting/math helpers
3. geometry/path helpers
4. content declaration helpers
5. asset-loading helpers

In the current codebase, this should usually mean **moving more responsibility into the existing extracted modules** instead of creating placeholder modules purely to match the original plan text.

### Expected result

`timeline/mod.rs` should mainly coordinate timeline state and evaluation rather than implement every subsystem directly.

### Validation

- tests stay green after each sub-extraction
- line count of `timeline/mod.rs` decreases materially
- new modules have coherent ownership rather than being miscellaneous buckets

---

## 8. Phase 4 — Clarify Build-Time vs Runtime Boundary

**Goal:** make the internal architecture match the intended pipeline more honestly.

### Current issue

The docs describe a clear boundary around post-expansion lowering into `Timeline`, but the code still makes `Timeline` feel like both a builder product and the builder itself.

### Work

- introduce a dedicated builder/lowering layer, such as `TimelineBuilder` or equivalent internal structure
- move AST/program-lowering responsibilities behind that builder
- keep runtime/evaluation behavior on `Timeline`
- do not redesign IR/VM while doing this

### Target areas

- `crates/animatix/src/timeline/mod.rs`
- new internal build module, likely `crates/animatix/src/timeline/build.rs`
- callers in CLI and GUI that currently call `Timeline::build*` directly

### Expected result

The code should more clearly express:

- module/program expansion
- timeline lowering/build
- runtime/frame evaluation

as separate concerns.

### Validation

- CLI and GUI still build timelines exactly as before
- public behavior unchanged
- no IR/VM execution-path changes introduced accidentally

---

## 9. Phase 5 — Clean Up Asset-Loading Boundaries

**Goal:** stop doing ad hoc asset/file loading inline inside broad timeline statement processing.

### Work

- extract SVG loading/parsing coordination into an asset-facing helper module
- keep image loading behind the same kind of seam
- ensure diagnostics remain honest when asset reads fail

### Target areas

- `crates/animatix/src/timeline/mod.rs`
- `crates/animatix/src/timeline/svg.rs`
- `crates/animatix/src/timeline/image.rs`
- likely new `crates/animatix/src/timeline/assets.rs`

### Expected result

Timeline build code should request prepared asset content or asset-loading services, not embed raw file-loading logic inline.

### Validation

- SVG and image examples still render correctly
- asset-failure diagnostics still surface properly
- inline filesystem handling is reduced or eliminated from broad statement-processing codepaths

---

## 10. Phase 6 — Move Derived Runtime Knowledge Out of the GUI

**Goal:** reduce duplication between `animatix` and `animatix-gui`.

### Current issue

The GUI currently reimplements or reconstructs information the runtime/core library already knows.

### Work

- expose runtime-owned helpers for:
  - duration
  - scene dimensions derived from config/AST where appropriate
  - keyframe-time queries if still needed by the GUI
- update `crates/animatix-gui/src/document.rs` to consume those helpers
- keep the GUI focused on editor/session/UI orchestration rather than timeline internals

### Secondary tooling follow-up

- isolate or replace the GUI’s fallback `.amx` syntax definition so the tooling boundary is cleaner
- the ideal future direction is alignment with the shipped tree-sitter grammar, but this phase does not need to solve full editor integration if that becomes too large

### Validation

- GUI preview behavior stays the same
- duplicated duration/dimensions logic disappears from the GUI crate
- cross-crate ownership becomes clearer

---

## 11. Phase 7 — Final Orchestration Cleanup

**Goal:** finish with a cleaner internal architecture and synced docs.

### Work

- review what remains in `timeline/mod.rs`
- remove leftover helpers that still belong in extracted modules
- ensure `architecture.md` still describes the code honestly
- update internal comments/docstrings if the build/runtime seam changed materially

### Expected end state

At the end of this phase:

- `timeline/mod.rs` is substantially smaller
- responsibility clusters live in focused internal modules
- GUI/runtime boundaries are less leaky
- internal typing is cleaner
- asset and content-processing seams are easier to reason about

---

## 12. Suggested Session Strategy

This plan is explicitly meant to survive multiple work sessions.

Because portions of the extraction work have already landed, each new session should begin by identifying which phase goals are already satisfied in code and which still remain.

### Good single-session units

- Phase 1 alone
- Phase 2 alone
- one extraction cluster from Phase 3
- Phase 5 alone
- Phase 6 alone

### Good checkpoints after each session

At the end of any session:

1. update this document with what landed
2. record any phase-order changes or new constraints discovered
3. list files touched
4. note validation run status

Recommended session note format:

```md
## Session Notes

### YYYY-MM-DD
- completed:
- files touched:
- validation:
- follow-up:
```

---

## 13. Validation Expectations

Each phase should be verified independently.

Minimum validation per phase:

- relevant unit/integration tests
- project build
- targeted manual verification for GUI or rendering changes where applicable

Examples:

- shape typing: shape/primitive tests
- content dedup: text/math/code timeline tests
- asset cleanup: SVG/image examples
- GUI cleanup: preview still renders and duration/timeline UI still behaves correctly

---

## 14. Explicit Non-Goals

This plan does **not** currently cover:

- IR lowering redesign
- bytecode VM redesign
- modifier compilation strategy changes
- public DSL redesign
- renderer replacement
- a full rewrite of timeline construction

If any phase starts pulling in those concerns, it should stop and split the work.

---

## 15. Success Criteria

This refactor is successful when:

1. `timeline/mod.rs` is no longer the single implementation site for most engine behavior
2. Text/Math/Code processing no longer exists as near-copy-pasted logic
3. build-time lowering and runtime evaluation are easier to distinguish in code
4. GUI no longer duplicates core timeline-derived logic unnecessarily
5. asset-loading boundaries are cleaner and easier to test
6. internal typing is stronger in obvious places like shape representation
7. all of the above land without disturbing the public DSL contract or dragging in the IR/VM workstream
