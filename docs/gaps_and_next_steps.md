# Language and Runtime Gaps: Next Steps

During the review and redesign of the Animatix showcase, several usability friction points, inconsistent language designs, and missing runtime capabilities were identified. This document captures these gaps to serve as a roadmap for future language and engine improvements.

## 1. Re-declaration for Morphing and Media Changes
**Current State:** To morph text or change an `Svg` or `Image` source, the user must redeclare the entire object at a new keyframe (e.g., `tagline: Text, text: "New Text" [1s]`). 
**The Gap:** This completely breaks the otherwise clean, consistent property assignment syntax used throughout the rest of the language (e.g., `node.color = red [1s]`).
**Next Steps:** 
* Introduce standard property assignment for strings and media sources (`tagline.text = "New Text" [1s]`, `icon.url = "new.svg"`).
* Map these property assignments under the hood to the existing path-morphing and cross-fade systems.

## 2. Asymmetrical Reveal and Exit Actions
**Current State:** The language ships with a standard `fade-in` action, but explicitly omits a dedicated `fade-out`. The supported exit actions (`wipe-out`, `reveal-out`, `draw-out`) do not provide a standard cross-fade out.
**The Gap:** Users are forced to rely on `reveal-out` or manually target the `fill_opacity` and `stroke_progress` properties for a simple fade exit, leading to asymmetrical entrance and exit syntax.
**Next Steps:**
* Implement a native `fade-out` action that perfectly mirrors `fade-in`.
* Ensure container-level reveals and exits cascade consistently to children.

## 3. Static Geometry and Path Animation
**Current State:** Geometry inputs like `Polygon.points` and `Path.commands` are restricted to declaration-time only.
**The Gap:** In a declarative animation tool, the inability to animate path points directly is a noticeable constraint that limits advanced vector graphics and custom morphing capabilities.
**Next Steps:**
* Add runtime track support for arrays of points and path commands.
* Allow users to write `poly.points = [(0,0), (50, 100), (-50, 100)] [1s, ease: ease-in-out]`.

## 4. Missing Primitive Transformations (Rotation)
**Current State:** Basic geometry shapes like `Ellipse` are documented as "Axis-aligned only" and do not support a native rotation parameter.
**The Gap:** This limits fundamental shape manipulation without relying strictly on verb-based matrix actions (`rotate node`), which creates a disconnect between declaration-time geometry and animated geometry.
**Next Steps:**
* Add a `rotation` (or `angle`) property to standard primitives like `Ellipse`, `Rect`, and `Polygon`.
* Ensure rotation assignments seamlessly interoperate with the existing `rotate` action.

## 5. Coordinate System Friction (`at` vs. `anchor`)
**Current State:** Positioning is somewhat split between an absolute coordinate system via `at: (x, y)` (or percentages) and a layout-managed system via `anchor: scene.*` and `offset: (x, y)`.
**The Gap:** Mixing layout-anchored objects with absolutely placed items often requires significant manual tweaking and math, breaking the declarative layout promise.
**Next Steps:**
* Unify the coordinate alignment model.
* Allow layout containers to gracefully accept relative percentage coordinates without breaking internal auto-layout boundaries.

## 6. Parser Leniency and Example Cleanup (Trailing Braces)
**Current State:** Several shipped demos contain stray trailing braces on inline component declarations (e.g., `legend_a: Text, text: "y = x^2 - 2" }`).
**The Gap:** While the parser gracefully ignores these trailing braces, it makes the declarative syntax look messy and confusing to learners who might think the braces are structurally required.
**Next Steps:**
* Tighten the Tree-sitter grammar and parser to either properly reject unmatched braces or formalize their usage.
* Audit and clean up all `examples/*.amx` files to ensure they represent the canonical, idiomatic syntax.