# Handoff — the silent-drop round (WP0–WP8)

> **STATUS: IN PROGRESS (2026-10-03).** WP0–WP5 are complete and committed. WP6
> has a decided design and no implementation. WP7 is half done: the plugin ABI
> easing table (B3) landed, the real `MorphStrategy::Fade` cross-fade (B2) has
> not been scoped or written. WP8 is untouched.
> **15 commits are local and unpushed on `main`, at the owner's explicit
> instruction ("hold the push") — pushing triggers the GitHub Pages deploy.
> Count them with `git rev-list --count origin/main..HEAD`; that number drifts
> as the remaining packages land, the instruction does not.**

This round had three sources that turned out to be one problem: values that are
parsed, authored and accepted, then discarded somewhere between the AST and the
GPU (`docs/roadmap.md` items 1/5/6/7/10), a roadmap of ~30 behaviour gaps whose
root causes were mostly unknown symptoms, and — the load-bearing one — **no way
to verify any of it**. Until WP0 the wasm build could not be produced in this
dev shell, `debug_readback` poisoned the whole browser tab, and headless
screenshots could not see a WebGPU canvas. Every "acceptance" of the site in the
two previous rounds was therefore a DOM probe plus the native renderer, never a
picture from a browser.

## State of play

| WP | What it was | Landed as | State |
|---|---|---|---|
| WP0 | Make wasm buildable locally | `eec10d5e` — `devShells.web-build` in `flake.nix`, `scripts/build-web.sh` appends instead of overwriting `RUSTFLAGS`, `web/README.md` says which shell | done |
| WP1 | Silent-drop cluster: `play` transition ease (A1), `Graph.map()` half-scale (A5), `Arrow` `color:` (A2), plot-family `opacity:` (A3) | `884feda2`, `1a88bf0d` | done |
| WP2 | Replace the tautological guardrails with a behavioural one | `6cb58e17` — `ast_format_coverage.rs` (934 lines) replaces `variant_coverage_guardrails`; repo-wide serializer-stability test added | done, **but** comment fidelity (B1) was re-scoped — see "Refuted premises" |
| WP3 | One false positive killed, one new lint born | `458412bb` (unreferenced *actor* labels demoted to hint, hints folded in the CLI report), `2dd9f689` (`inapplicable-property` at build time) | done |
| WP4 | Evaluation-path bugs: import-frozen clock (A10), text overprint (A8), step-vs-ramp | `dd029d2c`, `6d63ff93`, `384735b3` | done; A9's blamed mechanism **did not reproduce** |
| WP5 | Web runtime: readback poisoning (C1), frame cache locked out (C2) | `f6c7c518`, `8d1f9b3b` | done |
| WP6 | `text_align` anchor semantics | — | **decided, not implemented** |
| WP7 | B3 plugin ABI easing codes; B2 real fade cross-fade | `58b602b1` (B3) | B3 done, **B2 untouched** |
| WP8 | A11 pulse intensity, A12 native font bundling, empty `Remaining:` table, docs closeout | — | untouched |

## Verification: what this round proved about its own methods

Read this before trusting any pixel comparison, including ones from earlier rounds.

**The renderer is not byte-reproducible across processes, and the noise is
larger than the changes we were measuring.** Discovered while re-checking the
step-semantics claim in `384735b3`, whose commit message asserted "304 frames,
288 byte-identical, the other 16 at PSNR ≥107 dB". That comparison had been run
between two paths of the *same* build state (`cmp` proved the trees identical),
so the number described nothing.

The corrected measurement, two binaries differing **only** in the step fence:

- Positive control (proves the change is live): `~/probe_step.amx` at t=1.0 —
  fenced build 2725 lit px (still opaque), unfenced 2125 (already half-ramped).
- Corpus A/B: 149 `.amx` under `examples/`+`web/`+`dogfood/` × t∈{1.0, 4.0} =
  **298 frames: 280 byte-identical, 18 differing by 1–44 raw RGBA bytes** of
  3 686 400.
- Same-binary control: three renders of **one** binary of
  `dogfood/probes/008-render-correctness/text.amx` @ 1.0 differ from each other
  by 20, 36 and 46 bytes — *more* than its distance to the other build (12, 24).
  PNG file sizes drift too. On `web/demos/transformer/scenes/pipeline.amx @ 4`
  the two builds were byte-identical while two runs of one build were 8 bytes
  apart.

