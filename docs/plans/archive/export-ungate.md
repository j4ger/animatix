# Plan: Ungate single-frame raster (PNG/WebP) export from the `video` feature

## Goal
Make PNG and WebP single-frame export work in the default (no-FFmpeg) build of `animatix` and `animatix-gui`, while keeping Video/WebM/MOV/GIF behind the `video` feature.

## Verified facts (grounding)
- `crates/animatix/src/renderer/encode/image.rs` (`render_image`, `render_image_timeline`, `render_image_timeline_with_debug`, `render_image_timeline_with_progress`, `render_image_composition`, plus the two `*_async` helpers) depends only on: `image` crate, `pollster::block_on`, `OffscreenRenderer` (cfg `render`), `Composition`, `Timeline`, `SceneDimensions`, `DebugRenderOptions`, and `ExportError`. No rsmpeg / ffmpeg / gif.
- `image` and `pollster` are both pulled in by the `render` feature, which is part of `default = ["render", "text", "svg"]`. WebP works because the `image` dep enables `webp`; `img.save()` picks the encoder from the file extension, so WebP reuses the `render_image_*` path (the dialog already groups `Image | WebP`).
- `crates/animatix/src/renderer/encode/gif.rs` imports `crate::renderer::render_pipeline::{render_frames_streaming, render_frames_streaming_composition}`. `render_pipeline.rs` uses `rsmpeg::avutil::AVFrame` at module scope (`fill_rgba_frame`) and will not compile without rsmpeg. **GIF therefore must stay gated behind `video`** even though its encoder is the `image` crate.
- `ExportError`, `ExportSettings`, `MaxRenderThreads`, `VideoCodec`, `H264Preset`, `mux_audio_segments`, `is_ffmpeg_available`, `require_ffmpeg` live in `encode/mod.rs`. `mux_audio_segments` shells out via `std::process::Command` (no rsmpeg symbol). These are plain types/fns that compile under `render` alone.
- Current gating (the bug):
  - `renderer/mod.rs`: `pub mod encode; pub mod video; pub mod render_pipeline;` are all `#[cfg(feature = "video")]`. The big `pub use video::{ render_image*, render_gif*, render_video*, ... }` re-export is also `video`-gated.
  - `renderer/video.rs` is a `video`-gated shim re-exporting from `encode`.
  - GUI `export_target.rs`: `ExportTargetOwned` is `#[cfg(feature = "video")]`.
  - GUI `export_store.rs`: `export_thread`, `poll_export_status`, and the `animatix::renderer::video::ExportError` type path are `video`-gated.
  - GUI `export_dialog.rs`: `ExportStatus::Complete`, the entire worker-thread spawn, `cloned_target`, `has_composition`, `debug`, and `use std::sync::Arc` / `truncate_middle` are `video`-gated; the `#[cfg(not(feature = "video"))]` arm returns `Failed("Export requires the 'video' feature (FFmpeg)")` for **every** format including PNG.
  - CLI `main.rs`: `Commands::Image` and its handler, plus `use animatix::renderer;`, are `video`-gated (same bug on the CLI).
- 10 `unreachable!()` arms exist in `export_dialog.rs::start_export` (5 formats × {composition, timeline} match arms).

## Restructure decision
Gate the core `encode` module (and its `ExportError` + image functions + config types) behind **`render`** instead of `video`; keep the rsmpeg-dependent `video` mod, `render_pipeline` mod, and the `gif`/`video` encode submodules + their re-exports behind `video`. Rationale:
- Image export already requires `render` (it needs `OffscreenRenderer`), and `render` already provides `image` + `pollster`. No new feature is needed; introducing an `image-export` feature would add churn for no benefit since `render` is the natural boundary.
- `mux_audio_segments` / config enums are `pub` and won't trigger dead-code warnings in a render-only build.
- Make `video` imply `render` (`video = ["render", "dep:rsmpeg"]`) so the `video`-gated code that references `encode`/`OffscreenRenderer` always has them. This also removes a latent footgun where `--no-default-features --features video` fails today.

GIF stays `video`-gated (it transitively needs rsmpeg via `render_pipeline`). Confirmed FFmpeg-required set: **Video (mp4), WebM, MOV, GIF**. FFmpeg-free set: **Image (PNG), WebP**.

