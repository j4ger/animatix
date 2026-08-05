//! Streaming parallel frame rendering helpers.
//!
//! Provides the shared machinery used by all export encoders:
//! - [`fill_rgba_frame`] — borrow an RGBA buffer into an `AVFrame`
//! - [`render_frames_streaming`] — parallel timeline render with sequential output
//! - [`render_frames_streaming_composition`] — composition-aware variant

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use tracing::info;

use crate::composition::Composition;
use crate::renderer::encode::ExportError;
use crate::renderer::offscreen::{OffscreenRenderer, RenderedFrame};
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

/// Fill an `AVFrame` with a borrowed RGBA buffer.
///
/// # Safety
///
/// `rgba` must remain alive and unchanged for the lifetime of `frame`.
/// `fill_arrays` does not take ownership; it only stores the pointer.
pub fn fill_rgba_frame(
    frame: &mut rsmpeg::avutil::AVFrame,
    rgba: &[u8],
    width: i32,
    height: i32,
) -> Result<(), rsmpeg::error::RsmpegError> {
    let data_ptr = rgba.as_ptr() as *mut u8;
    // SAFETY: `data_ptr` points to `rgba`, which outlives this call.
    // `fill_arrays` only stores the pointer; the caller must keep `rgba` alive.
    // The RGBA buffer is valid for the lifetime of `frame` as ensured by the caller.
    unsafe { frame.fill_arrays(data_ptr, rsmpeg::ffi::AV_PIX_FMT_RGBA, width, height) }
}

// ---------------------------------------------------------------------------
// Streaming parallel frame rendering helper
// ---------------------------------------------------------------------------

