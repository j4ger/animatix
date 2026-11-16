# CI preparation pilot (not a production change)

Source base: `0dd37ec787db1be22692e87ab3dfb7a8370b0c7c`.
The disposable branch `ci-prep-experiment/20261116` alone triggers the pilot.
All arms check out the same experiment commit. No production workflow, gate
sequence, release permission, branch protection, signer or publisher is changed.

## Scope and arms

The first pilot runs only the complete registered **Check & Lint** job, not a
substitute for the production matrix. The three arms are:

- A: original unconditional runner-image cleanup and shell-grouped Rust cache.
- B: cleanup only below 30 GiB free, same Rust cache identity as A.
- C: the B cleanup policy plus actual Nix rustc/cargo + flake.lock + shell + job
  cache identity. Workspace-crate caching remains disabled in all arms.

Thirty GiB is a conservative experimental threshold, not an optimized or proven
space requirement. B/C fail if cleanup cannot provide that headroom. Production
A's behavior is unchanged. Nothing removes Nix stores, Cargo caches or developer
files. Cleanup is restricted to GitHub-hosted Linux on the experimental branch.

A/B's compiled content is identical, so they share one immutable experiment
seed; C has a separate seed. A unique run/attempt prefix prevents restoring from
or writing production Rust cache entries. Both cold arms run with no seed in
that namespace; the warm round depends on all cold jobs finishing including
cache-save hooks, then restores those frozen seeds. Warm rounds do not save.
Inspect actual cache-match logs; a missing/evicted seed is not a warm sample.
This is Cargo dependency-cache cold, not network or Nix-cache cold.

The action still includes its standard lockfile/environment hash and host Rust
hash. C adds the true Nix compiler identity without altering PATH. It does not
yet eliminate unnecessary host-toolchain invalidation. Identity collection
realizes the Nix shell early, and its elapsed time belongs in total job time.
Do not claim a speedup merely because this moves time out of the gate step.

## Resource and safety boundary

The plan makes a read-only repository cache-usage query before launching any
build. Existing cache usage above 5 GiB blocks this pilot; the conservative
remaining allowance is intended for two approximately 1.4 GiB seeds plus margin,
not an assertion of the account's actual configured quota. Cache namespaces do
not isolate storage quota. Concurrent unrelated runs can change usage after
this check, and cache sizes are measured, not guaranteed. Stop expansion if
storage growth or eviction is observed; never delete unrelated cache entries.

Only three runners execute simultaneously. Cachix is read-only and receives no
write token. Token permissions are contents:read/actions:read; there is no
Cloudflare secret, signing permission or production workflow invocation. The
experiment does not create a release or change deploy/pages. A failure in any
arm/round fails the aggregate pilot. Results are observational until reviewed.

## Evidence and decision

Public JSON records contain monotonic cleanup/identity/run durations, snapshots
of workspace filesystem free bytes, five-second minimum-free sampling during
the gate, and allowlisted runner/cache facts. Standard gate and phase records
remain available. Do not upload raw environment, compiler diagnostic archives,
credential-helper output or complete target directories.

Use Actions job/step timestamps for total runner time and queue delay. For each
arm compare full job elapsed time (including identity, cache restore/save and
Nix initialization), not just gate duration. Report cache compressed bytes from
action logs and free disk before/after restore and minimum during gates. Report
all gate verdicts; failed runs are not speed samples. The cold matrix and warm
matrix each add scheduling barrier time, so experiment workflow wall time is not
an estimate of production wall time. A single-job pilot measures that branch;
recompute production critical path rather than subtracting savings from 18m29s.

One successful cold/warm pair is a feasibility check, not a stable speed claim.
If promising, repeat with fresh namespaces while respecting cache headroom, then
expand to Web Build and Test & Examples. The helper's `critical` scope exists
for reviewed expansion but is not selected by this workflow. Main adoption
requires separate approval, production full-matrix validation, and a rollback
that restores only the preparation change. Delete/disable the temporary workflow
when the experiment is finished; do not leave an always-on benchmarking service.
