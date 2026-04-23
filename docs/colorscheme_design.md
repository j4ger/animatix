# Extensible Colorscheme Design for Animatix

> **Status: Design document — updated to reflect dual API approach**
>
> Built-in colorschemes v1 are shipped: `config { colorscheme: ... }`, semantic color aliases through `color` / `stroke`, and `color: auto` are part of the current runtime contract.
>
> The `Colorscheme` primitive with `extends` inheritance is implemented and functional for inline scheme definition.
>
> Standard module-based scheme reuse via `pub let` exports and `import` is the active future work direction.

This document describes the complete colorscheme system design, including both the shipped APIs and the planned module-based extensions.

The guiding decisions are:

- **colorschemes should be definable inline via the `Colorscheme` primitive** (shipped)
- **colorschemes should be reusable via standard modules** (future work)
- **explicit actor colors must remain the final authority** (shipped)
- **extensibility should be declarative data, not executable code** (design principle)

Animatix should make it easier for users to get a coherent palette and distinct actor colors by default, without taking away the current direct `color:` workflow.

---

## 1. Goals

The colorscheme system should accomplish seven things:

1. reduce repeated palette boilerplate across `.amx` files
2. let a scene define a colorscheme inline or import one declaratively
3. support automatic distinct color assignment for actor-like nodes when the author opts in
4. preserve explicit `color`, `stroke`, and assignment-based overrides exactly as first-class author intent
5. stay deterministic and portable across CLI render, GUI preview, image export, and video export
6. support both inline definition and module-based reuse
7. apply scheme-appropriate default colors to primitives when no explicit color is authored

---

## 2. Non-Goals for the First Colorscheme Slice

This design deliberately does **not** try to solve all styling problems at once.

Out of scope for the first slice:

- executable/plugin colorschemes
- remote scheme loading
- a GUI-first colorscheme workflow
- a general CSS-like styling system for all actor properties
- primitive-type magic such as "all circles are blue" as the primary model
- per-frame procedural recoloring that bypasses the existing stateless track/override model

In scope for default colors:
- primitive-type-aware default color selection when no explicit color is authored
- semantic mapping from primitive categories to scheme tokens (text, surface, stroke, accent)
- per-instance override remains fully supported and takes precedence

The first useful version should feel like a palette module that exports color constants, not a second rendering language.

---

## 3. Current Baseline

Today, Animatix colors are authored explicitly:

- scene background color via `scene.background_color`
- actor fill/stroke colors via `color`, `stroke`, and `stroke_color`
- variables such as `let accent = (0.38, 0.78, 1.0, 1.0)` reused by later declarations
- timed property assignments such as `badge.color = red [250ms]`
- stateless frame-local overrides via `always`

This is flexible, but it means users repeatedly recreate the same palette setup across examples and projects.

The current examples already show the product need clearly:

- a small curated stage palette is declared at the top of many files
- multiple actor-like nodes need distinct colors from a shared set
- nested-label overrides such as `left.badge.color = red` are already part of the shipped mental model

That means a colorscheme system should be built as a **defaulting layer beneath explicit author intent**, not as a replacement for the existing property model.

---

## 4. Core Model

Animatix should treat a colorscheme as a **declarative color contract** with two complementary pieces:

1. **semantic tokens** for fixed scene/UI-like roles
2. **an auto color pool** for deterministic distinct-color assignment

### 4.1 Semantic Tokens

Semantic tokens provide stable named roles such as:

- `scene.background`
- `text.primary`
- `text.secondary`
- `accent.primary`
- `accent.success`
- `accent.warning`
- `accent.danger`
- `surface.primary`
- `surface.secondary`
- `stroke.default`

These are for places where the author wants a specific semantic meaning rather than "give me the next distinct color."

### 4.2 Auto Color Pool

The auto color pool is an ordered list of colors intended for distinct actor-like nodes.

Example use cases:

- multiple badges/cards on one stage
- repeated plotted series that need distinct identity colors
- named characters/speakers in an explanatory animation
- component instances that should be visually distinguishable without hand-assigning every color

The auto color pool is **not** a primitive-type default table. It is a deterministic pool used when the author opts into automatic distinct assignment.

