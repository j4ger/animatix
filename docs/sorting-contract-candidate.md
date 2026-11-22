# Sorting contract candidate

This additive fixture is **not acceptance evidence for retiring old scenes**.
The old [sort.amx](https://github.com/j4ger/animatix/blob/fb1aee420f42e86e08f95f3e8e223394d9955056/web/demos/sorting/sort.amx), formatter input, SDK legacy inventory and
all audit budgets remain unchanged.

## Mapping

`crates/animatix/tests/contracts/sorting.amx` reduces the old sorting mechanism
to four absolute-position bars and one live text readout. It retains nested
step-sequenced function loops, mutable permutation state, dynamic actor indexing,
position assignments, build-time swap flags and frame-time nested reductions.
It intentionally excludes the old artwork, camera, captions and imports.

The shared `sorting_oracle.rs` checks original actor identities at initial,
intermediate and final times, preserving each bar's original height/y coordinate.
It also requires exactly five actors, 40-pixel bar widths and a five-second duration.
For values `[4, 1, 3, 2]`, the sorted identities are `[1, 3, 2, 0]`. Four
swaps complete; six comparisons run. Readout samples include pre-start, early,
intermediate and final states, rather than accepting a constant final tally.

| Directed fault | Required rejection |
| --- | --- |
| Drop permutation updates | Final actor positions |
| Change inner loop step from 300 to 600 ms | Intermediate actor positions/timing |
| Store every swap flag at index zero | Live readout |
| Replace live readout with `6/4` | Pre-start and intermediate readout |
| Change bar heights | Size |
| Add an extra actor | Actor count |
| Change configured duration | Duration |

Every mutant must parse/build successfully before its semantic rejection counts.
Build failures are failures of this test, not a successful negative control.
Both native engine and SDK tests run the same positive/negative oracle. CLI
acceptance uses `check` and strict structural `audit --render never`; it does
not claim the CLI command exposes or verifies the numerical frame oracle.

## Required acceptance

The engine cases are registered in the existing `engine_contracts` harness
(`animatix` disables automatic integration-test discovery). SDK and CLI retain
their automatically discovered integration targets:

```sh
cargo test -p animatix --test engine_contracts sorting_contract::
cargo test -p animatix-web --test sorting_contract
cargo test -p animatix-cli --test sorting_contract
```

Also run formatting and the existing legacy marker/formatter, SDK corpus and
audit gates. On 2026-11-16, the isolated e7ac9a6a-based candidate passed the pinned Nix
fmt and structure gates, engine sorting tests (2), native SDK sorting tests (2),
CLI check/audit contract (1), all engine contracts (117), SDK scene contracts
(4, including the legacy corpus), and the real-scene formatter regression (1).
The seven directed mutants build successfully and are rejected by both engine
and SDK semantic oracles. SDK tests here are native host tests, not browser
rendering evidence; CLI audit explicitly skips GPU rendering.

Validation corrected fixture subtraction spacing, the public `world()` accessor,
and oracle units: geometry tracks use half-extents, and declared duration is
distinct from the last authored keyframe. Physical size, time, position and
readout expectations were not loosened. If the positive fixture
fails, investigate clock or binding semantics before changing expectations.
If an existing bug is exposed, keep the old scene and record the failure; do not
weaken budgets or mark the old regression retired.
