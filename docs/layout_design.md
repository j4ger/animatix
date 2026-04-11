# Animatix Layout Design and Phase 1 Plan

This document turns the layout direction into an implementation-ready plan.

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