### 4.3 Opt-In, Not Magic-By-Primitive

The system should avoid "all `Circle` nodes get scheme slot 1, all `Rect` nodes get scheme slot 2".

That model is brittle because:

- primitive type is not semantic identity
- a scene often uses many circles or rectangles with different roles
- it produces surprising recoloring when authors refactor structure

Instead, the author should opt into scheme-driven coloring explicitly.

---

## 5. Selected Product Direction

The recommended product shape has three layers:

### A. Built-in named schemes

Animatix ships with a small built-in set:

- `default-dark`
- `default-light`
- `editorial-dark`

These give the feature immediate value and define the reference schema.

### B. Inline scheme definition via `Colorscheme` primitive (SHIPPED)

Users define colorschemes inline using the `Colorscheme` primitive with property-based declaration:

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
// themes/editorial-dark.amx
pub let background = (0.04, 0.06, 0.10, 1.0)
pub let text_primary = (0.97, 0.98, 1.0, 1.0)
pub let text_secondary = (0.73, 0.80, 0.89, 1.0)
pub let accent_primary = (0.38, 0.78, 1.0, 1.0)
pub let accent_success = (0.35, 0.86, 0.63, 1.0)
pub let accent_warning = (0.98, 0.83, 0.44, 1.0)
pub let accent_danger = (1.0, 0.46, 0.54, 1.0)
pub let stroke_default = (0.97, 0.98, 1.0, 1.0)

pub let auto_pool = [
  (0.38, 0.78, 1.0, 1.0),
  (0.35, 0.86, 0.63, 1.0),
  (1.0, 0.46, 0.54, 1.0),
  (0.98, 0.83, 0.44, 1.0),
]
```

Scenes import and use them via standard module syntax:

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

This preserves the current DSL mental model and makes the new system safe to adopt incrementally.

---

## 6. Proposed Surface Direction

### 6.1 Scene-Level Selection (Built-in or Inline)

For built-in schemes or inline-defined schemes, the existing syntax remains:

```animatix
config {
  colorscheme: "editorial-dark"
}
```

This selects a scheme by name. Built-in schemes are hardcoded in the runtime. Inline-defined schemes are resolved from `let` declarations in the current file.

### 6.2 Inline Scheme Definition

```animatix
let ocean = Colorscheme {
    extends: "default-dark",
    auto: { (0.2, 0.4, 0.8), (0.1, 0.6, 0.5) },
    accent.primary: (0.25, 0.55, 0.9),
}

config { colorscheme: "ocean" }
```

### 6.3 Module-Qualified Color Access (FUTURE)

For user-defined schemes in separate files, use standard module imports:

```animatix
import "themes/brand-ocean.amx" as brand

headline: Text { text: "Branded stage", color: brand.text_primary, anchor: scene.top, offset: (0, 88) }
panel: Rect, size: (300, 160), color: brand.surface_primary, at: (640, 360)
```

### 6.4 Alias-Based Color Properties (Built-in or Inline Schemes)

When a scheme is selected via `config.colorscheme`, semantic aliases are available:

```animatix
config {
  colorscheme: "editorial-dark"
}

title: Text { text: "Animatix", color: text.primary, anchor: scene.top, offset: (0, 80) }
subtitle: Text { text: "Palette-driven stage", color: text.secondary, anchor: scene.top, offset: (0, 116) }

alice: Circle, radius: 22, color: auto, at: (520, 360)
bob: Circle, radius: 22, color: auto, at: (760, 360)
```

### 6.5 Stroke Aliases

The same model applies to stroke-style surfaces:

```animatix
axis: Line, from: (-120, 0), to: (120, 0), stroke: stroke.default
```

### 6.6 Primitive Default Colors (Automatic Scheme Application)

When a colorscheme is selected via `config.colorscheme` and a primitive omits explicit `color` or `stroke`, the runtime applies a scheme-appropriate default based on the primitive type:

| Primitive Category | Default Property | Scheme Token | Example Primitives |
|---|---|---|---|
| Text-like | `color` | `text.primary` | `Text`, `Math`, `Code` |
| Shape fill | `color` | `surface.primary` | `Circle`, `Rect`, `Polygon`, `Ellipse`, `Square`, `Dot`, `RegularPolygon` |
| Shape stroke | `stroke` | `stroke.default` | `Line`, `Arrow`, `Arc` |
| Plot curves | `color` / `stroke` | `accent.primary` | `CartesianPlot`, `PolarPlot`, `ParametricPlot`, `ImplicitPlot` |
| Image/Svg | (none) | — | `Image`, `Svg` use authored or intrinsic colors |

```animatix
config { colorscheme: "editorial-dark" }

