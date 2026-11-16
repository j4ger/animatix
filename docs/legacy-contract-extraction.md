# Legacy scene contracts: first extraction

This phase detaches three consumers from the old Web artwork. It does not delete
old scenes, shrink the legacy SDK corpus, change audit budgets or replace the
canonical site's checks.

| Old input / consumer | New owned input | Preserved contract |
| --- | --- | --- |
| Transformer tokens, host marker test | `crates/animatix-web/tests/fixtures/timeline_markers.amx` | All nine authored timestamps; existing six required marker checks, point durations and separate composition checks remain. No modules or text shaping needed. |
| Sorting sort, formatter AST test | `crates/animatix-syntax/tests/fixtures/sorting_format.amx.txt` | Original bytes retained as parser input; formatter output reparses to the same scrubbed AST. Imports are syntax only, not resolved here. |
| Transformer attention, native presentation driver | `crates/animatix/examples/web_present_driver/fixtures/attention.amx` and `lib/` | Entire workload retained, including text and geometry. Only import prefixes change; all three module dependencies are frozen byte-for-byte. |

The attention manifest records original and relocated hashes separately. Reversing
only the import-prefix relocation restores the original source hash. Its import
closure is exactly theme, stage and components, which have no further imports.
The driver still uses the same sample times, presentation sizes, render quality,
paired arms and warmup counts. Its JSON now records this frozen input identity.
The basic-shapes input, renderer and font dependencies are not frozen by this
change: this is not a claim of pixel-identical performance across revisions.

`tools/quality/test-legacy-contract-fixtures.py` checks the frozen hashes, reversible
source transformation, module closure and marker timestamps in `ci-policy-tests`.
The existing Rust marker and formatter tests remain the semantic gates. Keeping
the full sort syntax is deliberate: reducing it before identifying its AST
coverage would silently weaken a real-world formatter regression.

Acceptance for this extraction requires the marker and formatter Rust tests and
a native presentation-driver run on a GPU-capable machine. The extraction's
source-identity checks alone are not runtime or pixel-equivalence evidence. Do
not delete the old corpus until those checks and its remaining consumers have
been addressed.
