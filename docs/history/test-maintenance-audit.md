# Test maintenance audit

This audit records the test ownership and the first maintenance changes made
while preparing the workspace for dependency-scoped PR runs and parallel CPU
tests. It does not treat test count as a coverage target. No historical
regression test was removed; the motion modifier cases below were grouped while
preserving every input and diagnostic assertion.

## Ownership and cost inventory

The cost notes identify work each suite performs; they are not runtime
measurements. Test-stage timings and memory measurements belong in the runner
A/B evidence.

| Owner / crate | Test suites | Cost signal and audit treatment |
| --- | --- | --- |
| Engine: `animatix` | `src/timeline/tests/**`; inline tests under `timeline/actions`, `animation_track`, `behavior`, `build`, `eval_shared`, `morph`, `persistence`, `property_engine`, `scene_eval`, `shapes`, `svg_import`, `timeline/assets.rs`; `composition`, `primitives`, and `extension_native_plugin/tests/**` | Build/evaluation suites parse or build timelines repeatedly. Media, SVG import, asset, and plugin tests touch the filesystem and therefore need unique temporary directories before parallel execution. Keep each distinct runtime or storage invariant. |
| Engine public boundary: `animatix` | `tests/engine_contracts.rs` (modules `applicability_table_agrees_with_reads`, `captions_and_speech_sync`, `ir_opt_tests`, `ir_tests`, `module_tests`, `plot_params_tests`, `svg_import_tests`) | Integration coverage exercises public build/evaluation paths. These modules share the consolidated Cargo test target; retain distinct cases because they verify public build/evaluation boundaries. |
| Language: `animatix-syntax` | Inline tests under parser, token, formatter, module, semantic diagnostics, symbol table, schema, typecheck, and typing; integration targets `ast_format_coverage.rs`, `ease_roundtrip.rs`, `roundtrip_examples.rs`, and `scene_config_promotion.rs` (with `tests/common/mod.rs` as support) | Parser and round-trip cases are a regression catalogue. Audit only exact duplicate contracts; preserve grammar edge cases and independent schema sentinels. |
| GUI: `animatix-gui` | Inline tests throughout `src/app/**`, `src/source_edit/**`, editor, highlighting, and cell editor; there is no `tests/` directory | Most tests are state/AST-oriented. SourceEdit has repeated parse setup. Document and handler tests include real filesystem paths; do not modify unrelated dirty GUI files while auditing or changing helpers. |
| Renderer: `animatix-render` | Inline `filter_backend/tests.rs`, `offscreen/tests.rs`, `transition.rs`, and `encode/tests.rs`; integration targets `tests/offscreen_render.rs` and `tests/vello_img_probe.rs` (with `tests/common/mod.rs` as support) | Offscreen and filter cases request a GPU and inspect rendered output. The Nextest package group serializes all renderer tests to catch GPU tests regardless of name; this conservatively serializes its CPU-only cases too. |

The four rows above cover the original engine, syntax, GUI, and renderer
consolidation work. The wider source inventory below records the other package
owners and the isolation boundaries found during the maintenance pass.

