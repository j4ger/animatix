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
