# Extensible Colorscheme Design for Animatix

> **Status: Design document — updated to reflect dual API approach**
>
> Built-in colorschemes v1 are shipped: `config { colorscheme: ... }`, semantic color aliases through `color` / `stroke`, and `color: auto` are part of the current runtime contract.
>
> The `Colorscheme` primitive with `extends` inheritance is implemented and functional for inline scheme definition.
>
> Standard module-based scheme reuse via `pub let` exports and `import` is the active future work direction.

The guiding decisions are:

- **colorschemes should be definable inline via the `Colorscheme` primitive** (shipped)
- **colorschemes should be reusable via standard modules** (future work)
- **explicit actor colors must remain the final authority** (shipped)
- **extensibility should be declarative data, not executable code** (design principle)

Animatix should make it easier for users to get a coherent palette and distinct actor colors by default, without taking away the current direct `color:` workflow.

---

## 1. Core Model

Animatix should treat a colorscheme as a **declarative color contract** with two complementary pieces:

1. **semantic tokens** for fixed scene/UI-like roles
2. **an auto color pool** for deterministic distinct-color assignment

### Semantic Tokens

Semantic tokens provide stable named roles:

- `scene.background`, `text.primary`, `text.secondary`
- `accent.primary`, `accent.success`, `accent.warning`, `accent.danger`
- `surface.primary`, `surface.secondary`, `stroke.default`

These are for places where the author wants a specific semantic meaning rather than "give me the next distinct color."

### Auto Color Pool

The auto color pool is an ordered list of colors for distinct actor-like nodes when the author opts in.

The auto color pool is **not** a primitive-type default table. It is a deterministic pool used when the author opts into automatic distinct assignment.

---

## 2. Selected Product Direction

### A. Built-in named schemes

Animatix ships with a small built-in set:
- `default-dark`, `default-light`, `editorial-dark`

### B. Inline scheme definition via `Colorscheme` primitive (SHIPPED)

```animatix
let ocean = Colorscheme {
    extends: "default-dark",
    auto: { (0.2, 0.4, 0.8), (0.1, 0.6, 0.5), (0.8, 0.3, 0.4) },
    scene.background: (0.05, 0.07, 0.12),
    text.primary: (0.95, 0.97, 1.0),
    text.secondary: (0.7, 0.75, 0.85),
    surface.primary: (0.08, 0.12, 0.2),
    surface.secondary: (0.12, 0.16, 0.25),
    accent.primary: (0.25, 0.55, 0.9),
    accent.success: (0.2, 0.7, 0.5),
    accent.warning: (0.9, 0.7, 0.3),
    accent.danger: (0.85, 0.35, 0.4),
    stroke.default: (0.9, 0.92, 0.95),
}

config { colorscheme: "ocean" }
```

The `extends` property allows inheritance from built-in schemes or other inline-defined schemes.

### C. Module-imported schemes via standard `import` (FUTURE WORK)

Users define colorschemes as standard `.amx` modules that export color constants.

```animatix
import "themes/editorial-dark.amx" as theme

config {
  colorscheme: "editorial-dark"
}

title: Text { text: "Animatix", color: theme.text_primary, anchor: scene.top, offset: (0, 80) }
subtitle: Text { text: "Palette-driven stage", color: theme.text_secondary, anchor: scene.top, offset: (0, 116) }

alice: Circle, radius: 22, color: auto, at: (520, 360)
bob: Circle, radius: 22, color: auto, at: (760, 360)
```

### D. Explicit per-actor override remains final

If an actor has an explicit `color:` or a later `node.color = ...` assignment, that should still win.

---

## 3. Precedence Rules

From lowest to highest priority:

1. runtime hardcoded property default (white)
2. selected colorscheme primitive-type defaults (when property is omitted)
3. alias-based declaration defaults through `color` / `stroke`
4. `color: auto` assignment from scheme auto pool
5. explicit declaration values such as `color`, `stroke`, `stroke_color`, `scene.background_color`
6. later timed assignments
7. frame-local reactive overrides (`always` blocks)

---

## 4. Primitive Default Colors

When a colorscheme is selected and a primitive omits explicit `color` or `stroke`, the runtime applies a scheme-appropriate default:

| Primitive Category | Default Property | Scheme Token | Examples |
|---|---|---|---|
| Text-like | `color` | `text.primary` | `Text`, `Math`, `Code` |
| Shape fill | `color` | `surface.primary` | `Circle`, `Rect`, `Polygon`, `Ellipse` |
| Shape stroke | `stroke` | `stroke.default` | `Line`, `Arrow`, `Arc` |
| Plot curves | `color` / `stroke` | `accent.primary` | `CartesianPlot`, `PolarPlot` |

---

## 5. Examples

### Minimal Built-In Scheme

```animatix
config {
  colorscheme: "editorial-dark"
}

// No explicit color needed — primitives receive scheme defaults automatically:
title: Text, text: "Animatix", anchor: scene.top, offset: (0, 80)
panel: Rect, size: (400, 200), at: (640, 360)
left: Circle, radius: 22, color: auto, at: (520, 360)
right: Circle, radius: 22, color: auto, at: (760, 360)
```

### Inline Scheme with Inheritance

```animatix
let ocean = Colorscheme {
    extends: "default-dark",
    auto: { (0.2, 0.4, 0.8), (0.1, 0.6, 0.5), (0.8, 0.3, 0.4) },
    text.primary: (0.95, 0.97, 1.0),
    surface.primary: (0.08, 0.12, 0.2),
    accent.primary: (0.25, 0.55, 0.9),
}

config { colorscheme: "ocean" }

title: Text, text: "Ocean Theme", color: text.primary
badge: Circle, radius: 20, color: auto
```

---

## 6. Diagnostics

The system provides diagnostics for:
- unknown colorscheme (built-in or inline name not found)
- unknown alias token (scheme lacks the requested token)
- invalid color value (malformed RGBA tuple)
- empty auto-assignment pool (no auto colors exist after resolution)
- inheritance cycle (circular `extends` references)

The runtime prefers build diagnostics with conservative fallback to built-in defaults, not crashes or partially uninitialized scene state.
