# Maintenance and CI work: findings and decisions

**Report date:** 2026-11-15 local (prepared 2026-11-14 21:20:24 UTC). **Implementation/census cut:** clean commit `bd34463df14fc94b68a411d84285c0d7e0774f5d`. The CI topology trials ran at source `1152395a68715739d87df667051557f5ad53e5da`; all nine finished and the default decision is settled. Measurements, run provenance, source hashes, and census JSON are in the sibling [`maintenance-ci-evidence.json`](maintenance-ci-evidence.json).

## Decisions

- **Use the two-worker Cargo test scheduler.** Cargo2 qualified in both the local default-profile baseline/Cargo2 comparison and the hosted debug=0 five-strategy comparison. Keep the measured profiles separate; do not combine their percentages.
- **Keep Linux LLD and macOS Apple ld.** The user stopped the Mold migration for low benefit. The valid default-profile Linux CLI and GUI measurements did not meet the five-second gain floor. Remaining Linux-test and macOS workloads were user-canceled and have no compatibility verdict.
- **Keep the font-profile and contract-layout changes.** The measured commands/cases passed their initial criteria, followed by integrated production validation. These results do not establish whole-workspace or hosted savings.
- **Keep generated property-reference tables.** They remove 560 hand-authored semantic cells while retaining the docs-only group map and prose. The check has a real cold compile cost; this is a maintenance tradeoff, not a speedup.
- **Keep the baseline CI topology.** Three same-source runs per arm reject both candidates: grouped is 17.25% slower on critical path despite 8.03% fewer runner minutes; shared-CLI is 3.57% slower and uses 3.45% more runner minutes. All nine runs succeeded, so first-failure feedback is unobserved (`null`), not zero.

## Changes and compatibility

- Authored-type exceptions moved from syntax name matching into existing core property descriptors. There are still 140 `PropertyId` rows; a complete legacy 140-row fixture pins order and authored type. `PropertyDescriptor::new` retains its three-argument form and a const builder sets exceptional hints. The added public descriptor field affects downstream struct literals; those callers must initialize it. `PropertyId` serialization/order, descriptor value kinds, and engine flags did not change.
- Sixteen property tables now derive rows from existing syntax and engine metadata. The docs-only group map still names all 140 properties to preserve presentation grouping/order. Editorial prose and scene/internal-only explanations remain handwritten. Engine binding metadata remains a separate source for runtime storage/default/read/flag facts. Authored `Any` remains `Any`; it is not narrowed to runtime storage type.
- The audited PID-only/shared-directory test fixtures now own unique temporary directories with RAII. No existing regression assertion was removed; four invalid-motion cases were grouped with each assertion retained. The ownership audit covers all 19 workspace packages, Node/browser checks, and 13 Python unittest modules. It is an ownership/isolation inventory, not an executed-coverage claim; see [`test-maintenance-audit.md`](test-maintenance-audit.md).
- CI path selection, shared artifact handoff, and topology measurement were added. PR selection is dependency-aware; main/release remain full-workspace paths. No frame-path runtime algorithm or ECS storage redesign was part of this maintenance work. Some production comments were corrected to match existing dispatch behavior.

Implementation checkpoints: `bd009448` hardened CI identities and SDK artifact fallback; `7a4e22cf` moved authored type hints into descriptors and generated property-reference tables; `13f3a95f` refreshed docs and added generated-region/orphan-marker checks; `9d290bdf` added the two-worker Cargo scheduler; `957588dc` locked the Mold candidate dependency graph; `1152395a` pinned every selected checkout to the triggering commit SHA; `bd34463d` finalized the collector and source census.

## Maintenance cost and source census

The census compares baseline `f51a7f40ea912e3a4d5093f8bfab90ac52728b5b` with clean implementation cut `bd34463df14fc94b68a411d84285c0d7e0774f5d`, before adding this archive report, evidence file, and index entry. It counts physical tracked Rust lines and marker lines; inline tests belong to their containing source-file path. These counts do not show what executed or what behavior was covered.

| Tracked Rust | Baseline | Final | Change |
|---|---:|---:|---:|
| Files | 1,082 | 1,086 | +4 |
| Physical lines | 259,148 | 259,526 | +378 |
| Comment-only lines | 27,354 | 27,384 | +30 |
| Blank lines | 20,474 | 20,489 | +15 |
| `#[test]`-family attributes | 2,498 | 2,500 | +2 |
| `#[cfg(test)]` attributes | 358 | 361 | +3 |

