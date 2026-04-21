# Extensible Colorscheme Design for Animatix

> **Status: future-extension / design-only document**
>
> Built-in colorschemes v1 are already shipped: `config { colorscheme: ... }`, semantic color aliases through `color` / `stroke`, and `color: auto` are part of the current runtime contract. This document now owns only the broader future design for external/loadable schemes, inheritance, and related extension work. For current runtime behavior, treat `docs/spec.md` and `docs/primitives.md` as the source of truth.
>
> Scoping note: this design is intentionally **data-first and runtime/DSL-first**. It does not assume a property inspector, a theme marketplace, remote loading, or executable plugins.

This document turns the remaining colorscheme follow-up discussion into a concrete product and architecture proposal.

The guiding decision is simple:

- **colorschemes should be reusable and loadable**
- **explicit actor colors must remain the final authority**
- **extensibility should be declarative data, not executable code**

Animatix should make it easier for users to get a coherent palette and distinct actor colors by default, without taking away the current direct `color:` workflow.

---

## 1. Goals

The colorscheme system should accomplish six things:

1. reduce repeated palette boilerplate across `.amx` files
2. let a scene select a built-in or user-provided colorscheme declaratively
3. support automatic distinct color assignment for actor-like nodes when the author opts in
4. preserve explicit `color`, `stroke`, and assignment-based overrides exactly as first-class author intent
5. stay deterministic and portable across CLI render, GUI preview, image export, and video export
6. fit the current load-time / timeline-build architecture without inventing a plugin runtime

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

The first useful version should feel like a palette and actor-color convenience layer, not a second rendering language.

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

Animatix ships with a small built-in set such as:

- `default-dark`
- `default-light`
- `editorial-dark`

These give the feature immediate value and define the reference schema.

### B. Loadable user schemes

Users can later load data-only scheme files from disk.

From the authoring perspective, built-in and loaded schemes should behave the same. The only difference is where the data came from.

### C. Explicit per-actor override remains final

If an actor has an explicit `color:` or a later `node.color = ...` assignment, that should still win.

This preserves the current DSL mental model and makes the new system safe to adopt incrementally.

---

## 6. Proposed Surface Direction

The colorscheme system should introduce one scene-level selection surface and one actor-level role surface.

### 6.1 Scene-Level Selection

Preferred syntax direction:

```animatix
config {
  colorscheme: "editorial-dark"
}
```

For user-provided files:

```animatix
config {
  colorscheme: "./themes/brand_ocean.amx"
}
```

Why `config`:

- colorscheme selection is a document-level authoring choice, not a timed animation property
- it fits the current use of `config` for document-wide concerns such as resolution
- it keeps the feature load-time oriented instead of frame-time magical

### 6.2 Alias-Based Color Properties

Preferred syntax direction:

```animatix
title: Text { text: "Animatix", color: text.primary, anchor: scene.top, offset: (0, 80) }
subtitle: Text { text: "Palette-driven stage", color: text.secondary, anchor: scene.top, offset: (0, 118) }

alice: Circle, radius: 20, color: auto
bob: Circle, radius: 20, color: auto
carol: Circle, radius: 20, color: auto

warning: Rect, size: (280, 120), color: accent.warning
```

This keeps colorscheme use explicit through the existing `color:` surface instead of introducing a second declaration property.

### 6.3 Stroke Aliases

The same model should apply to stroke-style surfaces:

```animatix
axis: Line, from: (-120, 0), to: (120, 0), stroke: stroke.default
```

### 6.4 Explicit Color Still Works

```animatix
alice: Circle, radius: 20, color: auto
bob: Circle, radius: 20, color: (1.0, 0.4, 0.5, 1.0)
```

In this example, `alice` receives an automatically assigned scheme color while `bob` uses the explicit authored color.

---

## 7. Proposed Scheme File Shape

The first extensible format should be **native AMX syntax**, reusing the existing parser and expression system. This avoids introducing new dependencies (like RON) and keeps the authoring experience consistent.

A colorscheme is a special primitive declaration that defines a reusable palette.

### 7.1 Minimal Scheme Shape

Illustrative direction:

