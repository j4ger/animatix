# Kurbo Shape Morphing

## Overview

The kurbo_shapes module provides seamless integration of parametric geometric shapes with the existing BezPath morphing system. This enables smooth animations between different shape types (Circle, Rectangle, Ellipse, Line, Arc, RoundedRect).

## Supported Shapes

### Circle
A perfect circle defined by center point and radius.

```rust
use animatix::timeline::kurbo_shapes::*;
use kurbo::Point;

let circle = KurboShape_::circle(Point::new(50.0, 50.0), 30.0);
// or with coordinates directly:
let circle = KurboShape_::circle_xy(50.0, 50.0, 30.0);
```

### Rectangle
Axis-aligned rectangle defined by min and max coordinates.

```rust
let rect = KurboShape_::rect(0.0, 0.0, 100.0, 100.0);
// Creates rectangle from (0,0) to (100,100)
```

### RoundedRect
Rectangle with rounded corners. Supports both uniform and per-corner radii.

```rust
// Uniform corner radius
let rounded = KurboShape_::rounded_rect(0.0, 0.0, 100.0, 100.0, 20.0);

// Per-corner radii (top-left, top-right, bottom-right, bottom-left)
let custom = KurboShape_::rounded_rect_radii(0.0, 0.0, 100.0, 100.0, (10.0, 15.0, 20.0, 25.0));
```

### Line
Simple line segment between two points.

```rust
let line = KurboShape_::line_xy(0.0, 0.0, 100.0, 100.0);
// Creates line from (0,0) to (100,100)
```

### Ellipse
Parametric ellipse with center, radii, and rotation angle.

```rust
use kurbo::Vec2;

let ellipse = KurboShape_::ellipse_xy(50.0, 50.0, 40.0, 20.0, 0.0);
// Center at (50,50), X-radius 40, Y-radius 20, no rotation
```

### Arc
Elliptical arc with sweep angle and rotation.

```rust
use std::f64::consts::PI;

let arc = KurboShape_::arc_xy(
    50.0,           // center x
    50.0,           // center y
    30.0,           // x-radius
    30.0,           // y-radius
    0.0,            // start angle
    PI / 2.0,       // sweep angle (90 degrees)
    0.0             // rotation
);
```

## Shape Morphing

### Basic Morphing

Morph from one shape to another using the `morph_kurbo_shapes` function:

```rust
use animatix::timeline::kurbo_shapes::*;
use kurbo::Point;

let from_shape = KurboShape_::circle(Point::new(50.0, 50.0), 30.0);
let to_shape = KurboShape_::rect(20.0, 20.0, 80.0, 80.0);

// Morph at 50% (midpoint)
let morphed = morph_kurbo_shapes_default(&from_shape, &to_shape, 0.5);
```

### Controlling Path Complexity

Use the tolerance parameter to control how curves are approximated to Bezier segments:

```rust
// Loose tolerance (fewer segments, faster rendering)
let path_loose = from_shape.to_path(0.5);

// Tight tolerance (more segments, smoother curves)
let path_tight = from_shape.to_path(0.01);

// Default tolerance (0.1 - recommended for UI)
let path_default = from_shape.to_path_default();
```

**Tolerance Guidelines:**
- **0.5-1.0**: Coarse approximation, suitable for low-res rendering
- **0.1**: Default, good balance for most UI animations
- **0.01-0.05**: High precision, use for high-quality output

### Animation Sequence

```rust
let circle = KurboShape_::circle(Point::new(50.0, 50.0), 30.0);
let rect = KurboShape_::rect(20.0, 20.0, 80.0, 80.0);

// Create animation frames
for t in (0..=100).map(|i| i as f64 / 100.0) {
    let frame = morph_kurbo_shapes_default(&circle, &rect, t);
    // Render frame...
}
```

## Integration with Timeline

Currently, the kurbo_shapes module provides the low-level morphing functionality. To integrate shapes into the timeline DSL, you would use the shapes in Rust code:

```rust
use animatix::timeline::kurbo_shapes::*;
use kurbo::Point;

fn create_shape_animation() {
    let circle = KurboShape_::circle(Point::new(960.0, 540.0), 50.0);
    let square = KurboShape_::rect(910.0, 490.0, 1010.0, 590.0);
    
    // Use morph_kurbo_shapes to create morphing animations
    for frame in 0..=60 {
        let t = frame as f64 / 60.0;
        let morphed = morph_kurbo_shapes_default(&circle, &square, t);
        // Render morphed path
    }
}
```

## How It Works

1. **Shape Creation**: Define shapes using builders (circle_xy, rect, etc.)
2. **Path Conversion**: Convert shape to BezPath using `to_path(tolerance)`
3. **Path Alignment**: Morph system aligns source and target paths at multiple levels:
   - List alignment (different number of subpaths)
   - Subpath alignment (different segment counts within each subpath)
   - Segment type alignment (convert lines to curves for uniform morphing)
4. **Interpolation**: Per-element linear interpolation of path coordinates

## Example: Circle → Rectangle Morph

```rust
use animatix::timeline::kurbo_shapes::*;
use kurbo::Point;

let start = KurboShape_::circle(Point::new(100.0, 100.0), 50.0);
let end = KurboShape_::rect(50.0, 50.0, 150.0, 150.0);

// t=0.0 → pure circle
// t=0.5 → halfway morph
// t=1.0 → pure rectangle
let midpoint = morph_kurbo_shapes_default(&start, &end, 0.5);
```

## Performance Considerations

- **Tolerance impact**: Looser tolerance = fewer segments = faster rendering
- **Shape complexity**: Circles/ellipses convert to multiple curve segments; rects are simpler
- **Morphing cost**: Independent of shapes' path complexity; proportional to aligned segment count

## Testing

Run the included tests to verify functionality:

```bash
cargo test --lib timeline::kurbo_shapes
```

Run the example demonstration:

```bash
cargo run --example kurbo_shape_morphing
```

## API Reference

### KurboShape_ Enum

```rust
pub enum KurboShape_ {
    Circle { center: Point, radius: f64 },
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
    RectUniform { x0: f64, y0: f64, x1: f64, y1: f64, radius: f64 },
    RectRadii { x0: f64, y0: f64, x1: f64, y1: f64, radii: (f64, f64, f64, f64) },
    Line { p0: Point, p1: Point },
    Ellipse { center: Point, radii: Vec2, rotation: f64 },
    Arc { center: Point, radii: Vec2, start_angle: f64, sweep_angle: f64, rotation: f64 },
}
```

### Key Functions

- `shape.to_path(tolerance: f64) -> BezPath` - Convert shape to path with specified tolerance
- `shape.to_path_default() -> BezPath` - Convert using default tolerance (0.1)
- `morph_kurbo_shapes(from, to, t, tolerance) -> BezPath` - Morph at parameter t
- `morph_kurbo_shapes_default(from, to, t) -> BezPath` - Morph using default tolerance

## Limitations & Future Work

### Current Limitations
- Shapes must be converted to BezPath before morphing (not live parametric morphing)
- No built-in shape-specific morphing (always goes through BezPath alignment)

### Future Enhancements
- Direct shape morphing (e.g., Circle → Circle with radius interpolation)
- Shape morphing in timeline DSL (need AST extensions)
- Advanced morphing strategies (feature-based, skeleton-based)
- Shape keyframe tracks in Animation system

## See Also

- [Morph Module](../src/timeline/morph.rs) - Low-level path morphing
- [Kurbo Documentation](https://docs.rs/kurbo/0.13.0) - Shape geometry primitives
