# Test and comment pruning (2026-11-15)

Baseline: `77b0d8a5`. This follow-up removes repeated execution, fixture
boilerplate and stale explanations. It preserves regression inputs and outcomes;
source/test counts alone are not coverage evidence.

## Retained coverage

| Previous test/setup | Retained test/case | Preserved behavior and assertions |
| --- | --- | --- |
| CLI lib's 56 shared tests also run in the bin harness | Default bin harness: all 70 unit cases; all 9 CLI integration cases remain | Identical shared module sources and 56 matching case names. `[lib] test = false` disables only duplicate automatic execution; explicit `cargo test -p animatix-cli --lib` still lists all 56 cases. Bin-only argument parsing and CLI process/exit/output tests remain. |
| `test_is_currently_animating_works_with_linear_easing` | `test_is_currently_animating_true_between_two_keyframes`, labeled `linear_easing` case | Both original Linear tracks remain: 0–1000ms sampled at 500ms and 0–2000ms sampled at 1000ms, expected true. Before/after/exact-boundary cases remain separate. |
| `test_registry_driven_max_keyframe_time_finds_min_width` | Renamed `...finds_min_height` | Unchanged min-height input and 4000ms expectation; the separate min-width regression remains at 7777ms. Letter/word-spacing component regressions remain. |
| Duplicate rich-type `Probe` component declarations | `make_rich_probe_env` in syntax typecheck tests | Accept and reject tests retain their Vec3/tuple/function inputs and diagnostic assertions; expected values are independent of the fixture. |
| `shortcut_for_unknown_returns_none` | `shortcut_for_payload_variant_matches_discriminant` | Original Scene(0)→Num1 and Nudge→ArrowLeft assertions remain; Scene(99)→Num1 now exercises the payload-independent lookup described by the old contradictory comment. |
| `toggle_playback_returns_repaint_effect` | `toggle_playback_toggles_playing_flag` | Same fresh 5s preview; exactly one Repaint from the first toggle, initial false→true→false transitions. End-of-playback reset remains separate. |
| `toggle_editor_sync_turns_{off_when_on,on_when_off}` | `toggle_editor_sync_updates_flag_and_status` | Fresh UI per row; both initial/result flags, one effect and exact ON/OFF status text retained. |
| `scrub_to_clamps_{negative,overshoot}_time` | `scrub_to_clamps_out_of_range_times` | Fresh stores per row; -5→0 and 999→5 seconds. Interior scrubbing's stop/dirty effects remain separate. |
| `is_some()` immediately before `unwrap()` in three track-field tests | Existing unwrap/variant/value assertions | `None` still fails; TextPaths variant and interpolation outputs remain checked. |

The baseline runtime inventory has 2,526 IDs. The intended default inventory has
2,465: 56 repeated CLI executions disappear and five function pairs are merged.
The exact accounting also checks the min-height rename and new matrix names;
all other runtime IDs must match. No tests are newly ignored. Pixel, public API,
serialization/property-ID, cache, IME and cross-layer contract tests stay intact.

## Comment cleanup

Removed duplicate property-registry field/workflow lists and doc comments that
had drifted onto `use`/`mod` declarations; public type/function docs remain.
Corrected claims that shift was unregistered, SVG currentColor/caps/joins were
unsupported, SourceEdit was an exposed inspector surface, and luminance helpers
composited transparent colors onto white. Arc/clip/inherit limitations, patch
rationales and hidden-by-default/resource/timing constraints remain documented.
Removed assertion narration from GUI tests and migration/roadmap scaffolding
from shared fixtures/theme tokens. Application Rust changes are comment-only;
non-comment source lines were compared with the baseline.

Using `scripts/maintenance-inventory.py` for `crates/` and `tools/`, workspace
Rust lines fall **252,357→252,137 (−220)**, standalone `//` lines
**26,028→25,884 (−144)**, and test-path lines **55,562→55,435 (−127)**.
Those categories overlap and exclude vendored Rust. The short archive entry is
additional documentation; no new experiment framework or helper crate was added.

