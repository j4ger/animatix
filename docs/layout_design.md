# Animatix Layout Design

This document describes the current shipped layout model.

Animatix uses a parent-driven layout system with an explicit manual-placement escape hatch. Layout is intentionally narrower than full CSS or constraint-based UI layout.

---

## 1. Core Model

- containers decide child placement when layout semantics are active
- children participate in layout only if they have an admitted `layout_size`
- manual placement is explicit, not inferred from sentinel values
- scene-graph membership and layout membership are related but not identical

The key current distinction is:

- `track.children` = scene traversal / rendering graph
- `container_metadata.layout_children` = admitted subset used for layout computation

---

## 2. Placement Modes

### A. Layout-managed

The parent container owns authored placement.

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

Manual children remain in the scene graph, but layout output only assigns positions to layout-managed children.

---

## 3. Layout Measurement Contract

Container layout consumes a dedicated `layout_size` track.

- shapes usually seed it from authored geometry
- text / math / code seed it from measured glyph bounds
- image seeds it from authored or intrinsic image size
- svg seeds it from measured SVG bounds when available

Legacy `size` still exists for rendering/runtime compatibility, but layout no longer reads it directly.

Children without seeded `layout_size` are excluded from layout admission. A build warning is emitted for layout-managed children excluded this way.

---

## 4. Shipped Layout Surface

### Row / Col
- Taffy-backed linear layout
- deterministic authored-order placement
- `gap` and cross-axis `align` supported
- containers can omit explicit `at` and default to `scene.center`

### Grid
- Taffy-backed grid placement
- deterministic ordering from declaration order
- explicit `cols`
- `gap` supported

### Stack
- special-cased, not Taffy-backed
- all admitted children share the same origin
- use cases: overlays, badges, foreground/background composition

### Scene-relative placement
- scene anchors: center, top, bottom, left, right, corners
- percentage-based `at`
- optional `offset`

---

## 5. Admission and Membership

At build time, containers snapshot two child lists:

- `child_order`: raw authored child order
- `layout_children`: admitted subset with seeded `layout_size`

Layout computation uses only `layout_children`.

This means:

- a child may exist and render in the scene graph but not participate in layout
- missing layout measurement is surfaced as a diagnostic, not silently replaced in layout computation

---

## 6. Dynamic Layout

When `config { dynamic_layout: true }` is enabled:

- admitted children are resampled from `layout_size` at frame time
- layout-managed children receive recomputed positions
- manual children are still excluded from authored position assignment

Current dynamic layout still uses static admitted membership from container metadata; it does not re-admit children at frame time.

---

## 7. Non-Goals

The current layout model does **not** try to solve:

- full constraint solving
- automatic collision avoidance
- responsive reflow across arbitrary breakpoints
- container-property animation (`gap`, `align`, `cols` remain static metadata)
- scene-graph membership changes driven by frame-time layout admission