Conclusion, recorded in `docs/roadmap.md` ("Pixel A/B has no noise floor yet"):
**the step fence has no measurable visual effect**, and a corpus pixel diff
cannot resolve any change below roughly "several tens of bytes on text scenes".
Text/glyph scenes show the drift and `web/demos/matrix/scene.amx` does not, so
the suspect is glyph rasterization or shaping order — unpinned so far.

Two rules follow, and both were used successfully this round:

1. **Every guard needs a negative control.** Reintroduce the bug, watch the test
   fail. This is how `format_action`'s data-destroying output (`move b to (300,
   200)` → unparseable), the `Applicable` table's two wrong rows, and the ABI
   lie were all caught before shipping — and how one early "guard" was found to
   have no teeth at all (`first_quoted` silently rejected underscored names, so
   the source-audit test audited nothing).
2. **Prefer a track-level or compile-error assertion over a pixel one.** The ABI
   guard now fails as `E0004: non-exhaustive patterns` if a new `Easing` variant
   appears without a code, which is stronger than any test.

Reusable scratch (untracked, outside the repo): `~/amx-measure.mjs` (corpus A/B,
writes PNGs), `~/amx-compare.mjs` (pixel-level compare + PSNR of already-rendered
frames), `~/amx-noise.mjs` (same-binary control), `~/probe_step.amx`,
`~/amx-measure-result.json`. Run with `node <file>`; they assume the two
binaries at `/tmp/binHEAD` and `/tmp/binPRE`, which no longer exist — rebuild
with `cargo build --release -p animatix-cli` and copy the binary aside.

## Remaining work, in the order that makes sense

### WP6 — `text_align`: decided, then build it

Design agreed with the owner 2026-10-03: **split the two meanings into two
properties.** `text_anchor: "left"|"center"|"right"` places the whole text block
relative to `at:`; `text_align` keeps its current meaning (how lines sit inside
a wrapped box). **Both default to what the engine actually does today** — block
centred on the anchor — so nothing moves until an author asks.

Why it is not a renderer tweak, and what the failed attempt measured (all in
`docs/roadmap.md`, rows "Single-line `text_align`…" and "`text_align` cannot be
fixed without deciding what it means"):

- `property_registry.rs` seeds `text_align` with `"left"` while `compile_text_*`
  centred the measured ink regardless of value, so the default has always been
  lying and the three spellings were indistinguishable.
- Making alignment real reaches ordinary centred text everywhere (~two thirds of
  the corpus moved, `examples/basics/00_hello.amx` at PSNR 40 — visible, not
  noise). That number was not noise-controlled either; re-derive it if it
  matters.
- `max_width == 0.0` (the engine default) routes to `compile_text_fast`, which
  never receives `text_align` at all.

Mechanical requirements, easy to get wrong: append `text_anchor` to
`animatix-core::property::PROPERTY_DESCRIPTORS` — **append, never reorder**
(`property_id_order_is_pinned`) — then follow both derivations
(`schema::raw_property_types()` index-aligned, `property_registry::BINDINGS`) or
the build fails. When it lands, delete the site's hand-computed `at:` x for
right-aligned label columns; that workaround is the thing this is for.

### WP7/B2 — `MorphStrategy::Fade` real cross-fade

Untouched, and **not yet scoped** — the exploration I started was declined when
the goal was paused, so nobody has mapped the redeclaration path this round. The
plan's own escape hatch stands: this is the one item where "warn that fade does
not cross-fade state shapes" is an acceptable outcome if the measured cost of
building the outgoing state per frame is not. It doubles shape compilation
inside morph windows, so it **must** go through `scripts/perf-bench.sh compare`
(the script now hard-fails if `python3` is missing instead of silently
reporting green — see `6d63ff93`). Start by reading `timeline/build/dispatch.rs`
(`render_type_name`, `shape_type_switches`), the `Fade` branch in the morph
code, and the text cross-fade in `crates/animatix/src/primitives/mod.rs`, which
already solves "draw two things at complementary alpha".

### WP8 — two semantics decisions and the docs closeout

- **A11 `pulse intensity` is additive** (`1.04` → 2.04×). Choose between
  percentage semantics and a lint for `intensity > 1`; the plan recommends the
  lint because it changes no existing render.
- **A12** whether native `FontContext::new()` should bundle-register like wasm's
  does. The four fast-path tests that quietly depended on system fonts were made
  hermetic on 2026-10-02; the engine question is still open.
- **C5**: `docs/roadmap.md`'s GUI-UX section has a `Remaining:` table with a
  header and no rows. Delete it.
- Then `docs/spec.md` (transition ease now really reaches the compositor;
  `inapplicable-property`; the scene-config position rule), `docs/history.md`,
  and prune `docs/roadmap.md` to remaining work only.

Also still open in the roadmap and found *by* this round's lint: percentage and
`fill` collapse in a `Row` (`timeline/layout.rs:946` hardcodes
`parent_content_size = [0.0, 0.0]`), layout has no `max_width` and the name is
taken by text wrap, `Applicable::Everything` rows hide exactly the drop class
`inapplicable-property` exists to catch, plot axes render pure white under
`dynamic_layout`, and `animatix-text`'s `fast_path_caches_results` flakes under
parallel test runners (process-global cache; green with `--test-threads=1`).

## Refuted premises — do not re-attempt these as specified

- **A9 "shared `#t` redeclaration samples its own target"** — probed on plain
  shape actors in four orderings and it does not reproduce; the pre-frame keeps
  the old shape and colour. Pinned by
  `a_redeclaration_starts_from_what_it_replaces_not_from_its_target`. What is
  left is specific to *component* actors and still needs a minimal repro.