// These all receive scheme-appropriate defaults automatically:
title: Text, text: "Hello"           // color defaults to text.primary
panel: Rect, size: (200, 100)        // color defaults to surface.primary
axis: Line, from: (-100, 0), to: (100, 0)  // stroke defaults to stroke.default
plot: CartesianPlot, func: (x) => x^2      // color defaults to accent.primary

// Explicit colors still override:
highlight: Circle, radius: 20, color: accent.warning  // explicit wins
```

**Rules:**
- Default colors apply only when the property is completely omitted from the declaration
- Explicit `color`, `stroke`, or `stroke_color` always wins over defaults
- `color: auto` is distinct from defaults — it cycles through the auto pool
- If the scheme lacks the expected token, fall back to the current hardcoded default (white)

### 6.7 Explicit Color Still Works

```animatix
alice: Circle, radius: 22, color: auto
bob: Circle, radius: 22, color: (1.0, 0.4, 0.5, 1.0)
```

In this example, `alice` receives an automatically assigned scheme color while `bob` uses the explicit authored color.

---

## 7. Why Both APIs?

The system supports both inline `Colorscheme` primitive and standard module import because they serve different use cases:

**Inline `Colorscheme` primitive:**
- Quick ad-hoc scheme definition in a single file
- Inheritance from built-in schemes via `extends`
- No separate file needed
- Good for experimentation and one-off scenes

**Standard module import:**
- Reusable across multiple scenes
- Version control friendly
- Composable with other module exports
- Follows the same patterns as component reuse

The trade-off is that module-qualified names are slightly more verbose than bare aliases (`theme.text_primary` vs `text.primary`), but this is acceptable for user-defined schemes. Built-in and inline schemes still provide the concise alias syntax via `config.colorscheme`.

---

## 8. Resolution and Precedence Rules

The feature lives or dies on whether its precedence is teachable.

The runtime should follow this order.

### Rule 1: Frame-local reactive overrides win

If an `always` block or another existing frame-local override path writes `node.color`, that result wins for the requested frame.

### Rule 2: Timed property assignments win over declaration defaults

If the author writes:

```animatix
badge.color = red [250ms]
```

that assignment wins over any colorscheme-derived declaration default.

### Rule 3: Explicit declaration colors beat alias-backed defaults

If a node has explicit `color`, `stroke`, or `stroke_color`, those values win over colorscheme-alias defaults from `color` / `stroke` declarations.

### Rule 4: Alias-backed defaults beat scheme fallback defaults

If a node uses `color: text.primary` or `color: auto`, that resolved scheme value wins over generic runtime default white.

### Rule 5: Scene background follows the same pattern

If the selected colorscheme defines `scene.background` and the author does **not** explicitly set `scene.background_color`, the scheme background applies.

If the author explicitly sets `scene.background_color`, the authored value wins.

### Rule 6: Primitive-type default colors

When a primitive omits `color` or `stroke` entirely and a scheme is selected, the runtime applies a primitive-category default from the scheme (e.g., `text.primary` for `Text`, `surface.primary` for `Rect`, `stroke.default` for `Line`).

If the scheme lacks the expected token, the runtime falls back to the hardcoded default (white).

### Canonical Order

From lowest to highest priority:

1. runtime hardcoded property default (white)
2. selected colorscheme primitive-type defaults (when property is omitted)
3. alias-based declaration defaults through `color` / `stroke` (e.g., `color: text.primary`)
4. `color: auto` assignment from scheme auto pool
5. explicit declaration values such as `color`, `stroke`, `stroke_color`, `scene.background_color`
6. later timed assignments
7. frame-local reactive overrides

This order should be documented in one place and mirrored in tests.

---

## 9. Auto Assignment Contract

`color: auto` should assign a deterministic color from the selected scheme's auto color pool.

### 9.1 Stable Identity Source

The assignment key should use the final runtime actor identity:

- explicit label path when available, such as `left.badge`
- component-expanded prefixed labels after expansion
- deterministic generated IDs for anonymous nodes when no label exists

### 9.2 Deterministic Mapping

The cycle assignment must be deterministic for a given compiled document.

Acceptable first rule:

- walk actor declarations in timeline-build order
- assign the next auto-assignment color to each unique actor-path that requests `color: auto`
- reuse the same color for later references to the same actor-path

### 9.3 Anonymous Actor Caveat

Anonymous nodes should still work if they receive deterministic auto-generated IDs, but the docs should recommend **explicit labels** for stable long-term visual identity.

### 9.4 Cycle Wrap Behavior

If the number of auto-assignment consumers exceeds the auto color pool length, the pool may wrap.

This is acceptable for v1 as long as:

- wrapping is deterministic
- the docs say it clearly
- the runtime can later warn when heavy reuse is likely to reduce clarity

---

## 10. Load Model

Colorscheme loading should fit the existing load/build architecture.

### 10.1 Load Time, Not Frame Time

Scheme resolution belongs to the document load / timeline-build phase.

The scheme should be fully resolved before frame evaluation so that:

- preview scrubbing stays deterministic
- image/video export sees the same scene state
- diagnostics appear during build rather than as mysterious runtime color changes

### 10.2 Built-In vs Inline vs Module Sources

The loader should support three sources:

1. **Built-in scheme names** (resolved from hardcoded `BuiltInColorscheme` enum) — selected via `config.colorscheme`
2. **Inline scheme definitions** (`let name = Colorscheme { ... }` in the current file) — referenced by name in `config.colorscheme`
3. **Module-imported schemes** (standard `.amx` files with `pub let` exports) — imported via `import "path" as name`

Built-in and inline schemes seed semantic aliases and the auto color pool. Module-imported schemes provide color values through standard module-qualified names.

### 10.3 Resolution Flow for Built-in Schemes

```
config.colorscheme: "editorial-dark"
  -> Resolve built-in scheme by name
  -> Build ResolvedColorscheme
  -> Seed environment with semantic aliases
  -> Apply to timeline
