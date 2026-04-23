# Animatix Layout Design

This document defines the current layout model: layout-first as the default authoring model, absolute positioning as a first-class explicit escape hatch, and a narrow declaration-time measure/place contract.

Animatix should become easier for both humans and AI to author by default, without losing the ability to build hand-placed motion graphics.

---

## 1. Core Model

Animatix layout should follow a parent-driven model:

- containers decide child placement when layout semantics are active
- children report size and layout-relevant properties
- manual placement is opt-in, not inferred from magic values

This means placement should no longer depend on sentinel behavior like `(0, 0)` meaning "unset".

---

## 2. Placement Modes

Every actor or child should conceptually be in one of these placement modes:

### A. Layout-managed

The parent container owns placement.

```animatix
row: Row {
  a: Circle, radius: 20
  b: Circle, radius: 20
}
```

### B. Scene-relative

Placement is derived from scene anchors or scene-relative units.

```animatix
title: Text, anchor: scene.top, offset: (0, 80)
panel: Rect, at: (50%, 60%)
```

### C. Manual absolute

The actor opts into direct authored placement.

```animatix
badge: Circle, radius: 18, at: (1180, 80)
```

---

## 3. Recommended Syntax Direction

### Keep valid today
```animatix
orb: Circle, radius: 40, at: (240, 360)

row: Row, at: (640, 360), gap: 20 {
  a: Circle, radius: 20
  b: Circle, radius: 20
}
```

### Preferred current patterns
```animatix
row: Row, gap: 20 {
  a: Circle, radius: 20
  b: Circle, radius: 20
}

stack: Stack {
  bg: Rect, size: (280, 120)
  badge: Circle, radius: 22
}

title: Text, anchor: scene.top, offset: (0, 80)
```

### Defer
- full general-purpose constraints
- multi-pass dependency-based alignment systems
- implicit "smart" reflow with unclear precedence

---

## 4. Shipped Layout Surface

### Row / Col
- children are either layout-managed or manual
- `(0, 0)` sentinel behavior is removed
- containers can omit explicit `at` — defaults to `scene.center`

### Stack
- default overlapping container
- children share the same container placement origin
- later children render on top of earlier children
- use cases: badges, labels over shapes, foreground/background composition

### Grid
- default structured 2D layout container
- predictable row/column placement with `gap` support
- explicit column count, deterministic ordering from declaration order

### Scene-relative placement
- scene anchors: center, top, bottom, left, right, corners
- percentage-based `at`
- optional `offset`

---

## 5. Non-Goals

The current layout model should **not** try to solve:
- full constraint solving
- automatic collision avoidance
- responsive reflow across arbitrary breakpoints
- per-frame relayout from animated content, scale, or visibility changes
