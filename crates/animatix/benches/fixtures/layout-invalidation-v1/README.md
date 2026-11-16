# Layout invalidation v1

Six independent signal-only rows under `layout_invalidation_v1`: **static**,
**color**, and **size**, each with **16 or 64 Rect children plus one Row**.
No old benchmark is removed and no CI performance threshold is introduced.
These are CPU scene-evaluation workloads designed to exercise layout validity,
not timers around the layout solver alone.

## Fixed inputs

`manifest.json` describes `../../layout_invalidation_workload.rs`. The viewport
is 2048×256; the centered Row is 1792×128 with gap 4 and `dynamic_layout: true`.
Rect children begin at 12×24, opaque red, with no stroke, text, media or effects.
Static inputs do not animate. Color changes every child's fill to blue over
one second. Size changes every child's width to 20, leaving height 24 and red
fill unchanged. Both animations explicitly use `[1s, ease: linear]` at `#0s`.
Even the largest endpoint fits inside the viewport without overlapping siblings.

All modes use the same repeating 64 binary-exact times `(i % 64) / 64` seconds.
The clock persists across Criterion batches. Adjacent values differ after the
engine's millisecond quantization, including wrap. This does **not** disable or
prove hits/misses in layout, subtree, geometry or frame caches. Do not label the
static mode a cache hit or subtract its timing to claim pure layout cost.

## Measurement boundary

Before timing: deterministic source generation, parse, build, diagnostics/count
checks and one warm evaluation at 0.5 seconds. During timing: clock advancement,
`Timeline::evaluate`, and destruction of the returned Vello Scene. The public API
includes normal layout/property evaluation, traversal, encoding, allocations and
internal cache behavior. No structured item collection, GPU work, readback, file
I/O, authoring rebuild or web presentation/recycling API is measured. Criterion
Elements is the authored Rect count, excluding the extra Row, not draw calls.

Only compare the same complete row with matched input fingerprints, dependencies,
features, hardware and conditions. Different modes/counts are different workloads,
not before/after improvements. Establish reviewed baselines before changing any
policy; existing limits and strict coverage behavior remain untouched.

## Output contracts and provenance

The registered `engine_contracts::layout_invalidation_contract` tests read
geometry and fill from **the same evaluated SceneProgram's draw commands and
transforms**, not from separately sampled source properties. At intermediate
0.25/0.5/0.75/0.984375 seconds and the endpoint they verify exact child count,
opaque fill, rendered widths/heights, four-pixel sibling gaps, in-viewport bounds,
color progression, unchanged geometry in color mode and unchanged red fill in
size mode. Width expectations use the engine's quantized time and 0.01 px
tolerance for the independently sampled Rect path at every checkpoint. The
fractional 984 ms size sample allows 1.01 px only for gap/center spacing from
Taffy slot rounding; unchanged geometry, reset, height, and all integer-width
checkpoints retain 0.01 px. This allowance follows the pinned Taffy 0.10.1
[default rounding configuration and `layout()` API](https://github.com/DioxusLabs/taffy/blob/v0.10.1/src/tree/taffy_tree.rs),
which the engine uses without disabling rounding. It is a source-based bound,
not a claim that this fixture's real output has already been measured; the
Cargo contract test must still be executed before acceptance. Color expectations are monotonic/discrete changes rather than assuming
a particular interpolation color space. Backward seek to zero must restore every
bound and fill, and another forward seek must animate again. Fixed source-byte
snapshots guard accidental v1 fixture drift; they are not security hashes.

`perf-bench.sh save` records `layout_invalidation_v1.contract` and SHA256 of the
manifest, generator, runner and frame registration in `META.json`, alongside the
existing dependency/configuration provenance. These are save-time identities,
not a hermetic build or automatic comparability verdict. Change the workload
version when changing input or timed semantics, and retain historical baselines.

```sh
# Contract/compile checks, not benchmark measurements:
cargo test -p animatix --test engine_contracts layout_invalidation_contract::
cargo check -p animatix --bench frame
# Optional measurement on an explicitly selected, idle host:
cargo bench -p animatix --bench frame -- layout_invalidation_v1
```

No timings or performance improvement claims accompany this addition.
