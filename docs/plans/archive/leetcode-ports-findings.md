# Findings: Porting LeetCodeAnimation topics to .amx — language & implementation issues

## Files created (regression cases)
- `examples/leetcode_sort_colors.amx` (LeetCode 75, Dutch flag)
- `examples/leetcode_reverse_linked_list.amx` (LeetCode 206)
- `examples/leetcode_climbing_stairs.amx` (LeetCode 70, DP)

All three pass `check` with no diagnostics. Render probes via `animatix image --time T`.

## Confirmed implementation BUGS (high severity)

### B1. For-loop actor generation is broken — actors never instantiated
`for x, i in list { name[i]: Type, ... }` parses (AST shows `array_index: Some(Ident("i"))`)
but the declaration pass NEVER expands `name[i]` → `name__N` into real actors.

Evidence:
- `lint examples/28_generation_reactive.amx` → 18× `warning: undefined-label: Undefined label: dots__0 … dots__8` (a SHIPPED example).
- Runtime: ex28 dot-region ink is 732px at both t=0 and t=2 (unchanged) even though `always` sets `dots__N.opacity = clamp(...)` → 1.0 by t=2. The `always` assignments silently no-op because the actors don't exist.
- Minimal repro: `for c, k in {red} { a[k]: Rect, ..., opacity: 1 }` renders 0 pixels; `lint` says "Unused actor: 'a'" (the loop body is seen as one unused actor `a`, not expanded).
- `fade-in a[0]` on a for-loop actor → runtime ERROR `build:unsupported-action-target: Action 'fade-in' does not support target 'a__0': the target is not declared yet.`

Impact: any animation using array-indexed actors (the documented pattern for arrays/grids/dots) is silently broken. This is the single most impactful bug found.

### B2. `check` and `lint` are inconsistent
`animatix check` reports "OK (no diagnostics)" for files that `animatix lint` flags with
24 warnings (undefined labels from B1). Users who rely on `check` (the documented
build-diagnostics command) miss entirely broken for-loop actors. `check` does not run
the undefined-label / unused-actor analyses that `lint` does.

## Confirmed linter FALSE POSITIVES (medium severity)

### L1. `swap` flagged "Unknown action" but works at runtime
`lint` → `warning: unknown-action: Unknown action: swap`. But `animatix image` proves
`swap b0, b5` on Row children genuinely exchanges the bars (b0 blue→red, b5 red→blue by
t=5.8). The linter's action allowlist is stale (missing `swap`, likely `reorder` too).

### L2. Container-child labels unresolved
`b3`, `b4` declared inside `row: Row { b3: Rect, ...; b4: Rect, ... }` are flagged
`undefined-label` even though they are valid container children. The linter does not
register actors declared inside a container body block in its symbol table.

### L3. `at` flagged "not commonly used on Rect/Text"
`at` is THE documented positioning property for all actors, yet the linter emits
`info: unknown-property: Property 'at' not commonly used on Rect`. The linter's
per-type property allowlist is incomplete/noisy.

## Language-design gaps confirmed by authoring

### G3 (biggest). No runtime-mutable state, no branching timeline
Sort-Colors and Reverse-LL both required hand-unrolling the entire algorithm into
explicit per-step keyframe blocks. The data-dependent control flow (`if nums[i]==0
then swap-left elif ==2 swap-right`) cannot be expressed — `always` is stateless,
`let` doesn't persist across frames, keyframes are declarative. You animate a
*recording* of the algorithm, never *the algorithm*. This is the foremost language gap.

### G5. No follow/attach primitive; resolved child positions not readable
Every pointer label (`zero_p`, `i_p`, `two_p`, `pre_p`, `cur_p`, `next_p`) needed
manually-computed `.at` keyframes. No way for a label/arrow to track a moving actor.
(`Callout.target` is the lone exception.)

### G6. Arrow endpoints are free Vec2, not actor references
Reverse-LL required hand-computing each link's `from`/`to` coordinates; arrows don't
track nodes by reference. Relinking = manual coordinate math.

### G1/G4. No list-of-actor-refs, no runtime-indexed targeting
The scan "cursor" had to be a separate overlay Rect because you cannot write
"highlight bar `i`" with a runtime `i`. For-loops give `name[i]` labels but (even
setting B1 aside) no runtime set to iterate.

## Hypotheses DISPROVED (things that work)
- **Arrow `from`/`to` timed interpolation WORKS** (H2.1 disproved): in Reverse-LL the
  links smoothly reverse direction; gaps open between nodes as links detach and
  reattach on the other side.
- **`swap` on Row children WORKS at runtime** (H1.1 confirmed): bars exchange layout
  slots and colors move with them.
- **Text assignment `dp.text = "3"` re-renders** (H3.2 disproved): Climbing-Stairs
  counters update (dp2 ink 0→0.135 after assignment; dp5 "13" appears at t=7).
- **`fade-in`, `draw-in`, `pulse`, `stagger`, `sequence` all work.**

## Note on the example files
`leetcode_sort_colors.amx`'s color legend (a for-loop) does not render due to B1.
This is intentional as a regression case. The bars/pointers/swap animation itself works.