The tracked diff at that cut has **11,404 additions and 1,112 deletions** (+10,292 net across 110 paths). Major categories: experiment scripts +6,347; CI control/workflows +1,103 net; property-reference generator/group data +620; docs +590 net; other Rust source −20 net; other +1,635; dependency metadata +17. The archive files added afterward are outside those counts. This work does not support an overall LOC-saving claim.

Before generation, 16 Markdown tables held 700 hand-authored cells, including 560 non-name semantic cells. After generation there are 140 rows, 16 generated regions, and 840 cells across six columns. The remaining 140 names in the JSON map express grouping/order, not duplicate type/flag facts. The 21 former syntax name overrides now live as descriptor hints.

## Test-runner measurements

### Local default profile

Source and driver `bca7c88fb810eb1694f703c614c68190f9569df7`; Nix Rust/Cargo 1.99.1. The selected cohort compares only baseline and Cargo2, not all five strategies. The measured warm suite used the precompiled artifact manifest prepared at `03a09bee`; Rust/Cargo inputs were verified identical through bca7. The driver and source fingerprint are bca7. This distinction does not turn the warm samples into a fresh full-workspace compile at bca7; the separate cold builds listed below were run there.

| Arm | Five full-suite samples (seconds) | Median | Maximum tree RSS |
|---|---|---:|---:|
| Baseline | 192.242, 183.793, 185.368, 178.509, 180.424 | 183.793 s | 1,152,925,696 B |
| Cargo2 | 152.098, 152.772, 145.706, 140.008, 142.317 | 145.706 s | 1,044,275,200 B |

Cargo2 was 20.72% faster by ratio of medians; maximum RSS was about 9.42% lower. Both arms passed all five rounds with the same 1,436 sensitive IDs; active test-ID coverage matched 2,525. Three separate cold `cargo test --workspace --no-run --locked` builds passed (357.360, 348.907, 378.916 s); the examples check passed. Optional CLI+GUI video correctness passed in 53.60 s. Per-round RSS and artifact-manifest hashes are in the evidence JSON.

### Hosted Linux, debug=0

