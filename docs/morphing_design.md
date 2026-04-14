# Vector Morphing Design for Animatix

> **Status: planned / design-only document**
>
> This file describes future-facing morph APIs and strategy controls, not the shipped DSL contract. For current runtime behavior, use `docs/spec.md` and `docs/primitives.md` as the source of truth.

## 1. Syntax for Morphing

In the Animatix language, morphing should be expressed as a high-level animation action rather than a simple property assignment. While property animations (e.g., `scene.item.paths = scene.item2.paths`) are intuitive for scalars and vectors, path morphing is structurally complex and often requires additional parameters (like mapping strategies or alignment hints).

**Status: Planned/Proposed API**

**Proposed Syntax:**
```animatix
// Action-oriented approach (Recommended)
scene.text1.morph_to(scene.text2) ease-in-out 1s

// Alternatively, a morph function in a property assignment
scene.text1.path = morph(scene.text2.path) ease-in-out 1s
```
The `morph_to` method is cleaner and explicitly communicates that a complex transformation is occurring between the two elements' vector representations, automatically animating their `kurbo::BezPath` data.

## 2. Handling Mismatched Path Lengths and Subpath Counts

When morphing between shapes like "A" (which has 2 subpaths: the outer outline and the inner triangle) and "BCD" (which has multiple subpaths for multiple letters and holes), the paths will rarely match 1-to-1 in subpath count or segment count.

**Subpath Count Mismatch:**
*   If the source has fewer subpaths than the destination, we must generate "degenerate" subpaths (zero-area subpaths that consist of a single point, usually placed at the centroid of the destination shape or merged into the nearest existing subpath) that expand into the new subpaths.
*   If the source has more subpaths, the extra subpaths must collapse into degenerate points (e.g., scaling down to the centroid of the nearest destination subpath) and fade out.

**Segment Count Mismatch:**
*   Once subpaths are matched, their internal segment counts must be equalized.
*   If Subpath A has 5 segments and Subpath B has 10 segments, we dynamically split the segments in Subpath A using `kurbo`'s curve splitting logic (e.g., splitting a cubic Bezier at `t = 0.5`) until both subpaths have exactly 10 segments.

## 3. Interpolating `kurbo::BezPath` Segments

`kurbo::BezPath` is composed of `PathEl` segments (`MoveTo`, `LineTo`, `QuadTo`, `CurveTo`, `ClosePath`). To interpolate between them, we must ensure both paths have the exact same sequence of element types.

**Normalization:**
*   Convert all lines (`LineTo`) and quadratic curves (`QuadTo`) into cubic bezier curves (`CurveTo`). This ensures that every segment (except `MoveTo` and `ClosePath`) is a `CurveTo`, simplifying interpolation.
*   For example, a `LineTo(p1)` becomes a `CurveTo` where the control points are collinear with the start and end points.

**Interpolation:**
*   Once normalized and matched in length, interpolation is a straightforward linear interpolation (Lerp) of the points within the `PathEl`.
*   For a given animation progress `t` (from 0.0 to 1.0, after easing is applied):
    *   `MoveTo(p0)` morphs to `MoveTo(p1)` by lerping the X and Y coordinates.
    *   `CurveTo(c1_src, c2_src, p_src)` morphs to `CurveTo(c1_dst, c2_dst, p_dst)` by lerping all three coordinate pairs.

## 4. Representation in `AnimationTrack`

In the engine, this will be represented by extending the `AnimationTrack` system to support complex types, specifically a track for `kurbo::BezPath`.

*   **Track Type:** We will introduce a `PathAnimationTrack` (or `AnimationTrack<BezPath>`).
*   **Keyframes:** The track will hold keyframes where the value is a normalized `BezPath`.
*   **Evaluation:** The `evaluate(t)` function of this track will:
    1. Find the adjacent keyframes based on the current time.
    2. Apply the easing function to calculate the interpolation factor `f`.
    3. Zip the normalized `PathEl` iterators from both keyframe paths.
    4. Lerp the coordinates of each paired `PathEl` and collect them into a new `BezPath`.
*   **Caching:** Because normalizing and equalizing segment counts is computationally heavy, the `PathAnimationTrack` should pre-process and cache the matched, converted `CurveTo`-only `BezPath` representations when the track is first created or compiled, ensuring `evaluate(t)` remains fast during playback.
