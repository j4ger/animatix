# Vello GPU migration baseline and crate inventory — 2026-11-19

This is the reproducibility anchor for the Vello GPU migration. It records the
committed starting trees and the process for running equivalent native CI
gates without checking out either revision over the active integration branch.
It is evidence about source identity and test procedure, not a passing native
test verdict.

## Source identities

| Role | Commit | Tree | Parent |
|---|---|---|---|
| Migration baseline | `204b668a2f1ef4107957f421fa513ac05e3a4846` | `32af5b7a753ee5b0ece7d948723a0c1c82899834` | `54352c5605f3ba996c6d56571b57131fe11f5cfc` |
| Original opt-in trial | `e6f1f1a31ce6ade5de973ff78e939128d671b6d9` | `0d68421eca8236bc558f1a539dd0855ef2d4eb88` | `54352c5605f3ba996c6d56571b57131fe11f5cfc` |

The trial and migration baseline are sibling commits on the same parent. The
trial is retained as historical comparison input; the migration candidate
should be compared to `204b668a`, not silently substituted with the older
trial tree.

## Reproducing native gates

`tools/ci/compare-snapshot-gates.py` exports each commit with `git archive` to a
temporary source directory and runs the same registered native PR gates in
both directories. It does not create or switch a Git worktree, touch the
current index, or replace the active checkout. Each side receives its own
temporary Cargo target directory. The report and logs are saved under
`target/ci/migration-compare/` and are intentionally untracked.
The report records source commit/tree IDs, CI gate definitions, host/tool
versions, per-gate timings and exit codes. It hashes `Cargo.lock` around every
gate and treats a gate that rewrites the lockfile as a failed run.

Run it inside the shell required by the selected gates. This full native gate
set includes formatting, check/clippy, the workspace test suite, example and
render checks, and the no-video feature check:

```sh
nix develop -c python3 tools/ci/compare-snapshot-gates.py \
  --baseline 204b668a --candidate <candidate-commit> \
  --gate fmt --gate lint --gate check --gate test --gate no-video \
  --gate render-smoke --gate examples
```

Without explicit `--gate` values the script uses `check`, `test`, `no-video`,
`render-smoke`, and `examples`. It reads the canonical gate registry in each
snapshot and rejects gates that are not native Nix PR gates in both source
trees. `--prepare-only` verifies commit resolution, archive extraction, and
gate availability without requiring Cargo.

The five Python tests passed. The preparation check extracted both source
identities and found the named `test` gate. Cargo formatting passed on an
archive of the baseline. The cloud Nix runner timed out downloading from
`cache.nixos.org`, so the full native comparison still needs its required
Nix and GPU environment.

## `animatix-std` → `animatix-core` inventory

The current merge surface is small and acyclic at the manifest level: core has
no engine/parser/GPU dependencies, and std depends only on core plus `tracing`.
Keep those properties when moving the modules.

| Current source | Destination module in core | Notes |
|---|---|---|
| `crates/animatix-std/src/callable.rs` | `callable` or a `stdlib` child | Built-in callable catalog and tests; core already owns callable value and argument types. |
| `crates/animatix-std/src/catalog/` | `catalog` | Primitive identity cards, built-in catalog, capability derivation, lookups, and tests (`mod.rs`, `caps_impl.rs`, `info_impl.rs`, `lookup.rs`, and `tests.rs`). |
| `crates/animatix-std/src/effects/` | `effects` | Built-in effect definitions, shared effect WGSL context, and tests; depends on core's effect contract. |
| `crates/animatix-std/src/lib.rs` | core exports / dispatch | `EFFECTS`, effect lookup/handles, plugin effect registry and uniform packer. Adds a `tracing` dependency to core; no GPU dependency is needed. |

Direct manifest consumers to redirect are `animatix`, `animatix-syntax`,
`animatix-analyzer`, and `animatix-web`. Update public re-exports, test imports,
source comments and generated/documentation references along with them. Core's
optional `serde` feature and the catalog/effect/callable tests must continue to
work after relocation. Removing the `animatix-std` package itself is a Cargo
dependency compatibility break; keep a compatibility facade only if external
Rust-crate consumers are supported, otherwise document the new core import
paths in release notes.

This inventory is preparatory. No crate manifests or Rust imports were changed
for the merge in this baseline pass.

## Archive boundary

Completed migration decisions and measurements belong in this dated history
record or a linked focused study. Only unresolved implementation work belongs
in the active [roadmap](../roadmap.md). Preserve the original trial commit as
the historical source; do not rewrite its identity to represent the new
candidate.