All registered fmt, structure, workflows, CI policy, native-package, doc-links,
workspace check/lint/test and strict Rustdoc/property-reference gates passed.
The final 32 harnesses executed exactly the expected 2,465 IDs, with no unexplained
missing/added IDs or newly ignored cases. Doctest identity/status sets match the
baseline (2 passed, 27 existing ignored; source-line offsets excluded). These
changes do not claim a measured wall-time speedup.

## Second pass (baseline `ac623451`)

Three Luna xhigh agents split engine, language intelligence and GUI/widgets,
then reviewed each other's changes. The follow-up focuses on repeated fixtures:
41 `Property` literals with both metadata fields `None` now use the existing
`Property::new(name, value)`. Names/expressions are unchanged; explicit span or
comment fixtures, public constructors and independent expected values remain.

| Previous tests/setup | Retained test/case | Preserved checks |
| --- | --- | --- |
| world `meta_default_tests::directly_inserted_tracks_get_default_meta_on_demand` | Existing `entity_meta_defaults_and_resets_on_replacement` | Fresh `(box,100,changed flags)` and `(direct,250,default flags)` rows retain default visibility, mutation readback, initially-unseen state, first-write wins over 900, and replacement resets. The duplicate module/file is removed. |
| `action_signatures_include_{reveal_out,draw_out,reveal_in}` | `action_signatures_include_reveal_actions` | Same three literal names and membership assertions; per-name failure messages. |
| Four empty-source keyword/type/snippet/action completion tests | `top_level_completions_include_all_builtin_kinds` | One identical query; every original category/label assertion retained. Partial/known-verb and extension boundaries remain separate. |
| Two lint parsing and two suppression tests | `lint_config_from_source_parses_disable_directives`, `lint_config_suppresses_specific_and_global_warnings` | Both exact directive strings, specific-code/global flag checks and warning/code assertions retained over the same unused-title fixture. Error-suppression coverage remains independent. |
| Three LSP `uri_to_path_*` tests | `uri_to_path_handles_file_and_rejected_inputs` | Exact file URI/path, HTTP URI/None and empty string/None cases. |
| GUI missing/empty timeline tests | `timeline_keyframe_times_handles_missing_and_empty_timeline` | `(None,None,None) => []` and an empty built timeline's default-background `[0.0]`; sorted/deduped timeline test stays separate. |
| Select highlight start/move and filtered/stale pairs | `step_highlight_starts_and_moves_with_clamping`, `step_highlight_skips_filtered_rows_and_resets_stale_current` | All start, direction, boundary clamp, filtered `[1,3]` and stale `[2,5]` assertions retained. Empty-list and builder-field cases stay independent. |
| Repeated CI grouped-job dictionaries | Local `grouped_jobs` fixture | All 13 Python test methods remain. Capturing all 30 old/new `analyze_jobs` calls confirmed identical arguments, including successful Plan and failed candidate/summary outcomes. |

The 41 constructor conversions are engine 18, syntax 16, analyzer 2 and GUI 5;
these conversions do not change test IDs. Application logic is unchanged;
production-file edits only remove a `cfg(test)` module declaration or change
an inline test module. No new helper crate, test DSL or benchmark was added.

Workspace Rust shrinks **252,137→251,912 (−225)**, standalone `//` lines
**25,884→25,881 (−3)** and test-path source **55,435→55,216 (−219)**.
The Python fixture extraction saves another 14 lines. Categories overlap;
these Rust counts use the same `crates/` + `tools/` scope as the first pass.
The verified default runtime inventory is **2,465→2,452** across 32 harnesses:
21 old IDs map to 8 new aggregate IDs plus the existing world lifecycle test.
The actual ID set matches exactly, with no newly ignored cases. Doctest identities
and statuses also match (2 passed, 27 existing ignored; line offsets excluded).
All ten registered gates listed above passed again. Local Cargo builds used two
workers without changing product/CI settings; timings are not speedup evidence.

## Third pass: initialization cost (baseline `c1c2f225`)

