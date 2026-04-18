# Animatix Layout Design and Phase 2 Direction

This document now serves two purposes:

1. preserve the rationale behind the shipped Phase 1 layout model, and
2. describe the next truthful layout direction beyond that shipped baseline.

For the current shipped surface, treat `docs/spec.md`, `docs/primitives.md`, and the roadmap in `docs/implementation_plan.md` as the source of truth.

The guiding decision is simple:

- **layout-first is the default authoring model**
- **absolute positioning remains a first-class explicit escape hatch**

Animatix should become easier for both humans and AI to author by default, without losing the ability to build hand-placed motion graphics.

---

## 1. Goals

Phase 1 layout work should accomplish five things:

1. reduce reliance on explicit `at: (x, y)` for normal scene composition
2. preserve exact manual placement for intentionally hand-crafted scenes
3. make container behavior deterministic and easy to generate
4. remove ambiguous runtime behavior around auto-vs-manual placement
5. create a foundation for later `Grid`, `Stack`, and scene-relative placement

---

## 2. Core Model

Animatix layout should follow a parent-driven model:

- containers decide child placement when layout semantics are active
- children report size and layout-relevant properties
- manual placement is opt-in, not inferred from magic values

This means placement should no longer depend on sentinel behavior like `(0, 0)` meaning “unset”.

---

## 3. Placement Modes

Every actor or child should conceptually be in one of these placement modes:

### A. Layout-managed

The parent container owns placement.

Examples:
```animatix
row: Row {
  a: Circle, radius: 20
  b: Circle, radius: 20
}
```

The children do not specify manual coordinates. The container places them.

### B. Scene-relative

Placement is derived from scene anchors or scene-relative units.

Examples of target syntax direction:
```animatix
title: Text, anchor: scene.top, offset: (0, 80)
panel: Rect, at: (50%, 60%)
```

This is still explicit placement, but it avoids brittle fixed pixel math.

### C. Manual absolute

The actor opts into direct authored placement.

```animatix
badge: Circle, radius: 18, at: (1180, 80)
```

This remains valid and important for motion graphics, overlays, and precise art direction.

---

## 4. Precedence Rules

The runtime should follow these precedence rules.

### Rule 1: Explicit manual placement beats layout placement

If a child explicitly opts into manual placement, the container must not overwrite it.

### Rule 2: Layout-managed children use container placement

If a child does not opt into manual placement, the container owns its final placement.

### Rule 3: Container placement defaults apply only when placement is omitted

If a container omits explicit placement, the runtime uses container-default or scene-relative placement rules.

### Rule 4: Absolute positioning stays legal everywhere it is legal today

The system should not remove or silently reinterpret existing manual `at` semantics.

### Rule 5: No sentinel coordinates

`(0, 0)` must become a valid authored position, not an “unset” special case.

---

## 5. Phase 1 Runtime Semantics

Phase 1 should define these semantics explicitly.

### 5.1 Row / Col manual-vs-auto distinction

Current problem:
- `Row` / `Col` only auto-place children when the child’s current position is exactly `(0, 0)`

Phase 1 change:
- replace sentinel behavior with explicit placement semantics
- a child is either layout-managed or manual
- the runtime should never infer “manual” or “unset” from the numeric coordinate value itself

### 5.2 Optional container placement

Current problem:
- even layout containers still need explicit `at`

Phase 1 change:
- `Row`, `Col`, `Stack`, and `Grid` should be allowed to omit explicit placement
- omitted placement should resolve to a deterministic default

Recommended default:
- initial default anchor: `scene.center`

This default is simple, deterministic, and easy for AI to rely on.

### 5.3 Stack semantics

`Stack` should become the default overlapping container.

Phase 1 target behavior:
- children share the same container placement origin
- container-level `align` places children relative to a shared stack box
- later children render on top of earlier children
- manual child placement remains allowed

Primary use cases:
- badges
- labels over shapes
- foreground/background composition
- callouts and overlays

### 5.4 Grid semantics

`Grid` should become the default structured 2D layout container.

Phase 1 target behavior:
- predictable row/column placement
- `gap` support
- explicit column count or row/column flow configuration
- deterministic ordering from child declaration order
- manual child placement remains allowed, but default behavior is grid-managed