```

### 10.4 Resolution Flow for Inline Schemes

```
let ocean = Colorscheme { ... }
config.colorscheme: "ocean"
  -> Find inline Colorscheme declaration by name
  -> Resolve inheritance if extends is present
  -> Build ResolvedColorscheme
  -> Seed environment with semantic aliases
  -> Apply to timeline
```

### 10.5 Resolution Flow for Module Schemes

```
import "themes/brand-ocean.amx" as brand
  -> Load file via ModuleGraph (reuse existing file loading)
  -> Parse as AMX AST (reuse existing parser)
  -> Extract pub let exports
  -> Bind to module namespace
  -> Use via qualified names (brand.text_primary, etc.)
```

---

## 11. Diagnostics

This system should fail honestly and softly.

### Required diagnostics

1. **Unknown colorscheme**
   - selected built-in or inline name not found
2. **Unknown module path**
   - import path missing / unreadable (reuse existing module diagnostics)
3. **Invalid color value**
   - malformed RGBA tuple in module export or inline definition
4. **Unknown alias token**
   - `color: accent.branding` when the resolved scheme has no such token
5. **Empty auto-assignment pool**
   - `color: auto` used but no auto-assignment colors exist after resolution
6. **Colorscheme inheritance cycle**
   - circular `extends` references between inline schemes

### Failure strategy

The runtime should prefer:

- build diagnostics
- conservative fallback to built-in defaults or current hardcoded property defaults
- no crashes or partially uninitialized scene state

Unknown role usage should not silently pretend that the scheme worked.

---

## 12. Runtime Integration Direction

### 12.1 Build-Time Resolution

The timeline builder should resolve colorscheme-backed defaults into the same property-track system already used for explicit colors.

That means:

- the scheme chooses initial values
- the track system still owns interpolation and later assignments
- frame-time override semantics stay unchanged

### 12.2 Environment Seeding (Built-in and Inline Schemes)

For built-in and inline schemes, the environment exposes resolved scheme values under dotted names for expression use later.

```text
scheme.scene.background
scheme.text.primary
scheme.accent.warning
```

### 12.3 Scene Background Seeding

If a scheme is selected and the scene omits explicit `scene.background_color`, the loader/build phase seeds the background track from `scene.background`.

This should remain a simple default, not a second background system.

---

## 13. Examples of Intended Authoring Direction

### 13.1 Minimal Built-In Scheme with Automatic Defaults

```animatix
config {
  colorscheme: "editorial-dark"
}

