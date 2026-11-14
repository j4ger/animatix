# Content catalog: organize by purpose, publish by target

`web` is a delivery target, not a second category of content. Keep existing
paths and URLs for now; this first pass inventories sources and records decisions
without deleting, relocating, or republishing them.

## Two public doors

| Content | Reader's question | Acceptance bar |
| --- | --- | --- |
| Example | “How do I use this one feature?” | Short, editable, one clear lesson |
| Showcase | “What can I make?” | Complete idea, deliberate pacing, polished composition |

Recipes are a problem-oriented view of examples; templates are starting points.
Gallery, home highlights, and embedded figures are views of the same works, not
independent copies. The showcase metadata remains in `web/works.json`.

Internally, keep **fixtures** (regression cases), **benchmarks** (fixed workloads),
**experiments** (unreviewed exploration), **library modules**, and **brand assets**
separate in purpose. A test fixture need not be beautiful. A pretty screenshot
is not proof that a benchmark may change.

## Inspect the current inventory

From the repository root:

```sh
python3 scripts/content-catalog.py --check
python3 scripts/content-catalog.py --output /tmp/animatix-content.json
```

The report includes every tracked and non-ignored untracked `.amx`, current
content hashes, inferred purpose, import edges, work membership, and literal
full-path references from code, scripts, pages, and docs. No generated inventory
is committed: it would become another stale list. The JSON records the base
revision; hashes describe the actual working tree, including uncommitted edits.

`content/catalog.json` is a small review overlay, not another site manifest.
It gives reviewed candidates a stable ID, canonical source path, proposed
`decision`, rationale, and source relationships. Unregistered sources have a
null ID until curated; do not use their path-derived classifications as durable
public identifiers. Preserve an assigned ID if its file later moves.

The report is intentionally conservative:

- `kind_basis: path-inference` means an initial bucket, **not** curator approval.
- `entry-candidate` does not certify standalone compilation or playback. Imports,
  plugins, fixtures, and cross-scene context still require checks.
- `visual_review: not-reviewed` is the default, even for existing public works.
- Import discovery is textual; unresolved paths may be runtime aliases. Literal
  references miss generated URLs, basename lookups, shell discovery and dynamic
  imports. Absence of a reference is **not permission to delete**.
- `works` is inferred from manifest posters, figures, and deck directories; it
  describes association, not an additional publication list.
- This prototype is not wired into CI or the publisher. `--check` validates the
  review registry, not animation quality or the completeness of dependencies.

## Initial source-level decisions

| Candidate | Proposed action | Evidence / next step |
| --- | --- | --- |
| Gradient walkthrough | Merge maintenance, not scenes | `scene.amx` explicitly hand-copies four acts. Generate the plate from those acts and retain independent embeds. |
| Epicycles walkthrough | Keep | Existing generator and `--check` prevent plate/act drift. Keep the source acts and generated output. |
| Ambience example + web scene | Review | Similar topic, but the example carries a population regression contract. Do not replace it solely on visual similarity. |

These are source reviews, not visual ratings. Do not infer that other existing
scenes should be retired. In particular, nearly identical A/B fixtures often
preserve the single variable a test needs.

## How to promote a candidate

1. Name the intended audience and one teaching goal or story. Assign a stable ID.
2. Identify its entry, imported modules, assets, existing embeds, tests, and any
   generator. Keep the canonical content in one place; generated derivatives
   should have a drift check, as the epicycles plate does.
3. Run `animatix check`, then render a storyboard. Inspect the actual images for
   collisions, clipping, legibility, transitions, and the resting composition.
4. Preview at the intended display size. Record CLI, GUI, and Web separately;
   one successful native render does not prove browser parity. Mark audio
   listening as unverified unless it was actually heard.
5. Record review evidence with the source hash/revision, commands, artifact
   location, and unresolved issues. An optional `visual_review` object on a
   registry entry can hold those facts; never use a bare “passed” as evidence.
6. Only after visual acceptance add or update the existing publication metadata
   and page references. A registered candidate is not automatically published.

Start with a small set: layout/arrays, time/easing, reactive values, plots,
text, paths, composition, and captions. Complement those focused examples with
three complete stories, such as a derivative, interference, and an algorithm.
Rewriting every existing demo at once would hide the quality bar rather than
establish it.

## Before any later move or retirement

Check `scripts/lib-scenes.sh`, `scripts/site-artifact.sh`, native `include_str!`
callers, the Web host's bundled-library aliases, and public URLs. Keep the
single library at `web/demos/lib/` until all consumers are migrated together.
Make a relocation map and preserve published URLs. `retire` in the review overlay
is a proposal, not an executable delete operation.
