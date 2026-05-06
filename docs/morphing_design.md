# Vector Morphing Design

Morphing in Animatix is **re-declaration based**: re-declaring an actor at a later keyframe with different geometry triggers automatic path interpolation.

```animatix
#0s
circle: Circle, radius: 50, at: (0, 0)

#2s
circle: Circle, radius: 100, at: (100, 100) [2s, strategy: match]
```

**Shipped modifiers** (timed path-morphing re-declarations only):

| Modifier | Effect |
|----------|--------|
| `strategy: auto` | Match subpaths by index (default) |
| `strategy: match` | Match subpaths by centroid sort |
| `path_arc: N` | Curved interpolation via quadratic Bezier arc |
| `stretch: true` | Normalize to unit bounds before interpolating |

`strategy: fade` is deferred (requires compositing architecture change).

---

## Pipeline

Morphing runs at 4 levels, all in `crates/animatix/src/timeline/morph.rs`:

1. **List alignment** (`align_path_lists`) — match path count between source/target lists. Extra paths become degenerate single-point paths.
2. **Subpath alignment** (`align_subpaths`) — match subpath count within each path. Uses centroid sorting when `strategy: match`.
3. **Segment alignment** (`align_segments`) — equalize segment count by splitting longest cubic Beziers until counts match.
4. **Interpolation** (`morph_paths_with_options`) — lerp points with optional arc curvature and bounds normalization.

**Segment conversion:** `LineTo` and `QuadTo` are converted to `CurveTo` during alignment so all segments have the same structure for interpolation.

---

## Internal Representation

Paths are stored as standard property tracks, not a dedicated morph type:

- **Text/Math/Code:** `PropertyTrack<Vec<TextPath>>` (compiled from Typst/LaTeX/source)
- **Shapes/Vector:** `PropertyTrack<Vec<VelloPath>>` (kurbo `BezPath` + fill/stroke)

Interpolation delegates to `interpolate_text_paths` / `interpolate_vello_paths` in `timeline/track.rs`, which call `morph_paths_with_options` from `timeline/morph.rs`.

---

## Future

- `strategy: fade` — cross-fade between states (needs compositing)
- `morph_to()` method syntax — **not planned**. Re-declaration with modifiers is the intended surface.
- Multi-strategy morph selection beyond `auto`/`match` — speculative