| Additional owner | Test surfaces | Isolation and cost evidence |
| --- | --- | --- |
| Shared metadata: animatix-core, animatix-std | Core descriptor/docs contract in tests/properties_doc.rs; standard catalog/effect tests under src/catalog and src/effects | Static contract checks; no engine construction or GPU setup. Keep descriptor/reference coverage pinned to the public catalog. |
| Language intelligence: animatix-analyzer, animatix-lsp | Analyzer inline parser/completion/diagnostic/manifest suites and broken corpus; LSP path/manifest tests in src/main.rs | Manifest discovery uses temporary roots with process id plus timestamp. Corpus and examples are read-only repository inputs. |
| Text: animatix-text | Fast-layout Unicode, font-context, cache, extraction, layout, and external-font tests | Font metadata is memoized. The compiler cache is thread-local in this crate's cfg(test) build, but engine tests link the normal text dependency with its process-global compiler cache. Keep the engine library serial until a separate boundary has evidence. |
| CLI: `animatix-cli` | Inline review/command tests and `tests/audit_cli.rs` | The 56 tests in modules shared by the lib and bin Cargo targets are compiled in both; the bin target has 70 unit cases including 14 in `src/tests.rs`. The documented `cargo test -p animatix-cli --lib` surface runs the shared library tests without the binary harness; retain both entry surfaces unless case-level evidence shows redundancy. Audit integration tests launch the built CLI with `Command`; the shared project helper returns a `tempfile::TempDir` owner with the scene path. The baseline manifest test also owns a `TempDir`. |
| Web: animatix-web | Host/resource tests and tests/site_scenes.rs | Host tests exercise source/resource handling. The wasm playback implementation is not a native GPU test target; scene files are read as fixtures. |
| Widgets: eparts | Token/theme JSON, registry/watcher, and widget tests | Theme registry and watcher cases now use tempfile::TempDir; the watcher drops before its directory, which is removed on drop. |
| Plugin support: `animatix-plugin-api`, `animatix-plugin-tooling` | ABI contract and manifest-tooling tests | ABI cases are in-memory; tooling roots already include a timestamp. |
| Web build tooling: `animatix-web-build` | One Rust unit test in `src/split.rs` | The test pins recognized and unknown capability-name mapping for the splitter. The `web-split-proof` gate separately compiles and exercises the Wasm boundary fixture. |
| Text parity executable: `animatix-text-parity` | No Rust unit-test attributes; `src/main.rs` is the probe executable | The `text-parity` gate builds/runs it through `scripts/text-parity.mjs`, comparing native probe output with the generated Wasm SDK using checked-in scenes and fonts. This is gate-level executable coverage, not a package unit-test suite. |
| Split proof fixture: `animatix-split-probe` (`tools/animatix-web-build/fixtures/split-probe`) | No Rust test attributes; `src/lib.rs` is compiled for wasm32 | `web-split-proof` uses this as a deterministic compile/split boundary fixture; it is build-proof input, not a unit-test target. |
| No Rust unit-test attributes: `animatix-hybrid-bench`, `animatix-plugin-demo` | The inventory found no `#[test]` or `#[cfg(test)]` source in these two packages | Benchmark and demo surfaces are build/example coverage, not unit-test evidence. `animatix-text-parity` and `animatix-split-probe` also have no Rust test attributes; their executable and compile-proof roles are listed separately. |
| CI scripts | Twelve Python unittest modules under `scripts/` and `scripts/experiments/` | Filesystem tests use temporary directories; subprocess tests use mocks or explicit cwd/env arguments. `test-ci-control.py` changes CWD only inside a temporary directory and restores it in `finally`; `patch.dict` scopes environment overrides. |
| Node/web SDK validation | `web/tools/runtime.test.mjs`, `web/tools/perf-statistics.test.mjs`, `web/tools/third-party-licenses.test.mjs`; `scripts/rgba-comparison.test.mjs`, `scripts/cdp-client.test.mjs`, `scripts/web-sdk-tests.mjs` | Registered `web-runtime-tests`, `web-sdk-tests`, `web-sdk-integrity`, `text-parity`, `web-raster-proof`, `web-smoke`, and `web-transactions` gates exercise Node tooling and browser-facing SDK/build paths. These checks are inventoried separately from the Python unittest modules. |

## Isolation screening and boundaries

This is a source inventory, not a claim that every test was executed in every
parallel mode. The scan covered all 19 Cargo workspace packages (16 package
directories under `crates/`, including `eparts`, plus three packages under
`tools/`), active Python and Node test entrypoints, and the CI/contributor
guidance. Fifteen packages currently contain Rust test attributes; the four
without them are identified above. Twelve Python unittest modules were found:
`scripts/test-ci-control.py`, `scripts/test-ci-plan.py`,
`scripts/test-perf-policy.py`, `scripts/ci-environment-tests.py`,
`scripts/icon-tests.py`, `scripts/native-package-tests.py`,
`scripts/platform-icon-tests.py`, and
`scripts/experiments/test_compat.py`, `scripts/experiments/test_harness.py`,
`scripts/experiments/test_identity.py`, `scripts/experiments/test_runner.py`,
and `scripts/experiments/test_topology.py`. Node/web SDK tests and build-proof
gates are inventoried separately above and are not part of that Python count.

The Rust search for set_var, remove_var, and set_current_dir found no matches
under crates/. Python's only direct CWD mutation in the test files is the
scoped test-ci-control.py case described above. Rust tests read renderer
control variables but do not mutate the process environment. Python tests use
scoped environment patches or pass copied env maps to child processes. Rust
Command use in tests is confined to the CLI audit integration target; FFmpeg
subprocess calls are production encoder behavior, not hidden test setup.

