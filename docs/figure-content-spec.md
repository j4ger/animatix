# Figure Content Spec — the per-template character budget

The figure kit (`examples/lib/figure.amx`, bundled into the web runtime and
mirrored to `web/demos/lib/figure.amx`) gives an explanatory figure its layout
baseline: the FIG header block, left-pinned section bands, and small info
cards. Layout is only half of the baseline; the other half is **how much text
fits**. These caps are measured at 1280×720 against the kit's fixed type
sizes — a template whose text overflows is not a template, it is a guess.

Rule of thumb: treat every cap like a title in prose — if the line wraps or
`animatix audit` reports `text.overflow`, the fix is to cut words, never to
shrink the font or nudge `text_max_width` by hand. The hand-tuned
text_max_width values in the old scenes are exactly the drift this kit
exists to retire.

## FigHead — header block

| Field   | Size        | Cap                                   |
|---------|-------------|---------------------------------------|
| kicker  | 18 px caps  | ≤ 52 chars, one line, UPPERCASE       |
| title   | 56 px bold  | ≤ 30 chars, one line                  |

- `title_size` may rise (58–64) for a short title, never drop below 56
  (web/DESIGN.md §1.5 floor).
- Kicker format: `FIG NN · SECTION NAME` — the figure number is the page's
  anchor, not decoration.
- The title states the question or the claim ("Text is not the input",
  "Many heads, one output"), never the mechanism's implementation ("Multi-head
  parallel subspace projection pipeline").

## Band — labelled content row

| Field        | Size            | Cap                                   |
|--------------|-----------------|---------------------------------------|
| label        | 16 px caps      | ≤ 56 chars, one line, UPPERCASE       |
| slot content | author's choice | row width ≤ `width` (default 1088)    |

- Label format: `SECTION NAME · qualifier` ("VOCABULARY IDS · 50,257-TOKEN
  CODEBOOK"). The qualifier carries the number a reader would otherwise hunt
  for in the diagram.
- Label and content share one left edge — that alignment is the template's
  job, so never place a hand-anchored label above a Band.
- Default slot children keep their own sizes; chips should use ≥16 px text
  (house floor) and a shared `width`/`height` so the row rhythm stays even.

## InfoCard — annotated data chip

| Field | Size                    | Cap                                  |
|-------|-------------------------|--------------------------------------|
| title | 20 px semibold, accent  | ≤ 16 chars at `width: 200`           |
| sub   | 16 px muted             | ≤ 24 chars at `width: 200`           |

- Scale `width` with the text, not the font: inner padding is derived
  (`width − 36`), so widening the card widens the text box. Re-derive the caps
  as `width − 36` px worth of characters when you widen.
- One card, one fact. A card that needs two sentences wants to be prose in the
  article, not pixels on the figure.

## Scene-level budget (whole figure)

- One FigHead, ≤ 2 Bands, ≤ 2 InfoCards per 1280×720 figure. More than that
  and the reader is reading, not watching — move it to the article text.
- Captions (`stage.Caption`): ≤ 55 chars, one line, one idea per act; a scene
  speaks in ≤ 3 captions total (hook / mechanism / payoff).
- In-canvas text never duplicates the article paragraph next to the figure:
  the figure labels its parts, the article argues.
- Nothing below 16 px except deliberate grid micro-labels (≥ 12 px,
  `text.muted`) — the kit's sizes already comply.

## Gates

`animatix check` + `animatix audit` on any scene using the kit must report no
`text.overflow`; `scripts/check_examples.sh` covers the kit's own example
(`examples/figures/figure_kit.amx`), which is the executable documentation of
every cap above.