## Plan

1. **Core: make `video` imply `render` and keep `image` available without video.**
   - File: `crates/animatix/Cargo.toml`.
   - Change `video = ["dep:rsmpeg", "dep:image"]` to `video = ["render", "dep:rsmpeg"]` (drop the redundant `dep:image` since `render` already enables it; add `render`).
   - Expected: `cargo check -p animatix --features video` still compiles; `--no-default-features --features video` no longer misses `render`.
   - Verify: `cargo check -p animatix --features video` and `cargo check -p animatix` (default).

2. **Core: re-gate `encode` behind `render`, keep rsmpeg modules behind `video`.**
   - File: `crates/animatix/src/renderer/mod.rs`.
   - Change `#[cfg(feature = "video")] pub mod encode;` to `#[cfg(feature = "render")] pub mod encode;`.
   - Leave `pub mod video;` and `pub mod render_pipeline;` `video`-gated.
   - Replace the single `#[cfg(feature = "video")] pub use video::{...}` block with two blocks:
     - `#[cfg(feature = "render")] pub use encode::{ ExportError, ExportSettings, H264Preset, MaxRenderThreads, VideoCodec, render_image, render_image_composition, render_image_timeline, render_image_timeline_with_debug, render_image_timeline_with_progress };`
     - `#[cfg(feature = "video")] pub use encode::{ render_gif_composition, render_gif_composition_with_settings, render_gif_composition_with_progress, render_gif_timeline, render_gif_timeline_with_debug, render_gif_timeline_with_settings, render_gif_timeline_with_progress, render_video, render_video_composition, render_video_composition_with_settings, render_video_composition_with_progress, render_video_timeline, render_video_timeline_with_debug, render_video_timeline_with_settings, render_video_timeline_with_progress };`
   - Expected: `animatix::renderer::ExportError` and `animatix::renderer::render_image*` resolve in a default (no-video) build; video fns only under `video`.
   - Verify: `cargo check -p animatix` (default) + `cargo check -p animatix --features video`.

3. **Core: gate the rsmpeg-dependent encode submodules and re-exports inside `encode/mod.rs`.**
   - File: `crates/animatix/src/renderer/encode/mod.rs`.
   - Keep `pub mod image;` ungated. Gate `pub mod gif;` and `pub mod video;` with `#[cfg(feature = "video")]`.
   - Keep `pub use image::{...}` ungated. Gate the `pub use self::video::{...}` and `pub use gif::{...}` re-export blocks with `#[cfg(feature = "video")]`.
   - Leave `ExportError`, `ExportSettings`, `MaxRenderThreads`, `VideoCodec`, `H264Preset`, `mux_audio_segments`, `is_ffmpeg_available`, `require_ffmpeg`, `FFMPEG_AVAILABLE` ungated (they compile under `render`; all `pub`, no dead-code warnings).
   - Expected: render-only build compiles `image.rs` + types; `gif.rs`/`video.rs` only under `video`.
   - Verify: `cargo check -p animatix` + `cargo check -p animatix --features video`.
   - Risk: if any `render`-only warning fires for an unused private helper, address inline; none expected since all flagged items are `pub`.

4. **Core: add a descriptive internal error variant for target/format mismatch.**
   - File: `crates/animatix/src/renderer/encode/mod.rs`.
   - Add `ExportError::Internal(String)` variant + `Display` arm (e.g. `write!(f, "Internal export error: {msg}")`). Used by the GUI worker to replace `unreachable!()` (task step 4).
   - Expected: enum + Display match stay exhaustive.
   - Verify: `cargo check -p animatix`.

