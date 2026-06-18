# Feasibility: Merge Fragment into Typst (every Typst actor highlightable)

## Verdict: Conditional GO

- **GO** on the cheap, valuable part: add whole-actor highlighting to `Typst`
  (Option A below). Additive, low-risk, uses existing infrastructure.
- **NO-GO** on eliminating `Fragment` / folding sub-segment highlighting into
  standalone `Typst`. Either it regresses the flagship use case, or it is a
  cosmetic rename that keeps the special container and delivers no real
  simplification.
- **`$$` shorthand is orthogonal** and can be introduced independently. It does
  not require this merge, and the merge does not enable it.

---

## What the source actually shows

1. **`$$` does not exist.** Only `text_shorthand` exists today
   (`label: "string"` → `Text`; `crates/animatix-syntax/src/parser/stmt.rs:260`,
   `docs/spec.md:477`). `$$` is a *new* feature to design, not an existing one
   to repurpose. The proposal conflates "introduce `$$`" with "merge Fragment
   into Typst"; they are independent.

2. **Highlight fields are already generic on `Track`**
   (`crates/animatix/src/timeline/track.rs:670-678`):
   `highlight_color`, `highlight_opacity`, `highlight_padding`,
   `highlight_radius`, `highlight_blend`. They are NOT Fragment-specific.

3. **The `highlight`/`unhighlight` actions already target ANY track** with no
   `ActorKindId` check (`crates/animatix/src/timeline/actions/highlight.rs:144-179`).
   `highlight someTypst` already sets the fields today — they just never render,
   because `Typst::evaluate()` never emits a `HighlightLayer`.

   ⇒ "Make Typst highlightable" is almost entirely a *rendering* gap, not a
   data-model change.

4. **The flagship use case is the blocker.** `examples/fft_explain.amx`
   compiles `x(t) = sin(2π·2t) + 0.55·sin(2π·5t) + 0.3·sin(2π·9t)` as ONE Typst
   document via `#box()` wrapping + `extract_glyphs_grouped()`
   (`crates/animatix/src/timeline/scene_eval.rs:699-816`,
    `crates/animatix/src/renderer/text.rs:379`), yielding per-fragment bboxes.
   This is "one equation, multiple highlightable parts." Independent Typst
   actors cannot reproduce it: each piece is a separate Typst document, so math
   layout (baselines, operator spacing, subscripts, fractions) does not compose
   across documents.

5. **The roadmap treats Equation+Fragment as intentional**
   (`docs/roadmap.md`): "Equation: bare string syntax sugar for anonymous
   Fragments" is listed as planned work — not accidental complexity to remove.

---

## The two capabilities the proposal conflates

- **A. Whole-actor highlight** — highlight the entire rendered text bbox.
  Naturally a property of any text actor. Cheap, additive, uses existing
  `Track` fields + existing action. **Does not require eliminating Fragment.**

- **B. Sub-segment highlight within a jointly-compiled equation** — highlight
  one part of a single rendered equation. *Structurally requires* a container
  that compiles children together and does grouped extraction. This is exactly
  what `Equation` + `Fragment` is.

The merge tries to fold B into A, but B is architecturally distinct.

---

## Options assessed (proposal question #2)

- **2a — separate Typst actors in a layout container.** REJECT. Loses coherent
  equation layout; pieces won't share baseline/spacing.
- **2b — Equation concatenates Typst children, keeps grouped extraction.**
  REJECT as a "simplification." This is just *renaming Fragment→Typst* while
  keeping the special container and the non-independent child render. It
  delivers none of the promised unification (the special `scene_eval.rs` branch
  stays; children still don't render standalone) but costs full migration
  churn. Cosmetically unified, architecturally identical.
- **2c — accept the loss.** REJECT. Regresses the flagship example and the only
  genuinely valuable sub-segment behavior.

---

## Preferred design (do A; keep B)

1. **Add whole-actor highlighting to `Typst`** (optionally `Text`/`Code`).
   In `crates/animatix/src/primitives/typst.rs::evaluate()`, when
   `track.highlight_opacity.get(time_ms, 0.0) > 0`, compute the bbox over all
   glyph paths (same loop already in the Equation branch,
   `scene_eval.rs:743-761`) and emit a `RenderCommand::HighlightLayer` before
   the `Text` command. The `highlight`/`unhighlight` actions already work on
   Typst tracks. Surface the 5 props in `default_props` + docs.
   Net: ~1 file of real logic.

2. **Keep `Equation` + `Fragment`** for sub-segment highlighting. It is the
   only model that preserves coherent multi-part equation highlighting.

3. **Treat `$$` as a separate, optional proposal.** It can target `Typst` at
   top level regardless of this merge. It does NOT simplify the Equation case:
   `highlight eq.f1` still requires named, addressable children inside the
   container — which is what `Fragment` provides.

---

## Answers to the specific questions

- **#1 (standalone highlight sufficient?)** Yes for whole-actor; NO for
  sub-expression. You lose sub-expression highlighting *unless* Equation stays
  special. "Ignore the feature if unneeded" is false framing — the feature
  you'd lose is the one users actually want.

- **#3 (what happens to Equation?)** Under 2b it stays special (just with Typst
  children). Under 2a/2c it's gone and the capability is gone with it. Neither
  is a win. KEEP IT.

- **#4 (property surface)** 5 extra props on Typst is mild and defensible *for
  whole-actor highlight*. But you'd then have the same props meaning two
  different things (whole-actor bbox vs. per-segment bbox) depending on whether
  the Typst is standalone or inside Equation — a subtle inconsistency. Keeping
  Fragment avoids this.

- **#5 (backward compat)** Fragment is referenced across:
  `primitives/fragment.rs`, `primitives/equation.rs`,
  `timeline/scene_eval.rs` (Equation branch), `timeline/actions/highlight.rs`,
  `timeline/tests.rs`, `primitives/mod.rs` (registry / `ActorKindId` /
  capabilities), `docs/primitives.md`, `docs/spec.md`, `docs/roadmap.md`,
  `examples/fft_explain.amx`. A deprecated alias is feasible but pointless if
  Equation's special branch must remain anyway.

- **#6 (`$$` DX)** Cleaner DX is real but ORTHOGONAL — achievable without
  touching Fragment.

- **#7 (scope to do A only)** `primitives/typst.rs` (evaluate emits
  `HighlightLayer`; `default_props`), `docs/primitives.md` (Typst highlight
  props + actions), `docs/spec.md`. ~3 files, low risk.

- **#8 (risks of full elimination)** Regression of multi-segment equation
  highlighting; dual semantics for highlight props; migration churn with no
  architectural payoff; loss of named-addressable children required by
  `highlight eq.f1`.

---

## Risks / blockers for the full merge

- **Hard blocker:** joint Typst compilation is required for coherent multi-part
  equations → Fragment's role cannot be replicated by independent Typst actors.
- The "uniform `$$`" promise breaks at exactly the interesting case: inside
  Equation, children must be jointly compiled → not standalone → not uniform.

## Next steps if moving forward (Option A only)

1. Spike: in `typst.rs::evaluate`, emit a `HighlightLayer` over the full glyph
   bbox when `highlight_opacity > 0`. Verify with a small `.amx` using
   `highlight someTypst [800ms]`.
2. Add the 5 props to `Typst::default_props` (default highlight off) and
   document in `docs/primitives.md`.
3. Keep `Fragment`/`Equation` untouched. No migration.