```animatix
// themes/editorial-dark.amx

Colorscheme {
  name: "editorial-dark",
  extends: "default-dark",
  
  // Semantic tokens
  scene.background: (0.04, 0.06, 0.10, 1.0),
  text.primary: (0.97, 0.98, 1.0, 1.0),
  text.secondary: (0.73, 0.80, 0.89, 1.0),
  surface.primary: (0.11, 0.16, 0.24, 1.0),
  surface.secondary: (0.17, 0.22, 0.3, 1.0),
  accent.primary: (0.38, 0.78, 1.0, 1.0),
  accent.success: (0.35, 0.86, 0.63, 1.0),
  accent.warning: (0.98, 0.83, 0.44, 1.0),
  accent.danger: (1.0, 0.46, 0.54, 1.0),
  stroke.default: (0.97, 0.98, 1.0, 1.0),
  
  // Auto color pool for distinct actor assignment
  auto: [
    (0.38, 0.78, 1.0, 1.0),
    (0.35, 0.86, 0.63, 1.0),
    (1.0, 0.46, 0.54, 1.0),
    (0.98, 0.83, 0.44, 1.0),
  ]
}
```

### 7.2 Why Native AMX Syntax

Using the existing AMX grammar provides several advantages:

- **No new dependencies**: No RON parser, serde, or additional crates needed
- **Consistent authoring experience**: Users already know AMX property syntax
- **Existing tooling support**: The parser, diagnostics, and syntax highlighting work out of the box
- **Expression support**: Colors can use variables, math, or other AMX expressions if needed
- **Comments and formatting**: Standard AMX comments work naturally

The `Colorscheme` primitive is a special non-rendered declaration (similar to how `let` defines variables). It is processed at build time to populate the `ResolvedColorscheme` structure.

### 7.3 Grammar

```
colorscheme_decl := "Colorscheme" "{" property* "}"
```

Properties are standard AMX `name: value` pairs. The following properties have special meaning:

| Property | Required | Description |
|----------|----------|-------------|
| `name` | Yes | Scheme identifier for reference |
| `extends` | No | Parent scheme to inherit from |
| `auto` | No | Array of colors for `color: auto` assignment |

All other properties are treated as semantic color tokens. Token names use dot notation (e.g., `text.primary`) to create namespaced aliases.

### 7.4 Inheritance via `extends`

When `extends` is specified, the loader:

1. Loads the parent scheme (built-in or file)
2. Merges parent properties with child properties (child wins)
3. Resolves the merged result into a `ResolvedColorscheme`

This is a load-time merge, not a runtime lookup chain.

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

### Canonical Order

From lowest to highest priority:

1. runtime hardcoded property default
2. selected colorscheme defaults
3. alias-based declaration defaults through `color` / `stroke`
4. explicit declaration values such as `color`, `stroke`, `stroke_color`, `scene.background_color`
5. later timed assignments
6. frame-local reactive overrides

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

### 10.2 Built-In and External Sources

The loader should support two sources:

1. built-in scheme names (resolved from hardcoded `BuiltInColorscheme` enum)
2. file-backed scheme documents (parsed as AMX `Colorscheme` declarations)

Both should resolve into the same in-memory `ResolvedColorscheme` shape before timeline construction uses them.

### 10.3 Resolution Flow

```
config.colorscheme: "./themes/brand_ocean.amx"
  -> Load file via ModuleGraph (reuse existing file loading)
  -> Parse as AMX AST (reuse existing parser)
  -> Extract Colorscheme declaration
  -> If extends: recursively resolve parent
  -> Merge properties (child overrides parent)
  -> Build ResolvedColorscheme
  -> Seed environment
  -> Apply to timeline
```

### 10.4 Extends / Inheritance

`extends` is worth supporting because it keeps external schemes small and encourages semantic reuse.

Examples:

- a project scheme extending `default-dark`
- a light variant extending a branded base scheme

Because this is a load-time graph, cycle detection should be explicit and diagnostic-backed. The existing `ModuleGraph` cycle detection (`visiting: HashSet<PathBuf>`) can be reused.

---

## 11. Diagnostics

This system should fail honestly and softly.

### Required diagnostics

1. **Unknown colorscheme**
   - selected built-in name not found
2. **Colorscheme file load failure**
   - path missing / unreadable
3. **Invalid colorscheme data**
   - malformed file, missing required `name`, or invalid RGBA tuple
4. **Colorscheme inheritance cycle**
   - `extends` graph loops
5. **Unknown alias token**
   - `color: accent.branding` when the resolved scheme has no such token
6. **Empty auto-assignment pool**
   - `color: auto` used but no auto-assignment colors exist after resolution

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

### 12.2 Environment Seeding

The environment may also expose resolved scheme values under dotted names for expression use later.

Illustrative direction:

```text
scheme.scene.background
scheme.text.primary
scheme.accent.warning
```

