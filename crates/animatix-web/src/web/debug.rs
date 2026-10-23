//! Diagnostic probes for the player: SVG state and flat canvas fills.

use animatix::composition::BuildTarget;
use animatix::timeline::DebugRenderOptions;
use wasm_bindgen::prelude::*;
use wgpu::CurrentSurfaceTexture;

use super::{AmxPlayer, EngineContext, with_engine};

/// One asset-cache entry of [`AmxPlayer::debug_svg_stats`].
#[derive(serde::Serialize)]
struct SvgCacheEntry {
    url: String,
    paths: usize,
}

/// One track row of [`AmxPlayer::debug_svg_stats`].
#[derive(serde::Serialize)]
struct SvgTrackEntry {
    label: String,
    paths: usize,
    /// Paths the track evaluates to at the probe time.
    paths_at_t: Option<usize>,
    /// The track's authored opacity at the probe time.
    opacity_at_t: f32,
}

/// The payload of [`AmxPlayer::debug_svg_stats`].
#[derive(serde::Serialize)]
struct SvgStats {
    cache: Vec<SvgCacheEntry>,
    tracks: Vec<SvgTrackEntry>,
    vello: VelloStats,
}

/// Draw/path counts of the probed frame's vello encoding.
#[derive(serde::Serialize)]
struct VelloStats {
    draws: usize,
    paths: u32,
}

#[wasm_bindgen]
impl AmxPlayer {
    /// Diagnostic: report the loaded document's SVG state — the asset cache's
    /// parsed path counts per url, and each track's path count both statically
    /// and as evaluated at `time_s`, with the actor opacity at that moment.
    /// Bisects "the SVG never parsed / seeded" vs "the geometry exists but the
    /// frame does not contain it". Sample at or after the actor's entrance —
    /// a pre-reveal sample reads as empty for a hidden-by-default actor.
    /// Returns `null` when no document is loaded.
    pub fn debug_svg_stats(&self, time_ms: f64) -> Result<JsValue, JsError> {
        let time_ms = time_ms as u64;
        let Some(target) = &self.target else {
            return serde_wasm_bindgen::to_value(&Option::<()>::None)
                .map_err(|e| JsError::new(&format!("failed to serialize: {e}")));
        };
        let timeline = match target {
            BuildTarget::SingleScene(timeline) => timeline,
            BuildTarget::MultiScene(composition) => match composition.scenes.values().next() {
                Some(scene) => &scene.timeline,
                None => {
                    return Err(JsError::new("composition has no scenes"));
                },
            },
        };
        let mut cache: Vec<SvgCacheEntry> = timeline
            .asset_cache()
            .svg_paths()
            .map(|(url, paths)| SvgCacheEntry {
                url: url.clone(),
                paths: paths.len(),
            })
            .collect();
        cache.sort_by(|a, b| a.url.cmp(&b.url));
        let mut tracks: Vec<SvgTrackEntry> = timeline
            .tracks()
            .iter()
            .map(|(label, track)| SvgTrackEntry {
                label: label.clone(),
                paths: track.svg_paths().len(),
                paths_at_t: track.svg_paths_at(time_ms).map(|paths| paths.len()),
                opacity_at_t: animatix::timeline::TrackAccessor::get(
                    &track.style.opacity,
                    time_ms,
                    1.0,
                ),
            })
            .collect();
        tracks.sort_by(|a, b| a.label.cmp(&b.label));
        // Probe stage 3: evaluate the frame like the render loop would and
        // count what actually made it into the vello encoding. A missing draw
        // here points at the engine; a present draw that renders nothing
        // points at the GPU path.
        let mut no_filters: Option<&mut dyn animatix::timeline::effects::FilterBackend> = None;
        let scene = timeline.evaluate_with_debug(
            time_ms as f64 / 1000.0,
            self.dims,
            DebugRenderOptions::default(),
            &mut no_filters,
        );
        let encoding = scene.encoding();
        let vello = VelloStats {
            draws: encoding.draw_tags.len(),
            paths: encoding.n_paths,
        };
        serde_wasm_bindgen::to_value(&SvgStats {
            cache,
            tracks,
            vello,
        })
        .map_err(|e| JsError::new(&format!("failed to serialize svg stats: {e}")))
    }

    /// Diagnostic: clear the canvas with a solid color through raw wgpu,
    /// bypassing vello entirely. Bisects "scene content" vs "present chain".
    pub fn debug_fill(&mut self, r: f32, g: f32, b: f32) -> Result<(), JsError> {
        with_engine(|ctx| self.debug_fill_inner(ctx, r, g, b))
    }

    fn debug_fill_inner(
        &mut self,
        ctx: Result<&EngineContext, String>,
        r: f32,
        g: f32,
        b: f32,
    ) -> Result<(), JsError> {
        let ctx = ctx.map_err(|e| JsError::new(&e))?;
        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            },
            _ => return Ok(()),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("animatix-web debug fill"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("animatix-web debug fill pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: r as f64,
                            g: g as f64,
                            b: b as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        // The screen now holds a flat colour that no frame signature describes,
        // so the next identical frame must draw rather than dedup.
        self.dedup.invalidate();
        Ok(())
    }
}
