# 1. Overview

Primitives are the basic building blocks of an Animatix scene. Each primitive defines a specific type of rendered object, such as a shape, text, or external asset. Primitives are declared using the actor declaration syntax.

**Syntax:**
```animatix
label: PrimitiveType, property: value, ...
```

---

# 2. Geometric Primitives

These primitives are rendered using GPU-accelerated techniques, leveraging Storage Buffers to process thousands of elements simultaneously (via SDF mathematical evaluation or Vertex meshes).

## Circle
**Description:** A perfect circle defined by center and radius.
**Properties:**
- `radius`: Number (length)
- `segments`: Number (optional, for tessellation)

**Example:**
```animatix
c: Circle, radius: 50, color: red
```

## Rect
**Description:** A rectangle defined by width and height.
**Properties:**
- `width`: Number (length)
- `height`: Number (length)
- `rounded`: Number (optional, corner radius)

**Example:**
```animatix
r: Rect, width: 100, height: 50, rounded: 5
```

## Line
**Description:** A straight line segment between two points in local space (relative to `position`).
**Properties:**
- `from`: Tuple (x, y)
- `to`: Tuple (x, y)
- `width`: Number (stroke width)
- `cap`: String (round, square, butt)

**Example:**
```animatix
l: Line, from: (0, 0), to: (100, 100), width: 2
```

## Path
**Description:** A custom shape defined by a sequence of points in local space (relative to `position`).
**Properties:**
- `points`: Array of Tuples
- `closed`: Boolean (whether to close the shape)

**Example:**
```animatix
p: Path, points: {(0,0), (50,100), (100,0)}, closed: true
```

## Polygon
**Description:** A regular polygon defined by side count and radius.
**Properties:**
- `sides`: Number (integer)
- `radius`: Number

**Example:**
```animatix
hex: Polygon, sides: 6, radius: 50
```

## Arc
**Description:** A partial circle defined by angles.
**Properties:**
- `radius`: Number
- `start_angle`: Number (degrees)
- `end_angle`: Number (degrees)

**Example:**
```animatix
a: Arc, radius: 50, start_angle: 0, end_angle: 90
```

## Ellipse
**Description:** A stretched circle defined by two radii.
**Properties:**
- `radius_x`: Number
- `radius_y`: Number

**Example:**
```animatix
e: Ellipse, radius_x: 100, radius_y: 50
```

---

# 3. Text Primitives

Text primitives use signed distance field (SDF) font atlases for crisp rendering at any scale.

## Text
**Description:** Standard rendered text string.
**Properties:**
- `content`: String
- `font`: String (font family name)
- `size`: Number (points or pixels)
- `weight`: String (normal, bold, light)

**Example:**
```animatix
t: Text, content: "Hello World", font: "Inter", size: 24pt
```

## MathTex
**Description:** LaTeX-style mathematical notation.
**Properties:**
- `content`: String (LaTeX syntax)
- `size`: Number

**Example:**
```animatix
eq: MathTex, content: "x^2 + 3", size: 18pt
```

## Code
**Description:** Monospaced text for code snippets.
**Properties:**
- `content`: String
- `language`: String (optional, for syntax highlighting)
- `size`: Number

**Example:**
```animatix
c: Code, content: "let x = 0", language: "rust", size: 12pt
```

---

# 4. External Assets

These primitives load external files from the project directory.

## Svg
**Description:** Scalable Vector Graphics file.
**Properties:**
- `url`: String (file path or URL to the SVG)
- `scale`: Number (optional)

**Example:**
```animatix
icon: Svg, url: "assets/icon.svg", scale: 1.5
```

## Image
**Description:** Raster image file (PNG, JPG).
**Properties:**
- `path`: String (file path)
- `size`: Tuple (optional, width, height)

**Example:**
```animatix
img: Image, path: "assets/photo.png", size: (400, 300)
```

---

# 5. Rendering Strategies

The engine automatically selects the best rendering strategy based on the primitive type. To allow for massive scene capacity and complex morphing, instance data for all strategies is uploaded to the GPU via **Storage Buffers**.

## SDF (Signed Distance Field)
- **Used for:** Circle, Rect, Ellipse, Arc, Text
- **Benefits:** Resolution independent, smooth edges, bypasses vertex attribute limits
- **Behavior:** Shapes are defined by mathematical formulas in the shader and rendered on tightly bounded quads, looking up instance configurations directly from the active Storage Buffer.

