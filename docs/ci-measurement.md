# Measuring CI before changing it

The original instrumentation baseline retained gate names, commands, feature
sets, shell selection and the **Check & Lint** order below. The current explicit
gate-list dispatcher can combine the adjacent scene/budget pair as described
below; this is an optimization beyond that no-op baseline:

```text
audit → scene-audit → budget-audit → check → lint → doc → no-video
```

A timing change is instrumentation, not evidence of a speedup. Gate totals,
subcommand phases and runner preparation answer different questions; report them
separately. Wrapper startup and artifact-writing overhead remains in the gate
total, outside the child phase duration. Nested phase durations are already included in their gate total, so
do not add both together. Failed or interrupted runs are not successful samples.

## What to observe

| Gate | Useful boundaries | Interpretation |
| --- | --- | --- |
| scene-audit | `build-cli` (cargo); `audit-corpus` (execution) | The corpus phase includes enumeration, worker startup and validation, not only engine execution. |
| budget-audit | `build-cli` (cargo); `check-budget` (execution) | Standalone calls retain their own build, even on a cache hit. The adjacent union records shared work instead. |
| scene-budget-audit | `build-cli` (cargo); `audit-corpus` (execution) | Shared scene/budget union, never two invented independent gate durations. |
| no-video | `default-binaries`; `minimal-engine`; `minimal-dependency-graph`; `animatix-cli-feature-graph`; `animatix-gui-feature-graph` (all cargo) | Keep default and no-default-feature operations distinct. A working FFmpeg-equipped Nix environment does not prove FFmpeg-free compilation. |
| doc | `rustdoc` (cargo); `property-reference` (mixed) | `cargo run -p animatix --example property_reference -- --check` includes build **and** execution. Do not label it execution-only or invent a split. |

The phase wrapper interface is:

```text
python3 tools/ci/phase-timing.py --gate NAME --phase NAME \
  --kind cargo|execution|mixed -- COMMAND...
```

Each invocation writes a separate UUID-named JSON file under
`ANIMATIX_PHASE_TIMINGS_DIR` (default `target/ci/phases`). Schema version 1 records
`gate`, `phase`, `kind`, UTC `started_at`/`finished_at`, monotonic
`duration_seconds`, `exit_code`, `signal` and a sanitized
Cargo-environment allowlist. Signal termination uses a signal value rather than
claiming an ordinary exit code. Command arguments and captured output are not
stored; stdout/stderr remain attached to the original check. Artifact-write
failure emits a warning without changing the check result. Missing phase records
therefore mean incomplete measurement, not a successful or zero-duration phase.

Keep phase names stable across runs. Record elapsed monotonic time and exit status;
UTC timestamps are for correlation. Preserve the subprocess failure status and
normal output, including failure from an inner shell function. A timing wrapper
must not turn a failing check green or weaken shell `errexit`. For these
noninteractive POSIX checks, the child has an isolated process group and normal
cancellation is forwarded once to that group, including descendants. The
Windows fallback forwards only to the direct child; this phase instrumentation
is currently applied to Linux/Nix gates, not interactive terminal sessions.
Forwarding covers SIGINT/SIGTERM, not SIGHUP or an uncatchable SIGKILL sent to
the wrapper. Descendants that create their own process groups are outside this
guarantee. Local cancellation tests are not evidence of a real GitHub-hosted
cancellation run; that integration remains to be observed.

Preparation records should distinguish eligible, skipped, started, completed and
failed steps where the observation supports that distinction. Eligibility alone
is not cache-hit evidence or a duration. For hosted runs, compare cache restore,
Nix setup, gate execution and cache save separately using their actual step
records. Queue time is not compiler time. Do not dump environment variables,
credential-helper output, tokens, authenticated URLs or shell traces into timing
artifacts; retain only an allowlist of non-secret configuration and tool facts.

## Same-invocation scene/budget union

Only an explicit gate list with adjacent `scene-audit budget-audit`, in that
order, combines the checks. A nonempty `ANIMATIX_BIN` or
`ANIMATIX_AUDIT_BUDGET` disables combination. Standalone gates, reverse order,
`all` dispatch and budget `--update` retain their existing behavior. Repeated
pairs each execute afresh: there is no cross-invocation cache or reused report.

