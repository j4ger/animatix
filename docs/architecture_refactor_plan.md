# Animatix Architecture Refactor Plan

> **Status: support lane for internal boundary improvements**
>
> This document tracks refactor work to improve internal code architecture without changing the public DSL contract. Separate from `docs/implementation_plan.md` (product/runtime roadmap).
>
> **Out of scope:** IR/VM workstream in `ir.rs` / `vm.rs`.

---

## 1. Purpose

Clean up architecture across multiple sessions without losing intent or accidentally turning into a rewrite.

**Main goals:**

1. reduce monolithic responsibility in `crates/animatix/src/timeline/mod.rs`
2. tighten internal boundaries between build-time lowering and runtime evaluation
3. remove duplicated declaration-processing paths
4. move GUI-owned derived logic back into the core crate where appropriate
5. improve internal typing and asset-loading seams

---

## 2. Current Problems (Summary)

- `timeline/mod.rs` is a god module mixing timeline building, declaration handling, modifiers, plotting, layout, asset coordination, and evaluation
- Text/Math/Code declaration processing has duplicated timing, color, position, and track-creation logic
- `Timeline` acts as both build/lowering output and runtime evaluator—boundary is blurry
- GUI duplicates runtime-derived logic (duration, scene dimensions, keyframe traversal)
- Asset-loading is inline in timeline processing rather than behind a clean seam
- Shape typing uses raw constants instead of a proper enum

---

## 3. Refactor Guardrails

Every phase must: compile independently, preserve behavior, keep tests runnable, and leave `timeline/mod.rs` smaller rather than larger. Do not change the public DSL contract. Do not mix in IR/VM redesign.

---

## 4. Recommended Phase Order

1. shape typing cleanup
2. Text/Math/Code declaration deduplication
3. extract helper modules from `timeline/mod.rs`
4. clarify build-time vs runtime boundary
5. asset-loading cleanup
6. GUI/runtime ownership cleanup
7. final orchestration cleanup and documentation sync

**Phase status:** Significant extraction work has already landed—helper modules (`build.rs`, `declarations_text.rs`, `colorscheme.rs`, `plot.rs`, `layout.rs`, `media.rs`, `svg.rs`, `image.rs`, `runtime.rs`, `scene_eval.rs`, `property_lookup.rs`, `timing.rs`, `assignments.rs`, `position.rs`) exist in `crates/animatix/src/timeline/`. Many phases below have partial work done; treat this as a remaining-work plan against a partially-refactored codebase.

---

## 5. Phase 1 — Replace Weak Shape Typing

**Goal:** remove raw shape constants, use a proper `ShapeType` enum.

**Target areas:** `timeline/mod.rs`, `timeline/track.rs`

**Work:** introduce `ShapeType` enum, replace raw constants, update helpers and comparisons.

**Validation:** shape tests pass, no raw constant comparisons remain.

---

## 6. Phase 2 — Deduplicate Text / Math / Code Declaration Processing

**Goal:** merge repeated declaration-processing logic into a shared internal pipeline.

**Target areas:** `timeline/mod.rs`, `timeline/declarations_text.rs`

**Work:** create a shared content-declaration processor for text-like declarations, parameterizing only true differences (content property name, defaults, compiler path). Keep diagnostics and timing/color/position handling unified.

**Validation:** text/math/code tests pass unchanged, duplication removed from `timeline/mod.rs`.

---

## 7. Phase 3 — Extract Internal Helper Modules from `timeline/mod.rs`

**Goal:** turn `timeline/mod.rs` into an orchestrator.

**Target areas:** `timeline/mod.rs`, existing extracted modules (`declarations_text.rs`, `timing.rs`, `property_lookup.rs`, `plot.rs`, `media.rs`, `svg.rs`, `image.rs`, `position.rs`, `layout.rs`)

**Work:** lean on existing extracted seams; move remaining responsibility clusters into appropriate modules rather than creating placeholder modules. Move modifier parsing helpers, plotting/math helpers, geometry/path helpers, content declaration helpers, asset-loading helpers.

**Validation:** tests stay green, `timeline/mod.rs` line count decreases.

---

## 8. Phase 4 — Clarify Build-Time vs Runtime Boundary

**Goal:** make `Timeline` only the runtime evaluator, not the builder.

**Target areas:** `timeline/mod.rs`, `timeline/build.rs`, CLI and GUI callers

**Work:** introduce a dedicated builder/lowering layer (`TimelineBuilder` or equivalent). Move AST/program-lowering responsibilities behind that builder. Keep runtime/evaluation on `Timeline`. Do not redesign IR/VM.

**Validation:** CLI and GUI still build timelines, public behavior unchanged.

---

## 9. Phase 5 — Clean Up Asset-Loading Boundaries

**Goal:** extract ad hoc asset/file loading from inline timeline statement processing.

**Target areas:** `timeline/mod.rs`, `timeline/svg.rs`, `timeline/image.rs`, likely `timeline/assets.rs`

**Work:** extract SVG/image loading into asset-facing helper modules. Keep diagnostics honest when asset reads fail.

**Validation:** SVG/image examples render correctly, diagnostics surface properly.

---

## 10. Phase 6 — Move Derived Runtime Knowledge Out of the GUI

**Goal:** reduce duplication between `animatix` and `animatix-gui`.

**Target areas:** `crates/animatix-gui/src/document.rs`, core library

**Work:** expose runtime-owned helpers for duration, scene dimensions, keyframe-time queries. Update GUI to consume those helpers. Consider isolating the GUI's fallback `.amx` syntax definition.

**Validation:** GUI preview behavior unchanged, duplicated logic disappears from GUI crate.

---

## 11. Phase 7 — Final Orchestration Cleanup

**Goal:** cleaner internal architecture, synced docs.

**Work:** review remaining `timeline/mod.rs` content, move leftover helpers to extracted modules, update `architecture.md` and internal comments if build/runtime seam changed.

**Expected end state:** `timeline/mod.rs` substantially smaller, focused modules own responsibility clusters, GUI/runtime boundaries clearer, internal typing cleaner.
