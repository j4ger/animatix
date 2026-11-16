# Geometry-count v1

Frozen, generated **performance inputs**, separate from renderer correctness
plates and mutable showcase examples. Generator: `../../geometry_count_workload.rs`.
Contract: `manifest.json`. Change inputs, grid, timeline, API or timed boundary
under a **new workload version**, not silently under these IDs.

| Criterion row | Parameters | Meaning |
| --- | --- | --- |
| `geometry_rect_count_v1/static_repeated_time/{count}` | 16, 64, 256 | Static Rects, repeated `t=0.5` after one warm evaluation. Eligible for whole-frame reuse, but no cache-hit counter is exposed or claimed. |
| `geometry_rect_count_v1/animated_varying_time/{count}` | 16, 64, 256 | All Rects move +8 px horizontally from 0 to 1 second with explicit `[1s, ease: linear]`; cycle 64 binary-exact samples `i/64`. Consecutive millisecond cache keys differ, including wrap. Subtree/geometry and other internal caches are not forcibly disabled. |

Both use the **same public `Timeline::evaluate` API**, default debug options,
1024×1024 viewport, 24×24 opaque red Rects, no stroke/text/assets/effects. A fixed
16-column grid has 56 px pitch and starts at (32,32); lower counts are prefixes.
All actors remain on screen, disjoint even at the end of the animation. Count
is authored Rect count, not pixels, Vello commands or GPU draw calls. Criterion
records the count in its ID and `Throughput::Elements` metadata.

## Timing boundary

Included per iteration: selecting the mode/time, CPU scene evaluation including
its normal internal caches and allocation behavior, and destruction of the
returned Vello Scene. Generation, parsing, timeline build, one warm evaluation,
GPU work, readback and file I/O are excluded. It is not a cold-start benchmark,
a cache-miss guarantee, GUI latency, or the web presentation/recycling API.

Parse/build diagnostics and actor counts are checked before timing. The
`engine_contracts::geometry_count_contract` tests additionally check observed
SceneItem count/visibility, rendered transforms and public affine lookups at
0/250/500/750/984 ms (0.001 px tolerance), motion endpoints, in-viewport
bounds, fixed source-byte snapshots and the sample sequence after millisecond
quantization. They do not test or assume internal cache-hit counts.

## Comparison and provenance

Only compare **the same complete row**, same manifest and generator/runner
fingerprints, matched toolchain/dependencies/features and comparable hardware.
Changing from repeated time to varying time, changing actor count, or replacing
an old `evaluate_*` row is not an optimization result. Retain old baselines;
start a new baseline directory including these new rows. Existing coverage
checks remain strict. All six new rows are `signal_only`; existing budgets,
sentinels and regression gates are unchanged.

`perf-bench.sh save` includes `geometry_count_v1.contract` and SHA256 fingerprints
of the manifest, generator, benchmark runner and `frame` registration in
`META.json`. Existing dependency/configuration fingerprints remain present.
These describe the files at save time, not a hermetic build or automatic
comparability verdict. Dirty engine code, environment flags, concurrent file
changes, compiler and machine state still need review.

```sh
# Contract/compilation checks; no Criterion measurements:
cargo test -p animatix --test engine_contracts geometry_count_contract::
cargo check -p animatix --bench frame
# Optional measurement, only on an idle, explicitly selected host:
cargo bench -p animatix --bench frame -- geometry_rect_count_v1
```

No measured performance improvement accompanies this addition.