The shared execution builds once and enumerates the corpus once. Each scene
receives one CLI audit with default policy, render `never`, the default budget
and JSON output. CLI exit status enforces the budget; scene validation additionally
requires schema 3, kind `audit`, and zero summary errors using the existing
missing-errors default. The two original scene walks are regression-tested against
the same exclusions, multi-file projects and paths containing spaces. A failed
build, enumeration, CLI invocation or JSON validation cannot complete the pair.
Both completion labels are printed only after the full shared check succeeds.

Joint execution exposes CLI stderr, as the standalone budget pass does; unlike
the standalone scene pass it does not discard those diagnostics. It records one
`scene-budget-audit` gate total and shared phase records, not independent timings
for work executed only once. CI Required still checks the selected matrix job's
actual result, not timing labels. Do not add shared and standalone totals or
interpret absence of individual timing rows as skipped safety checks.

To compare duplication removal specifically, run these from the same restored
cache state and with both overrides unset:

```bash
# A: independent production gates, exactly as before union dispatch.
nix develop --command bash -e -c \
  'tools/ci/ci.sh gate scene-audit; tools/ci/ci.sh gate budget-audit'
# B: shared work, same validation contracts.
nix develop --command tools/ci/ci.sh gate scene-audit budget-audit
```

Compare A's sum with B's shared total and wall time; correctness fixtures establish
call reduction, not a measured end-to-end speedup. The broader gate-order experiment
below is a separate experiment: both orders now use the same union optimization.

## Controlled local A/B

Use a dedicated checkout at one immutable commit, with a clean environment
outside any existing Nix/direnv shell. Keep `flake.lock`, `Cargo.lock`, CPU limits,
parallelism, runner image, toolchain, Cargo profile and flags fixed. Record the
commit, machine facts, experiment order and cache provenance beside the timing
files. Do not use the developer's active workspace or competing builds.

Baseline A is the order above. A small **candidate**, not a CI change, is B with
only `check` and `lint` exchanged. It tests whether their shared incremental work
or earlier diagnostic failures alter total time. Keep every original gate and
argument; never replace the two operations with a broader command or omit a
successful gate. Run the full sequence, not just the apparently faster step:

```bash
# A; run from a clean shell at the repository root.
nix develop --command tools/ci/ci.sh gate \
  audit scene-audit budget-audit check lint doc no-video

# B; local experiment only, not the registered CI order.
nix develop --command tools/ci/ci.sh gate \
  audit scene-audit budget-audit lint check doc no-video
```

Before each trial, restore the **same** target/cache seed to the **same** paths.
The gates currently execute the CLI under `target/debug`, so do not assume an
arbitrary `CARGO_TARGET_DIR` is supported. Archive measurements outside `target`
before resetting this disposable checkout's outputs. Do not wipe shared Nix
stores or shared developer caches for an experiment.

Measure two strata independently:

- **Cargo-cold:** an empty disposable target directory, with identical prefetched
  Cargo sources and Nix closure. This is not a network-cold or fresh-runner test.
- **Cargo-warm:** restore an identical frozen target snapshot before every trial;
  record exactly which command sequence produced it. Do not let A's outputs seed
  B while calling both runs equivalent.

Use at least three successful repetitions per order and stratum, alternating
A/B then B/A to expose machine drift. Keep rust-cache and any compiler-cache
configuration fixed; disable untracked compiler caches or freeze their seeds.
Separate cache downloads/network failures from command execution. Report medians,
ranges, full-job elapsed time and phase distributions, plus failed runs separately.
A single hosted run or a warmed B after a cold A is not a useful comparison.

## Limits and decision rule

Cargo artifact reuse depends on package selection, target kind, profile, compiler
flags and resolved features. Workspace feature unification can produce different
artifacts from the minimal engine check; build, check, Clippy and rustdoc are not
interchangeable. Changing feature flags to improve reuse changes the experiment
and potentially the safety contract. Keep scene concurrency and the corpus fixed.

First collect a baseline with the dispatch mode explicitly recorded. Try an
ordering candidate only if phase
measurements identify an actionable cost. Promote a change only after repeated
full-gate trials show a consistent improvement larger than run-to-run variation,
with unchanged checks and acceptable time-to-first-failure. Otherwise keep the
current order. The documentation gate and documentation-link job keep their
existing Nix shells; a smaller shell is a separate dependency audit, not a timing
optimization justified by these measurements.
