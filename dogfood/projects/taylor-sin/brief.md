# Taylor Series Approximation of sin(x)

## Goal

Explain Taylor-series approximation of `sin(x)` to first-year STEM students in
~22s: see the target curve, stack partial sums term by term, then sweep the
degree `n` from 1 to 13 continuously and watch the polynomial hug the sine.

## Scenes

1. `Title` — title card.
2. `Target` — `sin(x)` traced via `stroke_progress` inside a `Graph`.
3. `PartialSums` — S₁, S₃, S₅, S₇ appear one at a time with Typst labels.
4. `Sweep` — degree `n` driven by `t` inside the plot closure (step-gated
   terms); live `n = …` readout from an `always` block.
5. `Outro` — the full series formula.

## Constraints

- No assets, no audio; pure primitives + Typst.
- Deliberately exercises the plot pipeline (closures, func gates,
  `stroke_progress`), `always` reactive blocks, and multi-scene `play` chain —
  this project doubles as a language-expression probe for math explainers.

## Status

Render-verified via `animatix image` at t = 1.5, 5.0, 9.0, 10.5, 12.5, 14.0,
15.5, 17.5, 20.0 (see notes.md for the engine gaps this uncovered).