// No explicit color needed — primitives receive scheme defaults automatically:
title: Text, text: "Animatix", anchor: scene.top, offset: (0, 80)           // color: text.primary
subtitle: Text, text: "Minimal boilerplate", anchor: scene.top, offset: (0, 116)  // color: text.secondary
panel: Rect, size: (400, 200), at: (640, 360)                               // color: surface.primary
axis: Line, from: (-120, 0), to: (120, 0), at: (640, 360)                   // stroke: stroke.default

// Explicit colors and auto still work:
left: Circle, radius: 22, color: auto, at: (520, 360)
right: Circle, radius: 22, color: auto, at: (760, 360)
highlight: Circle, radius: 18, color: accent.warning, at: (640, 500)       // explicit wins
```

### 13.2 Inline Scheme with Inheritance

```animatix
let ocean = Colorscheme {
    extends: "default-dark",
    auto: { (0.2, 0.4, 0.8), (0.1, 0.6, 0.5), (0.8, 0.3, 0.4) },
    scene.background: (0.05, 0.07, 0.12),
    text.primary: (0.95, 0.97, 1.0),
    text.secondary: (0.7, 0.75, 0.85),
    surface.primary: (0.08, 0.12, 0.2),
    accent.primary: (0.25, 0.55, 0.9),
    accent.success: (0.2, 0.7, 0.5),
    accent.warning: (0.9, 0.7, 0.3),
    accent.danger: (0.85, 0.35, 0.4),
    stroke.default: (0.9, 0.92, 0.95),
}

config { colorscheme: "ocean" }

title: Text, text: "Ocean Theme", color: text.primary
badge: Circle, radius: 20, color: auto
axis: Line, from: (-120, 0), to: (120, 0), stroke: stroke.default
```

### 13.3 Explicit Override

```animatix
config {
  colorscheme: "editorial-dark"
}

left: Circle, radius: 22, color: auto, at: (520, 360)
right: Circle, radius: 22, color: (1.0, 0.9, 0.2, 1.0), at: (760, 360)
```

### 13.4 Module-Imported Scheme (FUTURE)

Scene file:

```animatix
import "themes/brand-ocean.amx" as brand

headline: Text { text: "Branded stage", color: brand.text_primary, anchor: scene.top, offset: (0, 88) }
panel: Rect, size: (300, 160), color: brand.surface_primary, at: (640, 360)
```

Scheme module (`themes/brand-ocean.amx`):

```animatix
pub let background = (0.02, 0.05, 0.08, 1.0)
pub let text_primary = (0.95, 0.97, 1.0, 1.0)
pub let text_secondary = (0.6, 0.75, 0.9, 1.0)
pub let surface_primary = (0.08, 0.15, 0.22, 1.0)
pub let accent_primary = (0.2, 0.6, 0.9, 1.0)
pub let accent_success = (0.3, 0.8, 0.5, 1.0)
pub let accent_warning = (0.95, 0.7, 0.2, 1.0)
pub let accent_danger = (0.9, 0.3, 0.4, 1.0)
pub let stroke_default = (0.8, 0.9, 1.0, 1.0)

pub let auto_pool = [
  (0.2, 0.6, 0.9, 1.0),
  (0.3, 0.8, 0.5, 1.0),
  (0.9, 0.3, 0.4, 1.0),
  (0.95, 0.7, 0.2, 1.0),
]
```

### 13.5 Scheme as Component (FUTURE)

```animatix
// components/branded-card.amx
import "../themes/brand-ocean.amx" as brand