5. **CLI: ungate the `Image` command (same bug on CLI side).**
   - File: `crates/animatix/src/main.rs`.
   - Remove `#[cfg(feature = "video")]` from `Commands::Image { ... }` (enum, ~line 59) and its match handler (~line 486).
   - Ensure `use animatix::renderer;` and `use animatix::timeline::DebugRenderOptions;` are available in the default build. They are currently `#[cfg(feature = "video")]`. Split: import `renderer` + `DebugRenderOptions` unconditionally (they are needed by the now-ungated `Image` arm), keeping `format_diagnostic`/`warn` gating as-is if still video-only. Verify no unused-import warning in the default build — if `renderer`/`DebugRenderOptions` become unused when `video` is off, that won't happen because the `Image` arm uses both.
   - Expected: `animatix image input.amx` works without `video`.
   - Verify: `cargo check -p animatix` (default) + `cargo check -p animatix --features video`; optional smoke `cargo run -p animatix -- image examples/<x>.amx` (skip if no GPU in CI).
   - Risk: `renderer::ExportSettings`/`VideoCodec` used by the still-gated `Gif`/`Video` arms must remain reachable — they are, via step 2's `render`-gated re-export.

6. **GUI: ungate `ExportTargetOwned`.**
   - File: `crates/animatix-gui/src/app/document/export_target.rs`.
   - Remove the `#[cfg(feature = "video")]` above `pub enum ExportTargetOwned`. Keep `#[allow(clippy::large_enum_variant)]` (now justified because the image path also uses it; update the inline comment to mention image export).
   - Expected: enum available in default build; no dead-code (used by image worker).
   - Verify: `cargo check --workspace`.

7. **GUI: ungate export thread plumbing and switch to the top-level `ExportError` path.**
   - File: `crates/animatix-gui/src/app/stores/export_store.rs`.
   - Remove `#[cfg(feature = "video")]` from the `export_thread` field, its `new()` initializer, and `poll_export_status`'s body gate.
   - Change the type path `animatix::renderer::video::ExportError` → `animatix::renderer::ExportError` (now exported under `render`) in the `JoinHandle` type and the `Err(... ::Cancelled)` arm.
   - Expected: poll logic runs in default build; image exports report `Complete`/`Failed` like video.
   - Verify: `cargo check --workspace` + `cargo check -p animatix-gui --features video`.

8. **GUI: ungate `ExportStatus::Complete` and its action-bar arm.**
   - File: `crates/animatix-gui/src/app/shell/export_dialog.rs`.
   - Remove `#[cfg(feature = "video")]` from the `ExportStatus::Complete { path }` variant (enum) and from the matching arm in `render_export_action_bar`.
   - Remove the now-unconditional-need gates: change `use std::sync::Arc;` and `use ...text::truncate_middle;` from `#[cfg(feature = "video")]` to unconditional (both are now used by the always-present image path / Complete arm).
   - Expected: image export can surface success; no `unreachable arm`/unused-import warnings either way.
   - Verify: `cargo check --workspace` + `cargo check -p animatix-gui --features video`.

9. **GUI: restructure `start_export` so PNG/WebP run unconditionally and FFmpeg formats stay gated.**
   - File: `crates/animatix-gui/src/app/shell/export_dialog.rs`.
   - Ungate `cloned_target`, `has_composition`, and `debug` (remove their `#[cfg(feature = "video")]`); the image path needs `cloned_target`/`has_composition`, and `debug` (`animatix::timeline::DebugRenderOptions`) is an ungated type.
   - Replace the `#[cfg(feature = "video")] { ... spawn ... }` + `#[cfg(not(feature = "video"))] { Failed(...) }` pair with a single unconditional spawn whose inner `match state.format` is:
     - `ExportFormat::Image | ExportFormat::WebP => { /* ungated image path */ }` (calls `render_image_composition` / `render_image_timeline_with_progress`).
     - `#[cfg(feature = "video")] ExportFormat::Video => {...}`, same for `WebM`, `Mov`, `Gif`.
     - `#[cfg(not(feature = "video"))] ExportFormat::Video | ExportFormat::WebM | ExportFormat::Mov | ExportFormat::Gif => Err(animatix::renderer::ExportError::Internal("This format requires the 'video' feature (FFmpeg)".into()))` so the failure message only applies to FFmpeg formats.
   - Store the handle via `self.export_store.export_thread = Some(handle);` unconditionally.
   - Expected: PNG/WebP export on a default build; video formats give a precise FFmpeg message; video build unchanged.
   - Verify: `cargo check --workspace` + `cargo check -p animatix-gui --features video` + `cargo test -p animatix-gui`.

