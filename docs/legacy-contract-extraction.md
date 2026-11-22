# Legacy scene contracts: first extraction

This phase detaches three consumers from the old Web artwork. It does not delete
old scenes, shrink the legacy SDK corpus, change audit budgets or replace the
canonical site's checks.

| Old input / consumer | New owned input | Preserved contract |
| --- | --- | --- |
| Transformer tokens, host marker test | `crates/animatix-web/tests/fixtures/timeline_markers.amx` | All nine authored timestamps; existing six required marker checks, point durations and separate composition checks remain. No modules or text shaping needed. |
| Sorting sort, formatter AST test | `crates/animatix-syntax/tests/fixtures/sorting_format.amx.txt` | The complete insertion-sort workload is migrated to current function, plan, call and timing syntax. The fixture is byte-identical to the current Web scene; formatter output reparses to the same scrubbed AST. Imports are syntax only, not resolved here. |
| Transformer attention, native presentation driver | `crates/animatix/examples/web_present_driver/fixtures/attention.amx` and `lib/` | The complete workload is migrated to current syntax, retaining actor identities, text, geometry and cue timing. The fixture keeps its relocated source and three library inputs pinned by hash. Its original paths are recorded for Git history lookup. |

The manifests preserve the pre-migration hashes as historical provenance and
record separate hashes for the migrated current source and relocated fixture.
The attention import closure remains exactly theme, stage and components, with
no further imports. The driver still uses the same sample times, presentation
sizes, render quality, paired arms and warmup counts. The basic-shapes input,
renderer and font dependencies are not frozen by this change: this is not a
claim of pixel-identical performance across revisions.

`tools/quality/test-legacy-contract-fixtures.py` checks historical provenance,
migrated source hashes, exact source-to-fixture relocation, module identity,
closure and marker timestamps in `ci-policy-tests`. The existing Rust marker
and formatter tests remain the semantic gates. Keeping the full sort syntax is
deliberate: reducing it would weaken a real-world formatter regression.

Acceptance for this extraction requires the marker and formatter Rust tests and
a native presentation-driver run on a GPU-capable machine. The extraction's
source-identity checks alone are not runtime or pixel-equivalence evidence. Do
not delete the old corpus until those checks and its remaining consumers have
been addressed.
