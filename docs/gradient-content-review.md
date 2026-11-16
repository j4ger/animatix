# Gradient: mathematical and parameter review

This records the initial correction of the contour-card version. The current
learning-rate figure uses a shared loss-versus-iteration plot; see
[the redesigned comparison](comparison-layout.md). The mathematical and slider
contracts below still apply.

This revision retains all five source URLs and the existing deck. It changes
content and assembly, not the engine or optimizer semantics.

## The actual model

The acts use `L(x,y) = 2x² + y²`, with gradient `(4x, 2y)`. One discrete update is
`x[n+1] = (1 − 4η)x[n]`, `y[n+1] = (1 − 2η)y[n]`.

| Rate | Factors (x, y) | Correct description |
| --- | --- | --- |
| 0.10 | (0.6, 0.8) | Both coordinates shrink without changing sign. |
| 0.35 | (−0.4, 0.3) | x zigzags, but both coordinates converge. Overshoot is not divergence. |
| 0.50 | (−1, 0) | x alternates at constant magnitude; y becomes zero. A bounded two-cycle, not convergence or explosion. |

The convergence interval for this bowl is `0 < η < 0.50`. Rates above 0.50
expand the x component. The page's slider ends at 0.50, so it does not demonstrate
divergence. The fixed left panel is 0.02, not the old page's 0.05; the fixed right
is 0.35, not 1.05. The middle is the selected rate, default 0.10.

For the learning-rate act's start (3.4, 1.6), 24 steps give loss approximately
0.78327 at 0.02, 0.00005709 at 0.10, 1.83e−18 at 0.35, and 23.12 at 0.50.
“0.00” is rounded display, not proof of exact zero.

## What changed

- Page copy matches the actual model, comparison rates and stability boundary.
- The selected-rate verdict distinguishes convergence, zigzag and persistent
  oscillation. Captions no longer promise every slider choice reaches the basin.
- The surface act's straight line to the origin is labelled an illustrative
  probe, not the negative-gradient trajectory. Its initial-loss chip stays fixed.
- `scene.amx` is generated from the four standalone acts by
  `tools/content/gradient-plate.py`. Both generator drift and mathematical contracts
  run in `tools/content/content-sync.sh`.
- The full walkthrough is explicitly static (`data-param-static`). The engine
  does not propagate build-time overrides into composed scenes. Parameter rails
  exclude opted-out players; the page says sliders rebuild individual figures
  while the complete walkthrough uses defaults. No unsupported interactivity is
  implied and no engine behavior is silently changed.

## Verification scope

`python3 tools/content/check-gradient-content.py` checks recurrence against closed
form for all three rates, two actual initial points and 8/24/48 steps, plus source
coefficients, manifest/page bounds, retained source links and static opt-out
wiring. It is a numerical/source contract, not browser interaction evidence.

Native CLI checks and final frames at 7.5 seconds are rendered for rates 0.10,
0.35 and 0.50. Review must show the correct rate, path, verdict and loss; 0.50
must retain an off-center endpoint and loss 23.12. The composed plate is also
checked and sampled. These do not certify WebGPU playback, live browser slider
behavior or audio; those remain separate QA tasks.

A focused Node test also ran the actual parameter handler against a mocked DOM:
0.50 then 0.10 each dispatch to four eligible players and update the readout.
This tests event-handler logic, not actual custom elements or browser rendering.
The surface probe at 6 s was rendered after the final readout-label fix: current
loss 0.00, initial loss 29.16.