## Vertex (Mesh)
- **Used for:** Path, Polygon, Line, Svg, Image
- **Benefits:** Arbitrary shapes, supports complex geometry
- **Behavior:** Shapes are tessellated into vertices. Dynamic path data and point-matching transforms can also be evaluated via Storage Buffers to scale efficiently.

---

# 6. Common Properties

All primitives share the following standard properties.

- `position`: Tuple (x, y) - Center position in scene coordinates (Default: `(0, 0)`)
- `at`: Tuple (x, y) - Shorthand for `position` (Default: `(0, 0)`)
- `color`: Color (hex, name, or rgb) - Fill or stroke color (Default: `white`)
- `opacity`: Number (0.0 to 1.0) - Transparency level (Default: `1.0`)
- `scale`: Number or Tuple (sx, sy) - Uniform or non-uniform scaling (Default: `1.0`)
- `rotation`: Number (degrees) - Rotation around center (Default: `0`)
- `z_index`: Number - Layer order; higher values render on top (Default: `0`)

---

# 7. Multi-Strategy Morphing

Primitives support automatic morphing when re-declared at a new keyframe. Taking inspiration from advanced animation engines like Manim, Animatix utilizes a **Multi-Strategy** morphing algorithm determined automatically during scene compilation, which can be overridden by user modifiers.

## Strategies

### Parametric (`strategy: parametric`)
- **Behavior:** Smooth mathematical interpolation between compatible parameters (e.g., radius, width, corner rounding).
- **Compatible Types:** Circle to Rect, Rect to Rect, Circle to Rounded Rect.
- **Result:** Perfect geometric transitions with zero ghosting or artifacts executed in a single SDF shader pass.

### Point-Match (`strategy: point_match`)
- **Behavior:** The engine normalizes the vertex count between two shapes and linearly (or curved via `path_arc`) interpolates the point positions.
- **Compatible Types:** Path to Path, Polygon to Polygon. 
- **Auto-Tessellation:** If an SDF shape (like a Circle) morphs into a Path, the engine will automatically tessellate the Circle into a mesh and use Point-Matching to transition cleanly into the target Path.

### Fade Transform (`strategy: fade_transform`)
- **Behavior:** Scales and fades the source into the target simultaneously, handling completely mismatched geometries.
- **Compatible Types:** Any to Any.
- **Result:** Source shrinks/grows toward the target bounds while cross-fading.

### Cross-Fade (`strategy: cross_fade`)
- **Behavior:** Standard opacity cross-fade in-place where the source fades out while the target fades in.
- **Compatible Types:** Text to Shape, Image to Code, or Complex SVG to Simple Shape.

### Match Shapes (`strategy: match_shapes`)
- **Behavior:** Deconstructs compound shapes (like text or SVGs) and morphs geometrically matching sub-components.
- **Compatible Types:** Text to Text, SVG to SVG.

## Modifiers
You can tweak the morphing behavior using optional modifiers:
- `path_arc`: Number (radians) - Curves the trajectory of points during a morph instead of moving them in a straight line.
- `stretch`: Boolean - Whether the source stretches to fit the target bounds during `fade_transform` or `point_match` (Default: `true`).

**Example:**
```animatix
#0s
  shape: Circle, radius: 50, color: red, at: (0, 0)
#2s
  # Force point-match and curve the trajectory with path_arc
  shape: Polygon, sides: 5, at: (100, 100) [2s, strategy: point_match, path_arc: 3.14]
```

---

# 8. Syntax Examples

## Basic Shapes
```animatix
#0s
  c: Circle, radius: 50, color: blue, at: (50%, 50%)
  appear c [1s]
```

## Text and Math
```animatix
#0s
  title: Text, content: "Demo", size: 24pt, at: (50%, 90%)
  formula: MathTex, content: "E = mc^2", size: 18pt, at: (50%, 50%)
  appear {title, formula} [1s]
```

## Morphing with Modifiers
```animatix
#0s
  box: Rect, width: 50, height: 50, color: red, at: (10, 10)
#2s
  # Transform into text using a fade transform
  box: Text, content: "Done!", color: white, at: (50, 50) [1.5s, strategy: fade_transform, stretch: false]
```
