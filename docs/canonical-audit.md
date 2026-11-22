# Canonical scene audit

`web/content/scenes/` is the authored site corpus. It currently has 16 scenes.
Existing site tests check parsing/building; they are not a multi-timepoint audit
or pixel acceptance. The 105 example/legacy scenes remain covered unchanged.

## Inventory

`tools/quality/audit_corpus.py` owns source discovery and exclusions for the
scene gate, budget gate, and corpus statistics. Its default `legacy` selection
uses the published inventory and discovers every AMX
under the canonical scene directory without inheriting legacy exclusions.
`--json` includes ownership and exclusion reasons. Missing roots and empty
corpora, including all-excluded inventories, fail rather than yielding an empty
success. Newline-containing paths are rejected because shell consumers use
newline-delimited lists. Canonical scenes are not
automatically added to the legacy budget-update command.

## Local diagnostics

From the repository root, inside the default Nix shell:

```sh
cargo build --bin animatix
python3 tools/quality/audit-canonical.py --output target/canonical-audit/first
```

The output directory must be new. The collector retains stdout JSON, stderr,
exit status, source hashes, exact commands, binary hash, and checkout commit.
It uses an explicit empty default-policy budget, never the existing debt file,
and never calls `--update-budget`. Passing requires zero errors and warnings,
an empty exceeded-budget list, and no budget update. Source and executable
hashes must remain stable during collection; an executable hash does not prove
which source revision built it. Timeout output is retained as partial evidence. Exit 1 means findings or failed audits need
review; it does **not** mean a baseline should be regenerated. Exit 2 is setup
failure. `--bin` permits an explicitly selected CLI build.

The blocking gate has a zero-debt contract. Triage failures into actual scene
defects, intended compositions, and analyzer false positives. Do not silently budget
new warnings. Any exception needs the actor/rule, rationale, evidence, and a
removal condition. Keep the existing four-file example budget unchanged.

`--render never` evaluates structural/timepoint facts, not GPU pixels. Follow
up with native `animatix storyboard <scene>` and `animatix audit <scene>
--evidence <directory>` on a working GPU; inspect entrances, label layout,
clipping, transitions, and the held final frame. Site-policy/resting composition
checks and browser playback remain separate. Validate canonical sources against
site metadata before retiring any unreferenced scene; this inventory intentionally
includes unreferenced authored files rather than silently dropping them.

## CI coverage and evidence

`canonical-audit` runs in the existing Check & Lint job using the default Nix
shell. It builds the CLI from the checkout and discovers the canonical corpus
at runtime; a new AMX is covered without editing a list. It is separate from
legacy `scene-audit budget-audit`, so canonical scenes run once, not in both
corpora. The Cargo build can reuse the preceding legacy audit's artifacts.

Changes to canonical scenes or fonts select Check & Lint; Rust package changes
already select it. Audit tooling and unknown paths conservatively select the
full matrix. Full/reusable CI and release validation include the registered
gate. This job-level policy also runs the remaining Check & Lint checks for
scene-only changes; splitting their scheduling is deferred rather than adding
another runner and independent CLI build now.

Each invocation has a fresh `target/ci/canonical-audit/run-*/` directory. Raw
JSON/stderr, local paths and commands stay under `raw/` and are **not uploaded**.
Only `summary.json` is an artifact: typed completion counters, completion state,
executable hash and stability flag. The summary is written before auditing and
after each scene, so interruption cannot appear as a completed pass. A complete
summary is not itself a pass: all expected scenes must pass and the binary must
be unchanged. The CLI exit code remains authoritative for gate failure.

No old scenes or budgets are removed. Legacy scenes referenced by unresolved
roadmap investigations must remain until reproduced or preserved as focused
fixtures/history. Structural audit success does not claim those bugs are fixed.

Tool contract checks (fake CLI results, not renderer evidence):

```sh
python3 tools/quality/test-audit-corpus.py
python3 tools/ci/test-joint-audit.py
```