- **`text_align` as a renderer fix** — see WP6.
- **B1 comment fidelity via the `attach_trailing_comments` trick** — cannot
  work: statements carry no byte spans (only `Property::value_span` does). Two
  real options are written up as roadmap item 2; neither is small, and (b)
  (spans on statements) also improves diagnostic locations for free.
- **Adding `animatix fmt --check` to CI** — still blocked; `web/` and `examples/`
  are not clean.
- **A perf claim for C2** — deliberately not made. The browser A/B at a pinned
  0.5 render scale, 300 frames after 60 warmup, was p50 45.2 ms before / 49.4 ms
  after: inconclusive in both directions, and single-run noise on that surface is
  larger than the effect. See `8d1f9b3b`.

## Environment traps that cost time this round

- **Nix dev shells prepend `PATH`**, so a nested `nix develop` keeps the *outer*
  toolchain — that is how the wasm check failed with "can't find crate for
  `core`". Use `nix develop --ignore-environment .#web-build`, and note that it
  also clears `HOME`, which then hides `wasm-bindgen`; pass `HOME=/home/j4ger`.
- `bash -lc` loses the dev-shell PATH (`rustc: command not found`) — use
  `bash -c` inside `nix develop --command`.
- **Piping a build through `head`/`tail` masks its exit code.** One "green"
  report was a stale Oct-1 bundle behind a failed build. Redirect to a file and
  echo `$?` per step.
- Files written under `/tmp` by a Bash tool call did not reliably survive to the
  next one this session; scratch that must persist belongs in `$HOME`.
- Node: `.mjs` needs `import`, not `require` (top-level await + `require` is an
  error), and the corpus driver is at `~/amx-measure.mjs`.
- `cargo` scoped to one crate can pass outside the shell while `--workspace`
  fails inside ALSA/FFmpeg — always run workspace gates in `nix develop`.
- Only **stable rustfmt** exists here; CI's fmt job uses nightly with
  `imports_granularity`/`group_imports`, which stable silently ignores. None of
  this round's commits has been through CI, because none is pushed.

## Gates to run before finishing anything

```bash
nix develop                                   # ALSA + FFmpeg + cog
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --no-fail-fast --workspace -- --test-threads=1
# the native-plugin host is a non-default feature; animatix's own lib tests
# only compile extension_native_plugin.rs (and its guards) when it is on:
cargo test -p animatix --features plugin-loading --lib -- --test-threads=1
cargo build --target wasm32-unknown-unknown -p animatix-web   # in .#web-build
```
Commit with `cog commit <type> "<summary>" [scope]` **inside** the shell. Scopes
in `cog.toml`: `animatix, gui, analyzer, lsp, syntax, parser, renderer, ci,
docs`. Then stop — do not push without the owner's go-ahead.

## The one line

Silent drops are now guarded at the three places they were found (parser→AST
printing, declaration→build consumption, plugin ABI), the site can finally be
verified in a real browser, and the round learned that its pixel method has a
noise floor it had been assuming away. What is left is one decided property
(WP6), one expensive morph change with permission to be downgraded (WP7/B2), and
the documentation closeout (WP8).
