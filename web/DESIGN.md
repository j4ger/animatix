# Web design guide

Use this guide for showcase scenes and the pages that present them. Review the
actual animation, not just the page or a passing gate. The storyboard should
include the first frame, main cues and resting composition.

## Scene composition

1. **Open on a composed stage.** Show a backdrop, title or subject at time zero;
   animate entrances within it rather than fading up from an empty black frame.
2. **Keep a useful resting frame.** An `autoplay loop` cycle holds the final
   frame. Do not erase the cast in an outro; let the player handle the loop dissolve.
3. **Start motion promptly.** Avoid an unexplained frozen plate before the first
   reactive movement. This is a visual-review rule, not an automated timing test.
4. **Use the stage.** Aim for the subject to occupy at least 60% of a 1280×720
   frame, with margins around 8% or less where appropriate. Do not shrink a diagram
   into an empty field merely to avoid layout decisions.
5. **Make type readable at embed size.** At 1280×720, use act titles of at least
   56 px, captions 22 px, readouts 20 px and kickers 16 px. Reserve 12 px micro-labels
   for deliberate backdrop detail, not explanatory content.
6. **Use editorial asymmetry.** Left-anchored headings and bottom-left captions
   leave room for the diagram. Avoid repeating the same centered title-card stack.
7. **Give the animation a progression.** A useful starting structure is hook
   (0–20%), mechanism (20–75%) and assembled payoff (75–100%). Adapt it to the idea;
   do not add a decorative effect merely to fill the final act.
8. **Choose a palette per work.** Prefer a coherent colorscheme; older examples
   use `examples/lib/theme.amx`, but new work need not import that library. Glass should reveal useful content beneath it, not become
   the default container for every element.

Older shared scene building blocks live in `examples/lib/stage.amx` (`StageFloor`,
`ActHead`, `Caption`, `Readout`) and `examples/lib/ambience.amx` (`Ambience`). They are optional example resources, not the Astro site's
required design system. Reuse a suitable component when it serves the work.
Frame-time readouts can update text inside `always`; review them in both playback
and an offline storyboard.

## Pages

- Keep a dark ground with restrained radial washes and per-work accent color.
  Let live scenes carry the strongest contrast.
- Use display typography for headings, sans for body and mono for timecodes,
  paths and data. Keep hierarchy stronger than decorative chrome.
- Give major players room. Pair a full-width or asymmetric stage with concise
  explanatory text; avoid wrapping every paragraph in another card.
- Use a readable authored frame as the poster. In the Showcase and Learn,
  clicking the poster starts live playback. Keep it visible until the first
  engine frame is ready. Keep catalog covers static so browsing opens no GPU
  contexts.
- Offer the Showcase recording after live playback fails. Identify it as the
  authored-default recording and fetch the MP4 when the visitor chooses it.
  Keep the AMX source and Playground links available during recovery.
- Provide scrubbing where timing matters. Parameter sliders should expose a small
  number of meaningful build-time variables with authored defaults.

`data-vars` and `setVars()` rebuild through the existing variable mechanism,
with debouncing and playback-position preservation. They do not introduce a
second expression language. See [the player API](README.md#live-figure-editors)
for promise, cancellation and error behavior.

## Content ownership

Current work and lesson metadata lives in `web/app/src/lib/content.ts`; technique
metadata lives in `techniques.ts`. Canonical published AMX and media live under
`web/content/`. Update the Astro templates and their content metadata when
changing a public demonstration. Archive entries retain their separate source
links.

Keep developer probes under `web/dev/`, outside showcase navigation. Shared modules at `examples/lib/` are source dependencies for examples and
regression tests, not built-in engine content or a requirement for new work.

## Verification

`web/tools/site/site-rest-check.py` selects canonical `web/content/scenes/` directly,
not the retired HTML. It is a manual GPU check, not proof from a static build.
The historical thresholds below are heuristics, not a measure of artistic quality:

- Scenes marked `// house-style: <note>` require a resting coverage ratio of
  at least 0.80 and at least 5% whole-frame ink at time zero.
- Legacy scenes retain the 0.35 resting threshold until they are revised and
  opt in. The stricter threshold is not yet universal.
- Exemptions must be explained in the scene source. Whole-frame coverage cannot
  detect every disappearing label, overlap or clipped actor.

Before accepting a new or rewritten scene:

```sh
animatix check path/to/scene.amx
animatix audit path/to/scene.amx
animatix storyboard path/to/scene.amx
animatix baseline path/to/scene.amx
```

Inspect the storyboard and relevant frames yourself. Then run the applicable
content, site-scene and web gates from `tools/ci/ci.sh gates` in their listed
shells. See [web verification](README.md#verification-and-inspection-tools) and
[contributing](../docs/contributing.md). A build or smoke-test pass does not
establish visual quality or cross-platform rendering parity.