But this should be treated as an additional convenience, not the core dependency of the first implementation slice.

The first slice should land clean alias-backed defaults before broadening expression lookup surface.

### 12.3 Scene Background Seeding

If a scheme is selected and the scene omits explicit `scene.background_color`, the loader/build phase may seed the background track from `scene.background`.

This should remain a simple default, not a second background system.

---

## 13. Examples of Intended Authoring Direction

### 13.1 Minimal Built-In Scheme

```animatix
config {
  colorscheme: "editorial-dark"
}

title: Text { text: "Animatix", color: text.primary, anchor: scene.top, offset: (0, 80) }
subtitle: Text { text: "One selected scheme, minimal color boilerplate", color: text.secondary, anchor: scene.top, offset: (0, 116) }

left: Circle, radius: 22, color: auto, at: (520, 360)
right: Circle, radius: 22, color: auto, at: (760, 360)
```

### 13.2 Explicit Override

```animatix
config {
  colorscheme: "editorial-dark"
}

left: Circle, radius: 22, color: auto, at: (520, 360)
right: Circle, radius: 22, color: (1.0, 0.9, 0.2, 1.0), at: (760, 360)
```

### 13.3 Loadable Scheme File

Scene file:

```animatix
config {
  colorscheme: "./themes/brand_ocean.amx"
}

headline: Text { text: "Branded stage", color: text.primary, anchor: scene.top, offset: (0, 88) }
panel: Rect, size: (300, 160), color: surface.primary, at: (640, 360)
```

Scheme file (`themes/brand_ocean.amx`):

```animatix
Colorscheme {
  name: "brand-ocean",
  extends: "default-dark",
  
  scene.background: (0.02, 0.05, 0.08, 1.0),
  text.primary: (0.95, 0.97, 1.0, 1.0),
  text.secondary: (0.6, 0.75, 0.9, 1.0),
  surface.primary: (0.08, 0.15, 0.22, 1.0),
  accent.primary: (0.2, 0.6, 0.9, 1.0),
  accent.success: (0.3, 0.8, 0.5, 1.0),
  accent.warning: (0.95, 0.7, 0.2, 1.0),
  accent.danger: (0.9, 0.3, 0.4, 1.0),
  stroke.default: (0.8, 0.9, 1.0, 1.0),
  
  auto: [
    (0.2, 0.6, 0.9, 1.0),
    (0.3, 0.8, 0.5, 1.0),
    (0.9, 0.3, 0.4, 1.0),
    (0.95, 0.7, 0.2, 1.0),
  ]
}
```

### 13.4 Scheme Without Inheritance

```animatix
Colorscheme {
  name: "high-contrast",
  
  scene.background: (0.0, 0.0, 0.0, 1.0),
  text.primary: (1.0, 1.0, 1.0, 1.0),
  text.secondary: (0.8, 0.8, 0.8, 1.0),
  surface.primary: (0.2, 0.2, 0.2, 1.0),
  accent.primary: (1.0, 1.0, 0.0, 1.0),
  stroke.default: (1.0, 1.0, 1.0, 1.0),
  
  auto: [
    (1.0, 0.0, 0.0, 1.0),
    (0.0, 1.0, 0.0, 1.0),
    (0.0, 0.0, 1.0, 1.0),
    (1.0, 1.0, 0.0, 1.0),
  ]
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

### Slice 4 — External loadable schemes *(active future work)*

- add `Colorscheme` primitive to AST and parser
- add file-backed scheme loading via `config.colorscheme: "path.amx"`
- add `extends` inheritance with cycle detection
- add diagnostics for load failures, invalid data, and cycles
- reuse existing `ModuleGraph` file loading infrastructure

### Slice 5 — Broader expression exposure and GUI follow-up *(future follow-up)*

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
- introducing new file formats or dependencies when the existing AMX grammar suffices

---

## 16. Success Criteria

The colorscheme design is successful when:

1. a user can select one scheme and remove most repeated palette boilerplate from a normal scene
2. actor-like nodes can get distinct deterministic colors through an explicit opt-in surface
3. explicit actor colors and timed overrides still work exactly as users expect today
4. built-in and external schemes share one schema and one precedence model
5. diagnostics explain missing schemes, missing roles, and invalid inheritance honestly
6. the feature improves authoring UX without introducing a plugin/security problem or a second competing runtime model
7. colorscheme files use the same grammar as the rest of the language, keeping the authoring experience consistent

(End of file - total 556 lines)