Three Luna xhigh agents audited engine/text, GPU rendering and consumers, then
cross-reviewed the changes. All test IDs and independent pixel checks stay.

| Test | Removed work | Retained contract |
| --- | --- | --- |
| `missing_glyph_warning_fires_for_unassignable_codepoints` | Scanning every host font for an unassignable scalar; supply the existing Open Sans fixture through `BuildOptions` instead. | Same source `a\u{10ffff}b`, implicit default family and cold missing-character lookup. Check `ab` is covered, then require the build's `MissingGlyph` diagnostic to name `U+10FFFF`. System/registered/bundled fallback and cache tests remain separate. |
| `build_quality_changes_plot_sampling_output` | Four scene builds become two; the render and geometry checks use the same Draft/Production timelines. | Same source, qualities, sample times, path-element ratio and nonblank-pixel assertions. |
| `storyboard_renders_multiframe_grid_to_disk` | Remove the preflight renderer; the storyboard API creates its own renderer. | Same three samples and grid/file assertions. Only renderer-creation errors follow the local no-GPU policy; required-GPU CI still fails. Other errors fail. |

Local measurements use Linux 7.2.10, Ryzen AI 9 HX 370 (24 logical CPUs),
Rust/Cargo 1.99.1 and the pinned default Nix shell. Compare saved baseline and
candidate workspace test binaries, with identical features and test profiles
(opt-level 0, debug 2). Each arm runs engine lib (1,088 cases) then offscreen
integration (19 cases) serially, using CI package environment/working directory,
`--test-threads=1` and `ANIMATIX_REQUIRE_GPU=1`. No source edits, builds or other
performance experiments run during these five AB/BA pairs. OS/shader caches are
warm; each arm starts new processes. These are execution measurements, not cold
builds or hosted CI end-to-end timings.

| Pair/order | Engine A (s) | Engine B (s) | Offscreen A (s) | Offscreen B (s) |
| --- | ---: | ---: | ---: | ---: |
| 1 (AB) | 26.46 | 7.69 | 16.37 | 16.35 |
| 2 (BA) | 32.14 | 7.87 | 16.45 | 16.02 |
| 3 (AB) | 33.47 | 7.54 | 16.84 | 16.53 |
| 4 (BA) | 32.26 | 8.08 | 16.87 | 16.16 |
| 5 (AB) | 23.84 | 7.70 | 16.71 | 16.49 |

Engine median is **32.14→7.70 s (−76.0%, 24.44 s saved)**; the largest
per-process peak RSS across five runs is **709.4→219.2 MiB** (Linux `wait4`,
not aggregate process-tree memory). Offscreen median is **16.71→16.35 s**;
that small 2.2% difference does not establish a material speedup. Retain its
confirmed setup deduplication without claiming otherwise. Combined median for
these two sequential harnesses is **48.59→24.07 s**; hosted CI gains depend on
its font inventory and must be measured there.

Reproduce by building baseline and candidate separately inside `nix develop`
with `CARGO_BUILD_JOBS=2 cargo test --workspace --no-run --locked
--message-format=json`, saving the two executables, then running the five orders
above with the recorded CI harness environment. Build workers affect preparation
only. Raw identities, hashes, matched features/profiles, rows, outputs and RSS
are in local `target/maintenance-evidence/pruning3-compare/`.

Keep CLI subprocess/exit-code tests, site import retries, GPU lifecycle/pixel
checks and text cache tests independent. Existing font contexts already share
the system DB; more wrapper sharing would not remove another scan. A shared
syntax corpus AST cache has no measured material benefit to justify added state.
No production or frame/export hot-path code, feature union or runner policy changes.

Final validation: all ten applicable registered gates above passed. The complete
32-harness runtime set matches the baseline exactly: 2,452 passed IDs, no new
ignores. Doctest identities/statuses also match (2 passed, 27 existing ignored;
line offsets excluded). Invalid-ICD checks confirm local no-GPU skip and
required-GPU failure. Independent review confirmed the measurements and mapping.