Primary use cases:
- dashboards
- legends
- equation term blocks
- repeated visual cards

### 5.5 Scene-relative placement primitives

Phase 1 should introduce a minimal scene-relative placement model before any advanced constraint system.

Recommended minimal surface:
- scene anchors: center, top, bottom, left, right, corners
- percentage-based `at`
- optional `offset`

Examples of intended direction:
```animatix
title: Text, anchor: scene.top, offset: (0, 80)
logo: Svg, at: (80%, 20%)
```

This is enough to remove a large amount of AI-generated coordinate math without making the language solver-heavy.

---

## 6. Recommended Syntax Direction

Phase 1 should prefer the smallest truthful syntax expansion.

### Keep valid today
```animatix
orb: Circle, radius: 40, at: (240, 360)

row: Row, at: (640, 360), gap: 20 {
  a: Circle, radius: 20
  b: Circle, radius: 20
}
```

### Add as new preferred patterns
```animatix
row: Row, gap: 20 {
  a: Circle, radius: 20
  b: Circle, radius: 20
}

stack: Stack, align: center {
  bg: Rect, size: (280, 120)
  badge: Circle, radius: 22
}

title: Text, anchor: scene.top, offset: (0, 80)
```

### Defer
- full general-purpose constraints
- multi-pass dependency-based alignment systems
- implicit “smart” reflow with unclear precedence

---

## 7. Incremental Rollout Plan

### Slice 1 — Semantics cleanup
- status: implemented for `Row` / `Col` child placement semantics
- remove `(0, 0)` sentinel behavior
- define layout-managed vs manual placement in runtime and docs
- preserve existing explicit `at`

### Slice 2 — Container defaults
- status: implemented for `Row`, `Col`, `Stack`, and `Grid` root containers via `scene.center`
- allow `Row` / `Col` without explicit `at`
- add default container placement behavior
- update demos and docs to show layout-first usage

### Slice 3 — `Stack`
- status: implemented
- implement runtime behavior
- add demo coverage
- document alignment and layering semantics

### Slice 4 — `Grid`
- status: implemented
- implement runtime behavior
- add demo coverage
- document ordering and gap semantics

### Slice 5 — Scene-relative placement
- status: implemented for scene anchors, percentage `at`, and `offset`
- add anchors / percentages / offset
- update examples to reduce manual coordinate math

Each slice should land with:
- runtime changes
- tests
- docs
- at least one focused example

---

## 8. Non-Goals for Phase 1

Phase 1 should **not** try to solve everything.

Not in scope:
- full constraint solving
- automatic collision avoidance
- responsive reflow across arbitrary breakpoints
- advanced per-child flex systems beyond what is necessary to establish sane defaults

The goal is to produce a predictable, AI-friendly layout foundation first.

---

## 9. Success Criteria

Phase 1 should be considered successful when:

1. authors can compose common scenes without needing absolute coordinates everywhere
2. absolute placement still works exactly where intentional manual composition is needed
3. `Stack` and `Grid` are runtime-real
4. docs and demos teach layout-first composition as the default
5. the runtime has explicit, testable semantics for auto vs manual placement

---

## 10. Phase 2 Direction — Size-Aware Measure/Place Layout

Phase 1 established deterministic container ownership and scene-relative placement. The next truthful layout step is **not** full CSS flexbox parity. It is a narrower **size-aware measure/place slice** that makes `Row` and `Col` rely on real child layout bounds instead of placeholder sizes.

The current runtime now proves that narrow slice for the primary first-wave participants: authored vector shapes, `Image`, and text-like declarations (`Text`, `Math`, `Code`) all publish layout size through the shared `size` track that container layout already consumes. What remains is to keep that contract explicit, well-tested, and clearly documented about where it stops.

### 10.1 Why this phase exists

Today the runtime already positions `Row`, `Col`, and `Grid` children from tracked child size, but the size-reporting contract is incomplete:

- some primitives already have meaningful authored or intrinsic size
- some primitives still fall back to placeholder track values
- layout is still applied mainly during timeline construction rather than from sampled child state

That means Animatix has the basic placement machinery for a flex-like system, but not yet the measurement truth needed to claim one honestly.

### 10.2 Goals for the first size-aware slice

This phase should accomplish four things:

