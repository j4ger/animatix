# Renderer correctness plates

`geometry_plate.amx` is an initial **correctness** specimen, not a showcase or
performance baseline. Run its real public pipeline with:

```sh
nix develop --command env ANIMATIX_REQUIRE_GPU=1 cargo test -p animatix-render --test geometry_plate -- --test-threads=1
```

The existing workspace test gate automatically discovers this integration test
and requires a working GPU backend. Outside that gate, the existing renderer
policy allows a missing GPU to skip; use `ANIMATIX_REQUIRE_GPU=1` for evidence.
A CPU-only oracle check is also available with no crate dependencies:

```sh
rustc --edition=2024 --test crates/animatix-render/tests/support/plate_oracle.rs -o /tmp/plate-oracle
/tmp/plate-oracle
```

## Plate contract

384 × 192, eight 96 × 96 cells, one render and readback at time zero:

| Row | Left to right |
| --- | --- |
| Top | solid rect; rounded rect; non-circular ellipse; diamond polygon |
| Bottom | closed triangular path; thick line; arrow with head; hollow stroked rect |

Each cell has its own named interior/color/opaque-alpha probes, exterior probes,
foreground bounding box and coverage interval. An eight-pixel black gutter
protects independence. Coordinates and shape dimensions deliberately differ so
an ellipse replaced with a circle or a hollow rect accidentally filled cannot
pass. Bounds tolerate 2px, channels 8/255, and coverage intervals allow boundary
antialiasing variation. These are explicit geometric expectations, not generated
from the production renderer and not a whole-image similarity score.

Every cell must pass. Failure messages name the cell and violated measurement.
After a successful real render the test injects three faults into **each** cell:
erasure, replacement with gray, and an eight-pixel horizontal translation. Each
must fail its own cell and leave all seven other cells passing. This proves local
sensitivity to those fault classes, not completeness against arbitrary defects.
Two additional faults exchange the diamond and triangle silhouettes, recoloring
into the destination's expected palette. They share nominal bounds and area but
must fail shape-sensitive reciprocal interior/exterior probes at (26,48) and
(28,64). These probes are away from antialiased boundaries.
The CPU synthetic rectangle test separately checks the oracle and mutation
isolation; it cannot stand in for the real GPU test.

## Boundaries

- Initial static fill/stroke geometry only. No opacity compositing, curves,
  animation/seek, camera, text or effect correctness claim is made.
- No shared average, golden update command, generated validator framework or
  new dependency. The fixture stays beside its owning renderer test.
- Do not delete existing isolated renderer tests: atlas residency, lifecycle,
  cross-batch/target isolation, filters, backdrop dependencies, transitions and
  text have contracts that packing into a single frame cannot preserve.
- Effects need separate plates with explicit support bounds and backgrounds;
  global camera/transition and cold/warm lifecycle remain isolated cases.
- Benchmark workloads remain separate. Fewer readbacks are a design objective,
  not a measured performance improvement until equivalent runs are timed.
- Do not weaken expectations simply to match a new output. Investigate a failed
  cell, then review an intended contract change explicitly.
