# Offscreen blur area v1

Manual completed-frame experiment, not a CI speed gate. Fixed 512×512 output,
centered opaque white squares of 64, 128 and 256 logical pixels, black backdrop,
plain or a single Blur radius 6. These are **authored geometry sizes**, not measured
GPU allocation sizes or dispatch areas: padding, scratch bucketing and the output
copy can make actual backend costs differ. Q=1: each begin is immediately waited
before the next begin. No overlap, GPU timestamp query, VRAM metric or speedup claim.

Run from the repository's Nix development shell:

```sh
python3 crates/animatix-render/tools/offscreen-blur-area.py
# Software results are separately labelled; never compare with hardware:
python3 crates/animatix-render/tools/offscreen-blur-area.py --allow-software
```

The wrapper builds the release example with `--locked`, discovers the executable
from Cargo JSON, and creates a unique result directory. Driver files use
`create_new`; prior results are not overwritten. The outer process-group timeout
bounds a stuck GPU wait/map (180 seconds by default; build has a separate 1800
second bound). Failure leaves diagnostics and `status=failed`, not an accepted
performance sample. Native Linux runner only; no browser or wasm timings.

## Timing boundary

Each sample has three integer nanosecond values:

- `host_evaluate_encode_submit_ns`: timeline evaluation, filter work orchestration,
  encoding and submission, including enqueueing the final texture readback copy.
  Driver/backend calls may internally wait; this is not guaranteed CPU-only time.
- `completion_map_cpu_copy_ns`: wait for submitted work, map completion, CPU row
  copy and unmap. Not a pure GPU duration or pure transfer duration.
- `total_completed_frame_ns`: the sum, from begin to CPU-accessible RGBA frame.

Adapter/device creation, parsing/building, four completed warmups per case,
correctness guards, frame destruction, sample storage and file IO are excluded.
Frame handles are dropped before the next measured frame to preserve reusable
readback allocation semantics. Six cases execute in a fixed order, not randomized;
thermal/order effects remain. Repetition at t=0 does not assert any cache hit.

## Identity and reproducibility

Actual `renderer.device().adapter_info()` fields are recorded: name, vendor/device,
backend, driver/info and device type. CPU adapters are rejected unless explicitly
allowed. Virtual/Other adapters are marked unqualified, never asserted hardware.
`hardware_reported` records WGPU's classification, not an independent hardware
attestation. Compare only like device/driver/backend/configuration environments.

The versioned manifest freezes six generated source SHA256 hashes. A Rust
contract also freezes source bytes with FNV-1a snapshots (not cryptographic). The wrapper
also hashes input source bytes, renderer/engine/vendor source trees, lockfiles,
workspace/renderer manifests and driver. Dirty state, revision, OS/architecture,
kernel, compiler version, logical CPUs, starting system load and SHA256 values
of a small environment-key whitelist are
recorded. This is provenance, not a promise of reproducible binaries or comparable
performance: GPU power/thermal state, compositor load and external dependencies
are not fully captured. No full environment dump or raw environment values are
collected. Build logs can contain local paths or compiler diagnostics; review them
before sharing.

## Correctness before measurements count

Every returned measured frame is checked outside timing: dimensions/buffer length,
opaque grayscale, center/background, four-axis interior/exterior extent, and two
soft-edge probes (or sharp edges for plain). Incorrect or missing blur fails the
whole run. CI integration tests use the same source and guard, exercise all six
cases, and inject blank/malformed/wrong-size/wrong-effect outputs. GPU correctness
may use software rendering; that does not produce performance evidence. CI compiles
the example and runs contracts only, never the timed driver. Python tests cover
schema/completeness/duration failures, software policy, timeout and overwrite refusal.

Broad pixel tolerances are semantic guards, not exact cross-device goldens. Their
real renderer acceptance must be verified on the pinned build; static inspection
alone is not GPU validation. Existing benchmarks and thresholds remain unchanged.
