# Language and Runtime Gaps: Next Steps

During the review and redesign of the Animatix showcase, several usability friction points, inconsistent language designs, and missing runtime capabilities were identified. This document captures these gaps to serve as a roadmap for future language and engine improvements.

## Active Gaps

### Gap #1: Text/Media Property Assignment

**Current State:** To morph text or change an `Svg` or `Image` source, the user must redeclare the entire object at a new keyframe (e.g., `tagline: Text, text: "New Text" [1s]`).
**The Gap:** This completely breaks the otherwise clean, consistent property assignment syntax used throughout the rest of the language (e.g., `node.color = red [1s]`).
**Next Steps:**
* Introduce standard property assignment for strings and media sources (`tagline.text = "New Text" [1s]`, `icon.url = "new.svg"`).
* Map these property assignments under the hood to the existing path-morphing and cross-fade systems.

**Implementation Status:** Infrastructure added for text property assignment (stores text content in timeline), but render-time text path compilation is deferred. The assignment `tagline.text = "New" [1s]` stores the value but does not yet trigger dynamic re-compilation of text paths at render time. Currently, text paths are compiled at declaration time only.

### Gap #2: Coordinate System Friction

**Current State:** Positioning is somewhat split between an absolute coordinate system via `at: (x, y)` (or percentages) and a layout-managed system via `anchor: scene.*` and `offset: (x, y)`.
**The Gap:** Mixing layout-anchored objects with absolutely placed items often requires significant manual tweaking and math, breaking the declarative layout promise.
**Next Steps:**
* Unify the coordinate alignment model.
* Allow layout containers to gracefully accept relative percentage coordinates without breaking internal auto-layout boundaries.

### Gap #3: Parser Leniency (Trailing Braces)

**Current State:** Several shipped demos contain stray trailing braces on inline component declarations (e.g., `legend_a: Text, text: "y = x^2 - 2" }`).
**The Gap:** While the parser gracefully ignores these trailing braces, it makes the declarative syntax look messy and confusing to learners who might think the braces are structurally required.
**Next Steps:**
* Tighten the Tree-sitter grammar and parser to either properly reject unmatched braces or formalize their usage.
* Audit and clean up all `examples/*.amx` files to ensure they represent the canonical, idiomatic syntax.

## Shipped Gaps

### Fade-out Action
**Shipped:** `fade-out` action that mirrors `fade-in` behavior. See phase completion notes.

### Primitive Rotation
**Shipped:** `angle` property for `Ellipse`, `Arc`, `RegularPolygon` primitives. See `examples/rotation_demo.amx`.

### Path Animation (Dynamic Points)
**Shipped:** Dynamic points property animation for `Polygon` primitives. See `examples/path_animation_demo.amx`.

**What works:**
- `poly.points = [(0,0), (100, 0), (50, 100)] [1s, ease: ease-in-out]` syntax
- Smooth point interpolation between different polygon shapes
- Morphing triangle ↔ square, pentagon ↔ star, or arbitrary point sets
- Runtime track support for `Vec<[f32; 2]>` with linear interpolation
- Integrates with existing action/assignment timing system

(End of file - total 73 lines)
