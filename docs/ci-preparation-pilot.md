# CI preparation pilot (not a production change)

Source base: `0dd37ec787db1be22692e87ab3dfb7a8370b0c7c`.
Only `ci-prep-experiment/20261116` triggers the pilot. All arms use the same
experiment SHA. Production `ci.yml`, its matrix, required checks and publishing
remain unchanged. The pilot does not substitute for production acceptance.

## First phase: cleanup only, no cache writes

Repository cache usage was 10,204,110,737 bytes (about 9.50 GiB) at the initial
read-only inspection. Namespaces share storage quota, so creating more large
cache entries could evict production caches. The new-cache experiment is paused.
No cache is deleted. The active workflow has mode `cleanup-only`:

- A: original unconditional runner-image cleanup.
- B: cleanup only when the workspace filesystem has less than 30 GiB available.
- Both run the complete registered Check & Lint gate sequence, unchanged.
- Two rounds (`sample1`, `sample2`) use separate fresh runners, two concurrent
  jobs per round. These are repetitions against an existing cache, not cold/warm.

Thirty GiB is a conservative candidate, not an optimized or proven requirement.
B fails if cleanup cannot supply it. Both retain exactly the production cleanup
path allowlist. No Nix store, Cargo cache or developer files are removed.
Cleanup requires a GitHub-hosted Linux runner on an isolated experiment branch.

Rust cache restores the existing production identity (`v0-rust`, shared key
`<shell>-gate`), with `save-if: false`, `cache-workspace-crates: false` and
`cache-on-failure: false`. This is read-only reuse, not a new namespace. Inspect
the full restored key and compressed size in each job: GitHub cache eviction or
other concurrent workflows can alter availability. A miss is not a warm sample;
do not compare unlike seeds as though they were equal. The workflow does not
freeze or modify other workflows' caches.

The Nix installer action can itself save its downloaded installer. To avoid
that write too, both arms download the same official v3.24.2 binary directly,
verify its pinned SHA-256 and exact size, then pass it through `source-binary`
to the reviewed pinned action. That action path bypasses its cache downloader.
This common setup differs slightly from production and is included in timings.
Cachix uses `skipPush: true` without a write token. There are no other cache-save
actions. Local ephemeral build output and public measurement artifacts are not
persistent build caches.

Permissions are contents:read/actions:read. There is no signing/Cloudflare
permission, credential output, production workflow call, release or deploy ref
write. Failed or skipped pilot jobs fail the experiment result, not production.

## Deferred cache grouping candidate

The helper retains `cache-pilot` mode for later reviewed work, but the workflow
does not select it. It is blocked unless observed existing cache usage is at
most 5 GiB; this is a conservative experiment budget, not proof of actual account
quota. Its proposed C arm adds actual Nix rustc/cargo, flake.lock, shell and job
to the cache identity while leaving workspace-crate caching disabled. Standard
lockfile/environment hashes remain. Collecting identity realizes the Nix shell
early, so count that time in the full job rather than claiming a faster gate.
Do not enable this mode until there is a safe, bounded storage plan.

## Measurement and decisions

JSON evidence uses monotonic cleanup/run durations, before/after filesystem
free bytes, five-second minimum-free samples, explicit cache-hit status and
allowlisted SHA/runner-image facts. Existing gate/phase timings remain. Actions
step logs provide compressed restored cache bytes; no new-cache upload size is
expected. Do not upload raw environments, credentials or the complete target.

Compare full job elapsed, queue time, cleanup, Nix setup, restore, compilation
and execution. Verify identical source, image/toolchain, cache key and gate
verdicts before comparing A/B. Missing data is unknown, not zero. A sampled
low-water mark is not an exact disk peak. The experiment's sequential-round
barrier is not production wall time, and a faster single branch only shortens
production if it remains on the critical path.

Two successful samples per arm are an exploratory signal, not a stable speed
claim. Repeat only if useful, then review expansion to Web Build and Test &
Examples. Main adoption needs separate approval and complete production-matrix
validation. Reverting the experimental branch cannot alter the published site.
