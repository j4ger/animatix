# Animatix Web — House Style & Portfolio Architecture

The normative design contract for everything under `web/`: the demo scenes
(`*.amx`), the pages that present them, and the machinery that keeps the two in
agreement. It exists because the 2026-11 redesign found the portfolio safe and
monotone: every scene a small diagram floating in a black void, every page the
same amber-on-obsidian card grid, and every work's metadata hand-copied into
four files. This document is the correction.

Scenes are reviewed by eye against the storyboard contact sheet
(`animatix storyboard <file>`), which includes the resting frame. A scene that
violates §1 does not ship, whatever the gates say.

## 1. Scene house style

Eight rules. The first three are mechanical and gated; the rest are reviewed.

1. **Ink at t=0.** The first rendered frame is already a composed stage: the
   backdrop floor is lit and the title block or the subject's silhouette is on
   it. No fade-from-black openings. Entrances animate *within* a lit stage.
   `scripts/site-rest-check.py` gains a first-frame floor alongside the resting
   one, so this is enforced, not requested.
2. **Rest = fullest.** The held frame of an `autoplay loop` cycle is the
   densest composition of the scene. Outros may move parameters but never erase
   the cast. `REST_FLOOR` rises from 0.35 to 0.80 for bordered figures; the
   full-bleed home hero keeps its audited exemption.
3. **No dead holds.** A scene's first reactive motion starts at t=0. An
   `always` block that wakes at t=2.5 leaves 2.5 s of frozen plate (the defect
   the old `matrix/rotation.amx` shipped).
4. **Full-stage composition.** Every scene carries a backdrop layer
   (`stage.StageFloor` plus, where wanted, `amb.Ambience`) so the frame is never
   a void, and the subject occupies ≥60 % of the 1280×720 frame area with
   margins ≤8 % per side. The old hash figure — eight buckets and two chips in
   80 % empty black — is the anti-pattern.
5. **Type floors.** At 1280×720: act titles ≥56 px, captions ≥22 px, data
   readouts ≥20 px, kickers ≥16 px. Nothing renders below 16 px except
   deliberate micro-labels on the backdrop grid (≥12 px, `text.muted`). The old
   plates set node labels at ~9 px, illegible at card size.
6. **Editorial asymmetry.** Title blocks anchor left with a rule
   (`stage.ActHead`); captions sit bottom-left with a mono tag
   (`stage.Caption`). The centered EYEBROW / Title / subtitle stack is retired.
7. **Three acts.** Hook (0–20 %): one bold move that states the question.
   Mechanism (20–75 %): the working diagram, revealed progressively. Payoff
   (75–100 %): the assembled whole plus one surprise — camera push, bloom
   flare, iris wipe — held into the resting frame.
8. **One palette per work.** Scenes draw in their work's colorscheme
   (`ink`/`ember`, `tide`, `moss`, `orchid`, `coral` in `demos/lib/theme.amx`),
   never in universal amber. Glass appears at most once per scene and only over
   something worth frosting.

Shared primitives live in `web/demos/lib/stage.amx` (`StageFloor`, `ActHead`,
`Caption`, `Readout`); atmospheric depth in `web/demos/lib/ambience.amx`
(`Ambience`). A scene imports both rather than re-deriving a backdrop.

## 2. Page visual language

Dark ground, real depth, editorial type — the scenes glow, so the page stays
dark, but the flat obsidian + single amber + uniform card grid goes:

- **Depth, not flatness.** Layered radial washes, a faint grain, and per-work
  accent glows behind full-bleed stages. Cards survive only where they carry a
  live player; prose and figures sit directly on the ground.
- **Editorial type.** A display face for h1/h2 at true display scale
  (clamp up to ~96 px on the home hero), the existing sans for body, and the
  mono for every timecode, file path and data readout. The long-declared
  `--serif` earns its keep as the pull-quote / lede voice.
- **Full-bleed asymmetric stages.** The home hero and each work's deck open on
  an edge-to-edge player with the copy in a side column, not a centered card.
- **Per-work accent.** Each work tints its own section, poster frame and deck
  aura from its palette, so scrolling the catalog reads as nine rooms, not one
  repeated card.

## 3. Interaction contract

- Every embedded player is scrubbable: the deck stepper, the gallery theater
  mode, and a global scrub on the home duality section stay.
- **Parameter sliders.** Flagship scenes expose 1–3 build-time `let` params
  with authored defaults. The player gains `data-vars` (JSON) and
  `setVars(obj)`: a debounced (~150 ms) rebuild through the same `--set`
  mechanism the CLI uses, preserving normalized playhead and autoplay state.
  No new expression semantics — the language's existing vars do the work.
- Hover-to-play posters must show a lit composed frame *before* hover (rule 1
  makes this true by construction); hover starts motion, it does not reveal ink.

## 4. Content architecture

`web/works.json` is the single source for every work's identity: id, title,
kicker, blurb, tags, category, scene count, poster scene, deck href, palette,
figure list, and slider params. Pages carry generated blocks delimited by
`<!-- site-gen:<block>:start|end -->` comments; `scripts/site-gen.py` rewrites
them from the manifest and `--check` verifies they agree, wired into
`scripts/content-sync.sh` as a gate. Hand-editing a generated block is a gate
failure, not a drift nobody notices.

Dev probes (`svg-probe/*`, `multi-probe`, `perf-probe`, `var_probe`) live under
`web/dev/`, out of the showcase tree and out of the nav.

## 5. Gates

- `site-rest-check.py`: a scene carrying `// house-style: <note>` is held to a
  0.80 resting floor **and** an absolute first-frame floor of 5 % whole-frame
  ink (a ratio test would punish dense diagrams for resting on a lit title
  stage); legacy plates keep the 0.35 resting floor until they are rewritten
  and opt in via the marker, so the rollout is staged and every exemption stays
  audited in the scene source. When the last legacy plate is rewritten,
  `REST_FLOOR` itself rises to 0.80 and the marker becomes decorative.
- `content-sync.sh`: + manifest/blocks agreement.
- `web-smoke`, `site-scenes`, `render-smoke`: unchanged, still green.
- Every new or rewritten scene passes the 3-step loop before commit:
  `animatix check`, `animatix review`, `animatix storyboard` + an eyeball of the
  contact sheet (fullest frame, overlaps, resting frame).