The tests contain no process-wide GPU setup outside the renderer suite. CPU
rendering is an explicit mode in vello_img_probe.rs; CI requires the
GPU-backed path. Mutable frame/evaluation scratch caches found in the engine
are thread-local. Syntax OnceLock tables and text font metadata are shared
memoized data; text compilation cache isolation differs between animatix-text
unit tests and the normal dependency linked into engine tests, which is why
the serial engine fallback remains conservative.

Initial fixed or PID-only roots were replaced with RAII unique directories in
engine unit and integration tests, GUI perf-log/load-worker/file-handler tests,
CLI audit and baseline-manifest tests, the renderer storyboard test, and eparts
theme registry/watcher tests. The engine relative transcript-base fixture stays
under a unique child of target/ so its relative-path precondition remains
valid. Remaining fixtures are intentionally separate: analyzer/LSP/plugin
tooling roots include timestamps; the preload-only transcript test names a
nonexistent base without creating it.
GUI tests that use the real loaded-document path retain its TestTempDir guard
alongside the parsed stores, so the fixture remains available for the test and
is removed automatically at scope exit. SourceEdit fixtures are in-memory and
required no filesystem conversion.

## Changes and retained case mapping

| Original tests or setup | New shared form | Retained contract |
| --- | --- | --- |
| `source_edit/{actor_edits,keyframe_edits,scene_edits}/tests.rs::parse` | `source_edit::test_support::parse` | Each SourceEdit operation keeps its own assertions; shared parsing preserves the prior AST-present check. |
| Renderer `offscreen/tests.rs` and `tests/offscreen_render.rs` local `parse`, `build`, and `build_checked` helpers | `tests/common/mod.rs::{parse,build,build_checked}` | `build` continues returning output without judging build diagnostics; `build_checked` continues requiring a clean build. Pixel and end-to-end cases remain in their original test targets. |
| `move_requires_to_vec2_modifier` | Case `move/to` in `motion_actions_reject_invalid_modifier_values` | Invalid numeric `to` still requires `InvalidModifierValue`. |
| `rotate_requires_numeric_by_modifier` | Case `rotate/by` in `motion_actions_reject_invalid_modifier_values` | Invalid tuple `by` still requires `InvalidModifierValue`. |
| `scale_requires_positive_numeric_by_modifier` | Case `scale/by` in `motion_actions_reject_invalid_modifier_values` | Zero scale still requires `InvalidModifierValue`. |
| `shift_requires_by_vec2_modifier` | Case `shift/by` in `motion_actions_reject_invalid_modifier_values` | Invalid numeric `by` still requires `InvalidModifierValue`. |

The four motion cases remain independently labeled in assertion messages. Valid
motion tests are unchanged because they assert different tracks and outcomes.

## Temporary filesystem isolation

Engine unit tests now use `TestTempDir`, which creates a process/time/counter
unique directory and removes it on drop. This covers media fixtures, asset-cache
fixtures, SVG-import files, and the native-plugin cached-image fixture. Unique
directories prevent same-process tests from colliding; RAII cleanup prevents
leftover files from affecting later runs. Existing committed SVG fixtures are
read-only inputs and remain unchanged. The same helper now owns composition and
cross-file module tests, the geometry import case, and transcript-cache file
fixtures; its new_in("target", ...) form preserves the relative-path test's
precondition while still cleaning only its unique child directory.

Other converted roots have explicit owners: GUI perf-log/load-worker/file-
handler tests use the GUI test-support TestTempDir or tempfile::TempDir; CLI
audit projects and the baseline manifest, renderer storyboard, and eparts
registry/watcher tests use tempfile::TempDir. Each fixture is removed on drop.

## Deletion and audit rule

No test was deleted in this change. Future removal requires a direct mapping to
an equivalent retained case with the same preconditions, observable behavior,
and boundary. Similar source snippets or tests at separate crate boundaries do
not count as duplication by themselves. Helper consolidation and test movement
reduce maintenance cost but do not claim CI speedups; runner changes require
separate paired timing and memory evidence.

Engine tests that depend on `animatix-text` as a normal dependency still see its
production process-global text compile cache; the text crate's own `cfg(test)`
thread-local cache does not apply to those dependents. The engine library remains
serial in the standard Cargo fallback until runner measurements and isolation
work establish a safe parallel boundary.
