# Vendored `wasm-split-cli`

This directory vendors `wasm-split-cli` 0.8.0-alpha.3 from Dioxus commit
[`cb27832398e8a3fa30a0fc8d6917c6d8a86fa71a`](https://github.com/DioxusLabs/dioxus/tree/cb27832398e8a3fa30a0fc8d6917c6d8a86fa71a/packages/wasm-split/wasm-split-cli),
path `packages/wasm-split/wasm-split-cli`. It is dual licensed under MIT or
Apache-2.0; see `LICENSE-MIT` and `LICENSE-APACHE`.

Animatix uses it to split the wasm32 web artifact into the main module and
on-demand capability modules. The upstream release loses some original Wasm
function relocations when linker symbol names are aliases, and its data
pruning treats symbol ranges as exclusive even when the linker emits overlapping
aliases. Both can drop formatting descriptors required before any capability
loads.

Local changes:

- Resolve original function relocation symbols by their Wasm function index,
  including aliases whose names do not appear in the function name map.
- Bridge relocation edges through original functions that bindgen removed or
  renamed, preserving surviving descendants without making unrelated shims main
  roots; merge actual bindgen IR call edges into the bridged graph.
- Extend shared-chunk membership for each referencing split and include each
  chunk's transitive reachability when determining imports and chunk
  dependencies.
- Keep shared indirect-function table slots function-only, and propagate
  original-to-clone ID remapping failures as errors.
- Preserve bytes covered by any main-owned symbol when pruning cold symbols;
  exclude those bytes from side-module initializers.
- Connect overlapping data symbols within a segment so a byte range has one
  module owner, including shared capability chunks.
- Retain zero-sized data symbols for graph identity without emitting empty
  side-module initializers.
- Associate data relocations with every intersecting source symbol range and
  return parse errors instead of silently skipping malformed relocation entries.
- Grow the main live closure to a fixed point using Wasm function indices and
  relocation descendants; no static-address scan is used.
- Validate the supported table, memory, and active data-segment layout before
  splitting; fail closed if original Rust-mangled names are present but do not
  survive bindgen mapping. Focused tests cover these boundaries.

The vendor can be removed when the selected upstream splitter handles original
function-index relocations, overlapping data-symbol ownership, and the validated
ABI boundaries without these patches, and its output passes Animatix's web
capability and graphics checks.
