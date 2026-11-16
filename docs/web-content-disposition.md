# Earlier Web studies: disposition

The current website is curated, not a directory listing of all AMX files. A
legacy file remaining in Git is not an endorsement of its explanation or every
parameter setting.

`web/app/src/lib/legacy-works.ts` records the disposition of all nine entries in
the former [retired work manifest](https://github.com/j4ger/animatix/blob/530b5f8843d31a77b19540ff8f42daf662fddfff/web/works.json). Astro generates their `/demos/<id>/` compatibility
pages and `/archive/` from that list. Those pages explain where the work went;
they do not automatically load the old player. `/demos/` points to the current
collection, learning route and archive instead of reproducing the old catalog.

| Earlier work | Disposition | Reason |
| --- | --- | --- |
| Gradient | Source study | Preserve the revised loss experiment without requiring a second flagship on the same launch. |
| Matrix | Focused replacement | Separate one spatial relationship from the former multi-topic deck. |
| Sorting | Event-driven replacement | Counters and narration must follow actual algorithm events. |
| Epicycles | Archived source | Keep the five chapters and generated full scene together; do not publish a formula inconsistent with the drawing. |
| Hash | Retired public demo | Parameter-dependent lookup and collision claims need a new model before publication. |
| Transformer | Archived source | A full architecture deck is not a substitute for a carefully scoped explanation. |
| Topology | Retired public demo | Illustrative metrics and election effects are not a protocol simulation. |
| Morph | Technique material | The original comparison is a technique study; its internal arc variable is not a working public control. |
| Kinetic | Replaced | The new typography story communicates a specific idea rather than listing effects. |

## Evidence and limits

The initial review sampled six native frames from each of the eight non-Gradient
main entries, with targeted parameter variants. It was not a full playback of
every chapter or a WebGPU certification. The compatibility copy states specific
limitations without implying every frame is broken. Gradient was separately
revised and checked during the earlier experiments.

Known legacy issues include fixed swap counts with variable input length, Hash
lookup/narration disagreement for some moduli, the omitted Fourier scale factor,
and an unexplained typography tilt readout. None should silently become a
current lesson merely because its source compiles.

## Preservation contract

- Preserve source files and module paths in the repository while tests or imports rely on them. The new public site does not distribute retired demo assets; archive pages link to the published historical source.
- Keep old source files and their existing test semantics separate from new
  curated source. Do not rewrite a fixture solely to match a new poster.
- Do not render the old interactive HTML beneath a new archive heading.
- New internal links use the site's base-aware URL helper. Historical source links pin the published baseline, rather than suggesting an unpushed experimental revision is online.
- New replacement links must resolve in the built site before publication.
- The archive is explicit retirement, not a claim that all nine works were
  individually remade. Current work and learning pages are the public product.

`legacy-works.test.mjs` checks the frozen nine-ID URL contract, immutable historical source links and the
absence of old player embeds. The assembled-site checks must also verify the
generated compatibility pages, rather than accepting copied legacy HTML as a
substitute.