1. make layout size an explicit runtime contract for layout-participating children
2. keep the contract deterministic and cheap enough for random-access evaluation
3. improve `Row` / `Col` authoring before introducing any broader flex vocabulary
4. document exactly which primitives report truthful layout size and which do not yet

### 10.3 Measurement contract

For the first slice, every layout-participating child should publish a **local layout size** into its existing size track.

Recommended contract:

- layout size is reported in the child’s local, unrotated coordinate space
- layout size should represent the bounds used for parent container placement
- the first implementation continues using the existing half-extents storage convention already consumed by container layout
- transforms such as rotation and visual-only scale should not redefine the base layout box in this slice

In concrete runtime terms, the shared `size` track stores `[half_width, half_height]` for layout. Containers double those half-extents when computing `Row`, `Col`, `Grid`, or `Stack` placement.

### 10.4 Initial primitive support

The first truthful slice should focus on primitives that already fit the current runtime architecture well.

#### Participates in the current tightened contract

- `Image` — reports explicit `size` when authored, otherwise its natural pixel size
- `Text`, `Math`, and `Code` — report measured bounds from the existing Typst/text-path compilation pipeline
- authored-shape primitives that already carry explicit size/radius semantics

#### Additional currently supported participant

- `Svg` currently computes local path bounds into the shared layout size track and can participate in declaration-time container placement, but it is still a less mature measurement path than authored shapes, text-like declarations, or `Image`

#### Still outside the guaranteed contract

- any primitive whose runtime rendering path does not yet expose stable local bounds back to layout

### 10.5 Container scope

The first size-aware layout slice should stay intentionally narrow.

#### In scope

- `Row` and `Col` continuing to own child placement
- existing `gap` semantics
- existing cross-axis `align` semantics
- truthful sizing for layout-managed children

#### Explicitly out of scope for this slice

- `flex-grow`
- `flex-shrink`
- `flex-basis`
- wrapping
- baseline alignment
- `justify-content: space-*`
- min/max sizing rules
- full parity with CSS Flexbox, Flutter Flex, Yoga, or Taffy

The target is to establish truthful measurement first, not to import a large flex vocabulary before the runtime can support it honestly.

### 10.6 Runtime model for the first slice

This phase should remain a **declaration-time measure/place** model:

- child bounds are computed when declarations or timed re-declarations establish the child state used for layout
- container placement continues to use deterministic parent-driven rules
- the runtime does **not** promise per-frame reflow from animated content, scale, or visibility changes yet

Later work may move layout toward sampled-state recomputation where needed, but that should be a separate architectural step rather than an accidental side effect of adding measured bounds.

### 10.7 Rollout plan

#### Slice 1 — Measurement contract and diagnostics
- define what “layout size” means in runtime/docs/tests
- identify placeholder-size fallbacks and make them explicit in diagnostics or docs
- add focused tests for layout size publication

#### Slice 2 — Text/Math/Code measured bounds
- extract local bounds from the existing compilation pipeline
- write those bounds into the size track used by container layout
- add one focused demo showing text participating truthfully in `Row` / `Col`

#### Slice 3 — Row/Col size-aware polish
- verify `Row` / `Col` behavior with mixed authored-size and measured-size children
- preserve manual child opt-out semantics
- keep layout deterministic under random-access evaluation

Status note: these three slices now exist in the runtime in a bounded form. The remaining work in this phase is mainly contract honesty: keeping docs, examples, and tests aligned with the shipped subset while avoiding broader flex claims.

#### Slice 4 — Future architectural follow-up
- evaluate when sampled child-state relayout becomes necessary
- only after that consider whether any broader flex vocabulary is warranted

### 10.8 Non-goals for the size-aware slice

This phase should still avoid:

- full general-purpose constraints
- a Cassowary-style solver
- responsive breakpoint systems
- collision avoidance
- solver-heavy dynamic layout dependencies
- claiming that animated text or animated visual scale automatically reflows neighboring layout in the current runtime

### 10.9 Success criteria for this phase

This phase should be considered successful when:

1. `Row` / `Col` produce truthful placement for supported measured children
2. docs clearly separate supported measured layout from unsupported future flex semantics
3. examples show measured text/media participating in layout without brittle coordinate math
4. tests prove the measurement contract rather than relying only on visual inspection
