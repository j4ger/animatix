# Animatix Implementation History — Index

Archive of completed, resolved, and closed work, split into monthly volumes so the
file stays navigable. The forward-looking backlog lives in [`roadmap.md`](../roadmap.md);
entries here are kept as evidence and should not be scheduled — take them as prior
art when a related item reopens.

Moved out of `roadmap.md` on 2026-09-13 (the roadmap had grown to 800 lines of
mixed history and open work). Split into monthly volumes on 2026-10-29; the old
`Completed Tracks` wrapper was dissolved into the volumes by entry date, and
`history.md` is a thin pointer kept for old links.

## Volumes

| Volume | Period | Contents |
|---|---|---|
| [`2026-08.md`](2026-08.md) | August 2026 | 10 entries |
| [`2026-09.md`](2026-09.md) | September 2026 | 21 entries |
| [`2026-10.md`](2026-10.md) | October 2026 | 31 entries |

## August 2026 — [`2026-08.md`](2026-08.md)

- Architecture Follow-Ups (2026-08-12)
- Presenterm-Inspired Design Tracks (evaluated 2026-08-12)
- Dogfood Follow-Ups (2026-08-13)
- Dogfood Content Backlog
- Closed issue log (2026-08-26)
- Resolved Open Questions (2026-08-26)
- Audit History
- Completed Backlog (2026-08-19)
- Archived Ideas
- Demo Gallery Redesign (2026-08-21 → 2026-08-25)

## September 2026 — [`2026-09.md`](2026-09.md)

- GUI UX Redesign (2026-09-14)
- Performance Evaluation Framework & Backlog
- Audit Fix Pass — Round 2 (2026-09-08)
- Audit Fix Pass (2026-09-07)
- Dogfood-driven fix pass — the plan as executed (2026-09-06)
- Built-in effects, wave 2 (2026-09-14)
- Authored `solo` (2026-09-14)
- Debug scaffolding that shipped as behaviour (found and fixed 2026-09-14)
- Property descriptors and render contexts (2026-09-14)
- Redundant property aliases removed (2026-09-14)
- `Rect` corner radius (2026-09-14)
- Built-in effects, wave 1 (2026-09-13)
- Entrance opacity and declared duration (2026-09-29)
- Web player: transitions, assets, runtime fonts (2026-09-29)
- The wasm-SVG item that never was (2026-09-29)
- Transition frames keep their filter scopes (2026-09-29)
- Build-time warnings for tofu text and dropped properties (2026-09-29)
- Web scenes can import fetched modules; a diagnostic-snippet panic (2026-09-30)
- Per-embed engine profile and build quality (2026-09-30)
- The blit pipeline vs the browser's canvas format (2026-09-30)
- The authored-opacity gate, and the demo lib that ships inside the wasm (2026-09-30)

## October 2026 — [`2026-10.md`](2026-10.md)

- Timeline ECS Refactor: Behavioral Systems, Componentized Caps, and Payload Split (2026-10-28)
- Web Build Hardening: Hermetic wasm-bindgen, Stamped Bundles, Browser Smoke (2026-10-27)
- Follow-Up Audit: Dead Bindings, Silent Drops, Dispatch Cleanup (2026-10-19)
- Primitives → `PrimitivePayload` Migration Follow-Up (2026-10-19)
- Non-Text Scene Evaluation & Viewport Culling Optimization (2026-10-18)
- GPU Pipeline & Compute Pass Modernization (2026-10-18)
- Core Architecture & Evaluation Modernization (2026-10-18)
- Speech/Voiceover Progress Synchronization & First-Class Captions (2026-10-17)
- CI Wall-Clock Runtime Optimization (2026-10-13)
- Animatix Arena (Dogfood 2.0 Migration) (2026-10-13)
- Delightful Animation & Kinetic Design (2026-10-10)
- A failed build replaced the live scene (2026-10-01)
- The instrument redesign: web/ as a living timeline (2026-10-01 → 2026-10-02)
- The Easing Pass — one vocabulary for the site's motion (2026-10-02)
- Phase 2 handoff — closed out (2026-10-03)
- The Motion-Vocabulary Round — Landed Work (2026-10-04 → 10-05)
- External reference points (2026-10-04 study)
- Found while probing: `draw-in` does not draw on non-plot shapes
- Item inventory and feasibility
- Remaining work at the close of the motion-vocabulary round (2026-10-04, superseded)
- Decisions (resolved 2026-10-04, with the choices each implementation made)
- The unified build and release pipeline (2026-10-08)
- The timeline ECS migration closes: performance evidence (2026-10-25)
- The frame path learns to use the ECS: per-entity caches, plan, FxHash (2026-10-25)
- ECS post-migration audit: carry gaps, layout label plumbing, doc drift (2026-10-26)
- ECS audit, second pass: carry completeness and the solo gate (2026-10-26)
- New perf baseline, and an allocation micro-cleanup (2026-10-26)
- Percentage and `fill` child sizing in a Row (2026-10-26)
- `max_item_width`: a layout width cap with an available name (2026-10-26)
- The `Applicable::Everything` audit: the rows were right, the record was missing (2026-10-26)
- Roadmap cleanup (2026-10-30)

## Themes

- **Architecture and storage** — crate split, the timeline ECS migration and its audits
  (October), and the file-size ratchet's history.
- **Rendering and effects** — the post-processing abstraction (August), effect waves 1–2
  (September), the second-input ABI and `Glass` (October).
- **Web player and site** — web-player hardening, transitions/assets/fonts, the instrument
  redesign, and the demo-review batches (September–October).
- **Language and tooling** — audit fix passes, property descriptors/aliases, the
  formatter's known bugs, and the motion-vocabulary round.
- **Performance** — the PF ledger, allocation rounds, the ECS frame path, and the
  measured web/native gap.

## Archived from the roadmap (2026-10-29)

- Frame-time geometry writes disagreed with their declarations (2026-10-07) — fixed
- `PlotCurve`'s `resolution` knob was dead (2026-10-07) — fixed
- Property-name audit — the text wrap width inversion (fixed 2026-10-08)
- Easing pass — what else it resolved (2026-10-02)
- Property value forms — the resolved rows (2026-10-03)
- The preview-lag round — what landed, what the measurements killed (2026-10-06)
- The unified pipeline — the enforced part (2026-10-08)
- The web frame cache was already gated (resolved 2026-10-06)

## Where did an entry go?

Each volume lists its entries above in document order. Search the volumes for a
heading, a commit hash, or a measured number; the split was a pure move (no entry
was rewritten), so old prose is intact.