[Run 43586963283](https://github.com/j4ger/animatix/actions/runs/43586963283), source `bca7c88`, Ubuntu 24, Rust/Cargo 1.99.1, `CARGO_BUILD_JOBS=2`. It completed 25 full-suite rounds (five per arm) and 100 separate sensitive stress rounds (20 per arm). Every full round covered the same 2,525 active IDs; every stress round covered 1,436 IDs and passed.

| Strategy | Five full-run times (seconds) | Median | Maximum tree RSS | Change vs baseline |
|---|---|---:|---:|---:|
| Baseline | 136.062, 127.347, 127.524, 127.396, 127.273 | 127.396 s | 1,090,207,744 B | Reference |
| Cargo2 | 113.315, 113.313, 113.318, 113.397, 113.388 | 113.318 s | 967,131,136 B | 11.05% faster; qualifies |
| Cargo4 | 116.141, 115.327, 115.413, 115.294, 115.440 | 115.413 s | 997,679,104 B | 9.41%; below 10% threshold |
| Nextest2 | 144.774, 144.393, 144.600, 144.727, 145.343 | 144.727 s | 860,561,408 B | 13.60% slower |
| Nextest4 | 131.695, 132.476, 131.273, 131.365, 131.374 | 131.374 s | 953,090,048 B | 3.12% slower |

Cargo2 was the only hosted arm to meet the 10% time threshold; its maximum tree RSS was 11.29% below baseline. Cargo4 did not qualify. Both Nextest arms were slower; their lower RSS does not offset that measured regression under the selected threshold. The hosted runner's Nextest executable identities were checked before and after samples. Rustdoc status replay found 29 IDs per execution: two passed and 27 ignored, with no failures. These are status IDs, not 29 executed passing tests.

The hosted summary also carries posthoc Nix-store hashes for the LLD, Mold, and BFD delegates. They were validated after the run, not captured on the hosted VM, and are corroborative identity evidence rather than performance evidence. The hosted debug=0 LLD delegate SHA itself was not recorded during that run. Keep these Nix-provided Rust 1.99.1 measurements distinct from the macOS jobs pinned to Rust 1.100.0.

## Build/check and generated-reference costs

The font-profile experiment changed `profile.dev.package.sha2.opt-level` from default to 3 (also inherited by tests); release/dist was unchanged. Five paired rounds measured:

| Command | Before median | After median | Peak tree RSS before → after |
|---|---:|---:|---:|
| CLI check | 55.50 s | 2.43 s | 254,136,320 → 252,530,688 B |
| Text cache test | 58.18 s | 2.54 s | 229,126,144 → 228,786,176 B |

These are initialization-sensitive commands, not whole-workspace savings. Contract layout processed the same 91 cases in five pairs: separate median 10.56 s / 259,616,768 B versus consolidated 3.22 s / 267,272,192 B (+2.95% RSS). The recorded decision was to adopt consolidation with that memory tradeoff.

The property-reference check compared the handwritten docs check with that check plus the generated Rust example. It ran three cold and five warm rounds:

| Phase | Handwritten | Generated |
|---|---:|---:|
| Cold median / maximum tree RSS | 1.404 s / 18.2 MB | 209.875 s / 3.04 GB |
| Warm median / maximum tree RSS | 1.435 s / 17.7 MB | 1.558 s / 104.0 MB |

Cold compile is expensive; warm checking adds about 0.123 s and roughly 86 MB peak RSS. The experiment excludes `cargo doc`. Four property-reference example tests passed, and repeated write/check validation was idempotent with no second-run document change. The native doc gate passed. The generator's value is removing duplicate semantic facts, not speeding up documentation checks.

## Linker measurements and decision

### Hosted debug=0 diagnostics

[Run 43586963283](https://github.com/j4ger/animatix/actions/runs/43586963283), source `bca7c88`, Ubuntu 24, Rust/Cargo 1.99.1, debug=0 and two Cargo jobs. Linux CLI, GUI, and workspace-test measurements completed and compatibility checks passed. This is diagnostic evidence, not default-linker adoption evidence; do not pool with debug=2. Medians in seconds, cold / warm:

| Workload | Default LLD | Mold | System BFD |
|---|---:|---:|---:|
| CLI | 136.66 / 3.58 | 139.88 / 3.29 | 149.40 / 10.06 |
| GUI | 274.87 / 3.25 | 282.58 / 2.94 | 300.57 / 17.85 |
| Workspace tests (`--no-run`) | 320.90 / 68.79 | 322.49 / 70.16 | 412.16 / 159.17 |

Mac linker jobs in that debug=0 cohort failed during toolchain setup before timing; they supply no macOS compatibility samples. This is separate from the later user-stopped default-profile cohort.

### Default-profile Linux CLI and GUI only

[Run 43610359036](https://github.com/j4ger/animatix/actions/runs/43610359036), source `957588dc`, Ubuntu 24, Rust/Cargo 1.99.1, debug=2, four jobs, serial measurements. Only Linux CLI and GUI are valid; each summary has 24 records, forced fresh links, matching output identities, and passing compatibility checks.

| Workload | Default cold / warm | Mold cold / warm | System BFD cold / warm |
|---|---:|---:|---:|
| CLI | 137.179 / 5.422 s | 137.130 / 5.452 s | 148.491 / 15.646 s |
| GUI | 174.681 / 6.060 s | 179.901 / 5.208 s | 190.844 / 19.560 s |

Recorded tool identities were LLD 23.1.1, Mold 2.40.4, and GNU ld/Binutils 2.44; per-sample executable hashes, delegate hashes, wrapper identities, and link counts are in the evidence JSON. Neither valid workload demonstrates a positive five-second gain. Keep Linux LLD and macOS Apple ld. The remaining Linux workspace-test and three macOS workloads were canceled by user direction, not compatibility failures. No Mold replay is planned.

## CI topology: keep baseline

CI9 compared baseline, grouped, and shared-CLI in nine successful workflow_dispatch runs at source 1152395a68715739d87df667051557f5ad53e5da. Critical path is the observed interval from Plan job creation through CI Required completion. Runner minutes are the sum of job start-to-completion intervals, not GitHub billing-rounded usage.

| Topology | Critical-path samples (seconds) | Median | Runner-minute samples | Median |
|---|---|---:|---|---:|
| Baseline | 1,325; 1,019; 1,177 | 1,177 s | 92.88; 86.37; 83.10 | 86.37 min |
| Grouped | 1,116; 1,521; 1,380 | 1,380 s (+17.25%) | 71.08; 79.70; 79.43 | 79.43 min (−8.03%) |
| Shared-CLI | 1,278; 1,219; 1,169 | 1,219 s (+3.57%) | 89.35; 85.98; 90.37 | 89.35 min (+3.45%) |

All nine runs reached successful CI Required; no publish job ran. First-failure feedback is unmeasured (null) because no workflow failed. Grouping reduces aggregate gate-step work (3,735s median versus baseline 4,001s) and runner minutes, but serializes native gates in the Native Grouped job; two of three grouped critical paths go through that job before CI Required. It extends wall time. Shared-CLI producer/consumer artifact handoff passed, but its critical path also did not improve. Keep the baseline. Neither candidate meets the 10% median critical-path improvement threshold; runner-minute changes are secondary to that failed requirement. The full run IDs, selected jobs, per-job times, host details, and raw job JSON hashes are in the evidence file.

The compared Linux native/Web paths used the same source SHA, Ubuntu 24, and Rust/Cargo 1.99.1, but hosted image versions included 20261111.127.1 and 20261111.370.1, with AMD EPYC and Intel Xeon hosts. The full matrix also retained the macOS and Windows eparts jobs. This variation confounds small timing differences. The grouped serialization is a clear topology difference consistent with the observed longer critical path, though three runs per arm do not isolate host variability. Neither candidate showed a median time gain to resolve with additional pairs, so both remain negative conclusions. An older baseline [run 43582061297](https://github.com/j4ger/animatix/actions/runs/43582061297) is one 5ab-source sample (22m43s workflow boundary, 94.75 summed job minutes) and is excluded from CI9.


## Validation and reproducibility

At final collector commit `bd34463d`, the production test gate passed in 145.208 s across 33 harnesses with exact coverage of 2,526 active IDs and passing doctests. Check and lint passed; fmt, structure, workflows, ci-policy-tests, and doc-links also passed. The CI policy modules reported 16 + 19 + 6 + 75 passing cases; the workflow-boundary suite reported 8. The no-shell ci-plan smoke selected its shadow matrix and completed without Rust/Cargo probes. At `9d290bdf`, the examples gate built the CLI and checked 63 DSL `.amx` files in 184.853 s, with one allowed warning. Separately, the workspace --no-run JSON contained seven Rust example compiler-artifact targets; the command compiles those targets but does not execute them. These are correctness results, separate from timing samples.

At source `957588dc`, `wasm-check` passed in the registered `web` shell. All seven registered Web gates passed in `web-ci`: `web-split-proof`, `web-build`, `web-sdk-integrity`, `text-parity`, `web-raster-proof`, `web-smoke`, and `web-transactions`; the last passed all 28 transaction checks. This was correctness validation, not a performance comparison. `web-build` exited successfully with duplicate generated split-key and missing upstream license-text warnings for three packages; those warnings are captured in the gate log. The built SDK identity was `be862e1551c89a3e4fe19b52cf3ca991b8de2516cb20d98f526b6236071cc82c`.

The gate shell assignments below follow `scripts/ci.sh`'s registry; run a gate in its registered shell and preserve its environment. See the [experiment entry points and cohort settings](../contributing.md#ci-selection-and-build-experiments) and [descriptor migration and generation commands](../contributing.md#property-metadata-and-generated-reference).

| Gate set | Registered gates | Required shell and invocation | Environment scope |
|---|---|---|---|
| Native | `fmt`, `lint`, `check`, `test`, `no-video`, `render-smoke`, `examples`, `ext-bench`, `doc`, `doc-links`, `ci-policy-tests`, `structure`, `workflows` | `nix`: `nix develop --command bash scripts/ci.sh gate <gate>` | Nix native toolchain and system libraries; no ad hoc Cargo profile overrides |
| Wasm | `wasm-check` | `web`: `nix develop .#web-build --command bash scripts/ci.sh gate wasm-check` | wasm32 target and pinned wasm-bindgen CLI |
| Web checks | `web-split-proof`, `web-build`, `web-sdk-integrity`, `text-parity`, `web-raster-proof`, `web-smoke`, `web-transactions` | `web-ci`: `nix develop .#web-ci --command bash scripts/ci.sh gate <gate>` | wasm tooling, Node, Chromium, and web test dependencies |
| Manual experiments | Runner, linker, font, contract, reference, topology | Local work ran in the registered Nix/web shell; hosted work ran in the recorded GitHub Actions environment | Source SHA, toolchain, profile, worker count, arm selection, and owned output/target paths are cohort metadata; no global target was cleaned |

Do not use canceled run `43574493170` for timing. Merged Mesa stderr made the old parser miss 57 renderer IDs, and the default-linker warm sample did not prove a forced relink. Replaying the raw logs corrected test-ID parsing to 2,525 active IDs; it does not validate that timing cohort. Later parser tests cover actual Cargo stream routing and Rustdoc multi-block output.

The evidence JSON embeds the source census, per-arm samples, final topology run data and provenance, validation results, and captured summary/log hashes. Tool identities that were posthoc or unavailable are labeled as such. Invalid or user-stopped cohorts are listed separately and are not pooled into adoption results.
