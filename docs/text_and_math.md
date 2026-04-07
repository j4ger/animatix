# Text and Math in Animatix

Animatix supports rich text rendering and mathematical equations powered by Typst and MSDF (Multi-channel Signed Distance Field) technology. This ensures sharp, high-quality text and math expressions at any scale and during any animation.

## Text Elements

You can create a text element in your scene using the `Text` block.

```amx
#0s
t1: Text { text: "Hello, Typst!", at: (100, 100), color: white }
```

### Properties
- `text`: A string containing the text to be rendered.
- `at` or `position`: A coordinate tuple `(x, y)` specifying where the text should be placed.
- `color`: The color of the text (e.g. `white`, `red`, `#FFFFFF`).

## Math Elements

Mathematical equations can be rendered cleanly using LaTeX-style syntax within a `Math` block.

```amx
#0s
m1: Math { math: "\\int_{-\\infty}^{\\infty} e^{-x^2} dx = \\sqrt{\\pi}", at: (400, 300), color: yellow }
```

### Properties
- `math`: A string containing the LaTeX-style math equation. Note that backslashes must be escaped in the string (`\\`).
- `at` or `position`: A coordinate tuple `(x, y)` specifying where the math should be placed.
- `color`: The color of the math expression.

## Rendering

Both `Text` and `Math` elements are processed via Typst to generate high-quality outlines, which are then compiled into MSDF font atlases. This allows them to participate seamlessly in the Animatix scene rendering pipeline.
