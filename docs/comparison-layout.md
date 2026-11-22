# Learning-rate scene: compare outcomes on one shared axis

The three repeated contour cards were mathematically valid but visually heavy.
Their tall ellipses dominated the composition. The replacement borrows the
quiet navy, cyan/coral/gold palette and clear hierarchy of the phase experiment.
It changes the explanatory view, not the objective function or its geometry.

## New story

**How fast does loss fall?** All three rates start at (3.4, 1.6) and follow
gradient descent on `L(x,y)=2x²+y²`. A single wide figure plots loss remaining
against iteration. This answers the actual question directly: how quickly does
the selected learning rate reduce loss, and does it converge at all?

- Cyan is the fixed 0.02 reference; coral is the fixed 0.35 reference.
- Gold follows the user's `lr` parameter. All share the same normalized loss
  axis, from 0 to 100% of the common initial loss 25.68.
- Every marker is a true discrete iterate. Lines connect those markers for
  readability; they are not additional optimizer steps. The selected marker
  advances in discrete steps, not through a fabricated continuous trajectory.
- At 0.50 the remaining loss approaches 23.12 (about 90%), visibly separated
  from the convergent references. At 0.35, gold and coral coincide; the footer
  explicitly explains the overlap rather than artificially offsetting a curve.
- The right rail contains the selected rate, step, loss fraction and two short
  sentences interpreting coordinate behavior. “Zigzag in x” does not imply
  alternating loss: the loss still decreases.

The earlier surface, direction and trajectory acts remain available. This
closing act now supplies a complementary view instead of repeating their
contour diagrams three times. Existing source URLs and parameter names remain.

## Composition rules worth reusing

A 1280 × 720 frame uses 64 px margins. Header and legend lead into a single
wide chart (x112–864, y310–550). A divider at x920 separates a compact reading
rail starting at x968. Title 46 px, subtitle 22, legend 19, supporting labels
16–19. Selected rate is the only large numeric emphasis (52 px). The readout is a
short sentence: “After 24 updates, <0.1% remains.” Small positive loss is not
rounded to an apparent exact zero. Sentence-case outcome headings lead into
two short explanatory lines, replacing dashboard-style all-caps labels.

Prefer one visual question per scene, shared axes for comparisons, and direct
color association rather than repeated bordered cards. Keep text and plotted
evidence in separate bands. Use no camera movement to decorate a quantitative
comparison. These are authoring principles, not a new global component API.

### Align readouts to a shared edge

A fixed-width `Col` with `align: "end"` pins each value to its padded
right edge. Use `align: "start"` for labels that share a left edge.
A `Row` with `vertical_align: "baseline"` aligns a large value and its
smaller unit on a common text baseline.

```animatix
rates: Col, at: (1060, 410), size: (160, auto), padding: 12, gap: 10, align: "end", opacity: 1 {
  reference: Text, text: "0.02", font_size: 24, font_family: "mono", opacity: 1
  selected: Text, text: "0.350", font_size: 24, font_family: "mono", opacity: 1
}
loss: Row, at: (1060, 510), size: (auto, 72), gap: 6, vertical_align: "baseline", opacity: 1 {
  value: Text, text: "90", font_size: 52, opacity: 1
  unit: Text, text: "%", font_size: 24, opacity: 1
}
```

The column's right edge is at x=1140. Its 12 px padding puts both values'
right edges at x=1128. See both layouts in the runnable
[readout alignment example](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/examples/layout/readout_alignment.amx).

## Correctness and review

`check-gradient-content.py` retains the optimizer recurrence checks and now
checks normalization and discrete selected-step sampling. The old viewport
padding check belonged to the discarded contour-card view and is retired.
`gradient-plate.py` still generates the complete walkthrough from standalone
acts; no hand-maintained second copy was introduced.

Native images at 0.10, 0.35 and 0.50 plus a boundary-rate time storyboard are
the review targets. Browser/WASM interaction, audio and complete cross-platform
rendering are not implied by this native review. The complete plate still uses
defaults; slider overrides apply only to standalone figures.

Verified locally: native frames at all three selected rates, plus four time
samples at the boundary rate; final audit rendered four samples with 0 errors,
0 warnings, 49 informational findings and 2 skipped checks. Numerical/source
contracts, content synchronization, catalog validation, docs checks and the
Nix `fmt` gate passed. This is not a full workspace build or browser test.

## Showcase page integration

This section records the earlier gallery implementation. The current
[`/demos/gradient/`](https://amx.j4ger.dev/demos/gradient/) route is an
archive entry. See the [content catalog](content-catalog.md) for current
site sources.

The existing `/demos/gradient/` URL now leads with the learning-rate experiment.
The three supporting figures follow as a quiet reading sequence; the complete
four-act composition is an optional disclosure. All five source URLs and the
s0–s4 anchors remain. `works.json` now points the showcase/gallery preview at
`learningrate.amx`; generated listings were refreshed together.

The generated slider rail still drives the four standalone figures through
`deck.js`. Page-local presets dispatch the same input event, rather than adding
a second rebuild path. A live DOM explanation makes the selected rate readable
on narrow screens independently of the canvas. The fixed-default composition
remains excluded with `data-param-static`.

### Browser review and remaining limit

Actual cloud Chromium review verified the page typography, 0.10/0.35/0.50 preset
selection and explanation updates, keyboard step limits 8/48, and a 400 CSS-px
narrow viewport with no horizontal overflow (document width 388). It caught a
shared CSS selector overriding the title, corrected with a page-scoped selector.
This is UI verification, **not a passing animation/slider-rebuild test**.

The standard release SDK builds and verifies successfully. The dev SDK splitter
was killed with exit 137; optimizing the splitter and removing only debug
sections did not resolve it. The standard `wasm-release` build did, without a
production code change. The original Wasm shrank from about 559 MiB to 65 MiB.

Live animation remained unverified in that review. The managed browser reported no WebGPU
adapter. Dedicated software-rendering profiles either have no adapter or fail
shared-image presentation. The test profiles were closed afterward. Native
scene renders are not substituted for a browser pass. If an interactive
comparison returns, verify preset and slider playback with a working WebGPU
adapter.