pub component BrandedCard {
  params {
    title: String,
    accent: Color = brand.accent_primary
  }
  
  card: Rect, size: (300, 160), color: brand.surface_primary
  card_title: Text, text: title, color: brand.text_primary, at: (0, -50)
  card_accent: Rect, size: (300, 4), color: accent, at: (0, 78)
}
```

---

## 14. Incremental Rollout Direction

The safest rollout is:

### Slice 1 — Built-in scheme model *(shipped)*

- define internal colorscheme data structures
- add one built-in default scheme
- support `config.colorscheme`
- seed scene background from scheme when omitted

### Slice 2 — Alias-backed declaration defaults *(shipped)*

- add colorscheme alias support through `color` / `stroke`
- support semantic token resolution
- preserve explicit `color` / `stroke` precedence

### Slice 3 — Automatic color assignment *(shipped)*

- add `color: auto`
- define deterministic actor-path assignment
- add precedence and wrap tests

### Slice 4 — Inline scheme definition via `Colorscheme` primitive *(shipped)*

- support `let name = Colorscheme { ... }` syntax
- support `extends` inheritance from built-in schemes
- support custom semantic tokens and auto pools
- add cycle detection for inheritance

### Slice 5 — Module-based scheme reuse *(active future work)*

- add `pub let` export syntax to parser and module system
- add `import "..." as name` syntax to parser
- implement module namespace binding for qualified access
- demonstrate scheme modules alongside other standard module patterns
- reuse existing `ModuleGraph` file loading and diagnostics

### Slice 5b — Primitive-type default colors *(active future work)*

- apply scheme-appropriate default colors when primitives omit explicit `color` / `stroke`
- define primitive-to-token mapping (text→text.primary, shape→surface.primary, stroke→stroke.default, plot→accent.primary)
- preserve explicit-color precedence: omitted < scheme default < alias < auto < explicit < assignment < always
- add tests verifying defaults apply correctly and explicit overrides still win
- add example demonstrating minimal-boilerplate scene authoring

### Slice 6 — Module-based scheme reuse *(active future work)*

- add `pub let` export syntax to parser and module system
- add `import "..." as name` syntax to parser
- implement module namespace binding for qualified access
- demonstrate scheme modules alongside other standard module patterns
- reuse existing `ModuleGraph` file loading and diagnostics

### Slice 7 — Broader expression exposure and GUI follow-up *(future follow-up)*

- optionally expose resolved scheme tokens in the evaluation environment
- later decide whether the GUI should surface scheme switching or role discovery

Each slice should land with:

- runtime changes
- tests
- docs
- one focused example where appropriate

---

## 15. What We Should Avoid

- making primitive type the primary color identity model
- creating a plugin/executable colorscheme system before the data-only model proves insufficient
- inventing a GUI dependency before the DSL/runtime contract is stable
- allowing unclear precedence between alias-backed `color`, explicit `color`, assignments, and `always`
- broadening into a full styling language before this smaller palette model lands honestly
- treating module-qualified names as too verbose — the clarity and composability are worth the extra characters
- removing the inline `Colorscheme` primitive before module-based reuse is fully functional

---

## 16. Success Criteria

The colorscheme design is successful when:

1. a user can select one built-in scheme and remove most repeated palette boilerplate from a normal scene
2. a user can define a custom scheme inline with the `Colorscheme` primitive
3. a user can define a custom scheme as a standard `.amx` module and import it into scenes
4. actor-like nodes can get distinct deterministic colors through an explicit opt-in surface
5. primitives without explicit colors receive scheme-appropriate defaults automatically
6. explicit actor colors and timed overrides still work exactly as users expect today
7. built-in aliases and module-qualified names share one precedence model
8. diagnostics explain missing schemes, missing roles, and invalid module paths honestly
9. the feature improves authoring UX without introducing a plugin/security problem or a second competing runtime model
10. scheme reuse follows the same patterns as other module reuse (import, pub let, pub component)