10. **GUI: replace the 10 `unreachable!()` arms with descriptive `ExportError::Internal` returns.**
    - File: `crates/animatix-gui/src/app/shell/export_dialog.rs`.
    - In each `match &cloned_target { ... _ => unreachable!() }`, return `Err(animatix::renderer::ExportError::Internal(format!("export worker: format {:?} expected a {} target", state.format, "composition"|"timeline")))` matching which branch (the `has_composition` true branch expects `Composition`; the else branch expects `Timeline`).
    - Note: each arm's value currently feeds the surrounding `render_*` call result; rewriting requires the arm to evaluate to `Result<(), ExportError>` — the composition arms already do (the `render_*` call returns `Result`), so the `_ =>` arm must also be a `Result`, hence `Err(...)` (not `unreachable!()`). Confirm the match is the tail expression of each format block so types line up.
    - Expected: a target/format mismatch produces a diagnosable `Failed("Internal export error: ...")` instead of a panic that crashes the worker thread.
    - Verify: `cargo check --workspace` + `cargo check -p animatix-gui --features video`.

11. **Docs: update the feature note.**
    - Files: `AGENTS.md` "Video Export" section and any `docs/*` mentioning that export needs `video`.
    - Clarify PNG/WebP export works without `video`; only Video/WebM/MOV/GIF need FFmpeg. Update the GUI export-dialog message wording note ("Export requires the 'video' feature" now only for FFmpeg formats).
    - Verify: prose only.

## Files to touch
- `crates/animatix/Cargo.toml` — `video` implies `render`.
- `crates/animatix/src/renderer/mod.rs` — gate `encode` on `render`; split re-exports into `render` vs `video`.
- `crates/animatix/src/renderer/encode/mod.rs` — gate `gif`/`video` submodules + their re-exports on `video`; keep `image`, `ExportError`, config types, mux helpers ungated; add `ExportError::Internal`.
- `crates/animatix/src/main.rs` — ungate `Commands::Image` + handler; adjust `renderer`/`DebugRenderOptions` imports.
- `crates/animatix-gui/src/app/document/export_target.rs` — ungate `ExportTargetOwned`.
- `crates/animatix-gui/src/app/stores/export_store.rs` — ungate `export_thread`/`poll_export_status`; use `renderer::ExportError`.
- `crates/animatix-gui/src/app/shell/export_dialog.rs` — ungate `Complete`/`Arc`/`truncate_middle`; restructure `start_export`; replace 10 `unreachable!()`.
- `AGENTS.md` / `docs/*` — feature-gating wording.

## Risks
- **Ordering**: Steps 1–4 (core) must land before GUI steps 6–10, which depend on `animatix::renderer::ExportError` and `render_image_*` being exported under `render`. Step 5 (CLI) depends on steps 1–3.
- **`video`-only re-export reachability**: `main.rs` `Gif`/`Video` arms and GUI video paths use `renderer::ExportSettings`/`VideoCodec`/`render_video*`/`render_gif*`. Step 2 must keep config types under `render` and video fns under `video` — verify with `cargo check -p animatix --features video` and `cargo check -p animatix-gui --features video`.
- **Dead-code / unused-import warnings in the default build**: ungating `Arc`/`truncate_middle`/`debug` could warn if a path is missed. The image worker uses all of them; confirm no `#[allow]` is needed, and if a warning appears, the fix is to use the item rather than re-add a `cfg`.
- **`unreachable!()` → `Err` type alignment**: each format block's `match &cloned_target` must be the block's tail expression yielding `Result<(), ExportError>`; the `Err(ExportError::Internal(...))` arm must match. Double-check `ExportError` is in scope (fully-qualify as `animatix::renderer::ExportError`).
- **`--no-default-features` builds**: gating `encode` on `render` means a `--no-default-features` (no render) build drops image export entirely. That is acceptable (no renderer = no export) and matches existing behavior; the GUI always enables `render` via `animatix` default features.
- **CLI import churn (step 5)**: moving `use animatix::renderer;` out of the `video` cfg could create an unused-import warning if the default build never references `renderer` elsewhere — the ungated `Image` arm references it, so it is used; confirm after the change.

## Final verification (run all)
```bash
cargo check --workspace                 # default, no video — must include GUI + image export
cargo check -p animatix-gui --features video
cargo test -p animatix-gui
cargo test -p animatix --lib            # core unaffected
```