/// Renders frames in parallel using up to `num_threads` workers, but feeds them
/// to `process_frame` in strict sequential order.  A small bounded channel per
/// chunk limits in-flight frames so memory stays O(num_threads × frame_size)
/// rather than O(total_frames × frame_size).
pub fn render_frames_streaming<F>(
    timeline: &Timeline,
    width: u32,
    height: u32,
    fps: u32,
    total_frames: u32,
    num_threads: usize,
    debug_options: DebugRenderOptions,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
    mut process_frame: F,
) -> Result<(), ExportError>
where
    F: FnMut(usize, RenderedFrame) -> Result<(), ExportError>,
{
    if total_frames == 0 {
        return Ok(());
    }

    let num_threads = num_threads.max(1);
    let chunk_size = (total_frames as usize).div_ceil(num_threads).max(1);
    let num_chunks = (total_frames as usize).div_ceil(chunk_size);

    info!(
        "Rendering {} frames using {} thread(s) ({} frame(s) per chunk)...",
        total_frames, num_chunks, chunk_size
    );

    // Create one renderer per chunk on the main thread.
    // Concurrent wgpu adapter enumeration from multiple threads can be flaky
    // on some drivers, so we stagger creation here.
    let mut renderers = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        renderers.push(OffscreenRenderer::new().map_err(ExportError::RendererCreation)?);
    }

    // Bounded channels limit memory to a few frames per chunk.
    const CHANNEL_CAPACITY: usize = 2;
    let mut senders = Vec::with_capacity(num_chunks);
    let mut receivers = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        let (tx, rx) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        senders.push(tx);
        receivers.push(rx);
    }

    let mut handles = Vec::with_capacity(num_chunks);
    for (chunk_idx, (renderer, sender)) in
        renderers.into_iter().zip(senders.into_iter()).enumerate()
    {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        let timeline = timeline.clone();

        handles.push(std::thread::spawn(move || -> Result<(), ExportError> {
            let mut renderer = renderer;
            for frame in start..end {
                let time = (frame as f64) / (fps as f64);
                let rendered = renderer
                    .render_timeline_with_debug(
                        &timeline,
                        time,
                        SceneDimensions { width, height },
                        debug_options,
                    )
                    .map_err(|e| ExportError::FrameRender { frame, message: e })?;
                sender.send(rendered).map_err(|_| ExportError::ThreadPanicked)?;
            }
            Ok(())
        }));
    }

    // Consume chunks in strict order so the encoder receives frames sequentially.
    for (chunk_idx, receiver) in receivers.into_iter().enumerate() {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        for frame in start..end {
            if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err(ExportError::Cancelled);
            }
            let rendered = receiver.recv().map_err(|_| ExportError::ThreadPanicked)?;
            process_frame(frame, rendered)?;
            if let Some(p) = progress {
                p.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    // Threads should have finished because we drained every receiver, but join
    // to be safe and to propagate any panics.
    for handle in handles {
        handle.join().map_err(|_| ExportError::ThreadPanicked)??;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-Scene Composition Streaming Renderer
// ---------------------------------------------------------------------------

/// Like `render_frames_streaming` but evaluates a `Composition` instead of a
/// single `Timeline`. At each global time step, resolves the active scene and
/// its local time, then renders that scene's timeline.
///
/// In Phase 1 (hard cuts only), a single scene is rendered per frame. Transition
/// blending (two-scene composite) will be added in Phase 7.
pub fn render_frames_streaming_composition<F>(
    composition: &Composition,
    width: u32,
    height: u32,
    fps: u32,
    total_frames: u32,
    num_threads: usize,
    debug_options: DebugRenderOptions,
    progress: Option<&AtomicU32>,
    cancel: Option<&AtomicBool>,
    mut process_frame: F,
) -> Result<(), ExportError>
where
    F: FnMut(usize, RenderedFrame) -> Result<(), ExportError>,
{
    if total_frames == 0 {
        return Ok(());
    }
    if !composition.has_scenes() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "Composition has no scenes to render".into(),
        });
    }

    let num_threads = num_threads.max(1);
    let chunk_size = (total_frames as usize).div_ceil(num_threads).max(1);
    let num_chunks = (total_frames as usize).div_ceil(chunk_size);

    info!(
        "Rendering {} scene(s) over {} frames using {} thread(s) ({} frame(s) per chunk)...",
        composition.scenes.len(),
        total_frames,
        num_chunks,
        chunk_size
    );

    // Create one renderer per chunk on the main thread.
    let mut renderers = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        renderers.push(OffscreenRenderer::new().map_err(ExportError::RendererCreation)?);
    }

    const CHANNEL_CAPACITY: usize = 2;
    let mut senders = Vec::with_capacity(num_chunks);
    let mut receivers = Vec::with_capacity(num_chunks);
    for _ in 0..num_chunks {
        let (tx, rx) = std::sync::mpsc::sync_channel(CHANNEL_CAPACITY);
        senders.push(tx);
        receivers.push(rx);
    }

    let mut handles = Vec::with_capacity(num_chunks);
    for (chunk_idx, (renderer, sender)) in
        renderers.into_iter().zip(senders.into_iter()).enumerate()
    {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        let composition = composition.clone();

        handles.push(std::thread::spawn(move || -> Result<(), ExportError> {
            let mut renderer = renderer;
            let dims = SceneDimensions { width, height };
            for frame in start..end {
                let global_time = (frame as f64) / (fps as f64);
                let (scene_name, local_time_s, transition_blend) =
                    composition.evaluate(global_time);

                let rendered = if let Some(blend) = transition_blend {
                    // Phase 7: transition blending — composite two scenes
                    let from_scene =
                        composition.scenes.get(&blend.from_scene).ok_or_else(|| {
                            ExportError::FrameRender {
                                frame,
                                message: format!(
                                    "From scene '{}' not found in composition",
                                    blend.from_scene
                                ),
                            }
                        })?;
                    let to_scene = composition.scenes.get(&blend.to_scene).ok_or_else(|| {
                        ExportError::FrameRender {
                            frame,
                            message: format!(
                                "To scene '{}' not found in composition",
                                blend.to_scene
                            ),
                        }
                    })?;
                    renderer
                        .render_transition(
                            &from_scene.timeline,
                            blend.from_local,
                            &to_scene.timeline,
                            blend.to_local,
                            blend.progress as f32,
                            blend.id.clone(),
                            blend.easing,
                            dims,
                            debug_options,
                        )
                        .map_err(|e| ExportError::FrameRender { frame, message: e })?
                } else {
                    // Single active scene — no transition
                    let scene_timeline = composition.scenes.get(&scene_name).ok_or_else(|| {
                        ExportError::FrameRender {
                            frame,
                            message: format!("Scene '{}' not found in composition", scene_name),
                        }
                    })?;

                    renderer
                        .render_timeline_with_debug(
                            &scene_timeline.timeline,
                            local_time_s,
                            dims,
                            debug_options,
                        )
                        .map_err(|e| ExportError::FrameRender { frame, message: e })?
                };

                sender.send(rendered).map_err(|_| ExportError::ThreadPanicked)?;
            }
            Ok(())
        }));
    }

    // Consume chunks in strict sequential order.
    for (chunk_idx, receiver) in receivers.into_iter().enumerate() {
        let start = chunk_idx * chunk_size;
        let end = ((chunk_idx + 1) * chunk_size).min(total_frames as usize);
        for frame in start..end {
            if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return Err(ExportError::Cancelled);
            }
            let rendered = receiver.recv().map_err(|_| ExportError::ThreadPanicked)?;
            process_frame(frame, rendered)?;
            if let Some(p) = progress {
                p.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    for handle in handles {
        handle.join().map_err(|_| ExportError::ThreadPanicked)??;
    }

    Ok(())
}
