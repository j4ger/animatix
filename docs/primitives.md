# 1. Overview

Primitives are the basic building blocks of an Animatix scene. Each primitive defines a specific type of rendered object, such as a shape, text, or external asset. Primitives are declared using the actor declaration syntax.

**Syntax:**
```animatix
label: PrimitiveType, property: value, ...
```

---

# 2. Geometric Primitives

These primitives are rendered using GPU-accelerated techniques (SDF or Vertex buffers).

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
- `path`: String (file path)
- `scale`: Number (optional)

**Example:**
```animatix
icon: Svg, path: "assets/icon.svg", scale: 1.5
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

The engine automatically selects the best rendering strategy based on the primitive type.

## SDF (Signed Distance Field)
- **Used for:** Circle, Rect, Ellipse, Arc, Text
- **Benefits:** Resolution independent, smooth edges, GPU efficient
- **Behavior:** Shapes are defined by mathematical formulas in the shader

## Vertex (Mesh)
- **Used for:** Path, Polygon, Line, Svg, Image
- **Benefits:** Arbitrary shapes, supports complex geometry
- **Behavior:** Shapes are tessellated into vertices on CPU or GPU

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

# 7. Morphing Support

Primitives support automatic morphing when re-declared at a new keyframe.

## Compatible Morphs
- **Circle to Rect:** Uses SDF interpolation
- **Circle to Polygon:** Uses SDF or Vertex resampling
- **Text to Text:** Uses glyph matching
- **Path to Path:** Uses point-match morphing (Warning: Automatic vertex resampling between disparate paths may cause self-intersection or looping artifacts).

## Incompatible Morphs
- **Shape to Text:** Uses fade strategy
- **Complex Path to Simple Shape:** Uses fade strategy

## Strategy Override
Users can force a specific morph strategy using modifiers.

**Example:**
```animatix
#2s
shape: Rect, ... [2s, strategy: fade]
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

## SVG Icon
```animatix
#0s
  logo: Svg, path: "assets/logo.svg", at: (50%, 50%)
  appear logo [1s]
```

## Morphing Example
```animatix
#0s
  shape: Circle, radius: 50, color: red
#2s
  shape: Rect, width: 100, height: 100, color: red [2s]
```
