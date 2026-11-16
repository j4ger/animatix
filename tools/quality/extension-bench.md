# Extension benchmark health gate

`tools/quality/extension-bench.sh --quick --max-plan-ns 10000` is an **absolute
health sentinel plus sample-completeness check**, not a historical regression
comparison. It must produce these four fresh Criterion results:

- `property_plan_lookup_and_sample`
- `timeline_evaluate_0s`
- `timeline_evaluate_1s`
- `timeline_evaluate_2s`

The three timeline results must exist and be valid; no new timing threshold is
applied to them. The property-plan limit remains **10,000 ns applied to the lower
confidence bound of the typical estimate**, with equality passing. Criterion's
typical estimate uses slope when present, otherwise mean. This preserves the
metric selected by the old text parser, not a new mean/point-estimate budget.

The old parser read a rounded displayed value and understood only `ns`. The
new validator reads full-precision nanoseconds from JSON, so measurements near
the limit can differ from the old display-rounded decision. Microsecond output
no longer causes a parse failure. A noisy negative lower bound is not silently
reinterpreted as the point estimate; this remains a coarse health sentinel.

## Isolation and failure behavior

Each invocation creates a unique `target/ci/ext-bench.<random>/` evidence folder.
`CRITERION_HOME` points to its new `criterion` directory; `CARGO_TARGET_DIR` and
compiled artifacts are unchanged. Exact benchmark filters and `--noplot` limit
the run. `--save-baseline health` writes results locally, but there is no previous
result for any of these IDs in this directory, so no historical comparison occurs.
The gate never consumes or deletes the shared `target/criterion` cache.

Build and benchmark logs plus `summary.json` remain in that folder, including on
failure when produced. An EXIT trap stages only this run's three logs, summary,
and twelve expected raw result JSON files into an evidence subdirectory. Each
file is capped at 64 KiB (log tails, marked truncated JSON prefixes), plus a small
manifest recording exit status/truncation. No environment dump or broad target
upload is collected. CI uploads that exact dynamic path even after gate failure,
only for the ext-bench job, with seven-day retention. Pipefail propagates Cargo failures. Independently of Cargo
exit status, the validator rejects missing/extra rows, corrupt JSON, wrong IDs,
symlinked results, invalid sample arrays, nonfinite numbers, inconsistent sampling
mode/slope, or malformed uncertainty intervals. A unique empty result root, not
filesystem timestamps or a successful Cargo exit alone, establishes freshness.

The validator reads each row's `new/benchmark.json`, `new/sample.json`, and
`new/estimates.json`. Criterion can log write/comparison failures without failing
the benchmark process, which is why all four rows are checked explicitly.
`CARGO_CRITERION_PORT` is unset for these child commands because cargo-criterion's
socket reporting suppresses the required JSON files. Shell environment changes
remain confined to this script.

Exit codes: **0** valid samples and health limit satisfied; **1** health limit
exceeded; **2** invalid/missing results or usage. Cargo/build failures propagate
their original exit status. This does not claim performance improvements or gate
historical regressions; the separate saved-baseline comparison workflow is unchanged.

## Validation without measurements

```sh
python3 tools/quality/test-extension-bench.py
```

The existing `perf-policy-tests` gate runs these tests. Fake Cargo fixtures verify
unique result directories, an untouched partial old cache, preservation of the
compilation target, all four required samples, and nonzero exit propagation.
They are harness tests, not real Criterion timing evidence.

## Upstream contract checked

Pinned Criterion **0.5.1** sources:

- [Output-directory priority and JSON saving](https://github.com/bheisler/criterion.rs/blob/0.5.1/src/lib.rs): `CRITERION_HOME` overrides result placement. `--discard-baseline` also suppresses `new` JSON and therefore is not suitable here.
- [Analysis and result persistence](https://github.com/bheisler/criterion.rs/blob/0.5.1/src/analysis/mod.rs): current sample/estimate writes and automatic comparison are separate operations; comparison errors need not fail the benchmark.
- [Typical estimate](https://github.com/bheisler/criterion.rs/blob/0.5.1/src/estimate.rs), [CLI interval](https://github.com/bheisler/criterion.rs/blob/0.5.1/src/report.rs), and [WallTime nanoseconds](https://github.com/bheisler/criterion.rs/blob/0.5.1/src/measurement.rs) define the preserved metric and units.

Recheck this contract when upgrading Criterion rather than weakening validation
to accept missing data.
