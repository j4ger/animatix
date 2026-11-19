use animatix::composition::BuildTarget;
use animatix::timeline::{
    DebugRenderOptions, RuntimeInputs, SceneDimensions, Timeline, effects::FilterBackend,
};
use animatix_render::transition::{collect_flip_pairs, encode_flip_overlay, plan_shared_actors};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use super::super::{AmxPlayer, frame::raster_dims};
use super::{effects::Effects, gpu::Gpu};

#[wasm_bindgen]
pub async fn create_webgl_player(canvas: HtmlCanvasElement) -> Result<AmxPlayer, JsError> {
    console_error_panic_hook::set_once();
    let (surface, config, gpu) = Gpu::new(canvas.clone()).await.map_err(|e| JsError::new(&e))?;
    let mut player = AmxPlayer::new(canvas, surface, config);
    player.webgl = Some(gpu);
    Ok(player)
}

impl AmxPlayer {
    pub(in crate::web) fn validate_webgl_document(
        &mut self,
        target: &BuildTarget,
        dims: SceneDimensions,
        duration: f64,
    ) -> Result<(), JsError> {
        let gpu = self.webgl.as_mut().ok_or_else(|| JsError::new("WebGL2 is not initialized"))?;
        let raster = raster_dims(dims, self.render_scale);
        let size = (u16::try_from(raster.0)?, u16::try_from(raster.1)?);
        let inputs = RuntimeInputs::default();
        gpu.clear_images();
        // Autoplay starts at zero, while a paused document installs at its
        // finished frame. Prepare both before replacing a committed document.
        for time in [0.0, duration] {
            draw_document(gpu, target, dims, size, self.render_scale, time, &inputs)
                .map_err(|e| JsError::new(&e))?;
        }
        Ok(())
    }

    pub(in crate::web) fn render_webgl_frame(&mut self, time: f64) -> Result<(), JsError> {
        let width = self.canvas.width().max(1);
        let height = self.canvas.height().max(1);
        let signature = self.frame_signature(time, width, height);
        let gpu = self.webgl.as_mut().ok_or_else(|| JsError::new("WebGL2 is not initialized"))?;
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&gpu.device, &self.config);
            self.dedup.invalidate();
        }
        if self.dedup.is_presented(&signature) {
            self.frames_deduped += 1;
            return Ok(());
        }
        let target = self.target.as_ref().ok_or_else(|| JsError::new("no document"))?;
        let raster = raster_dims(self.dims, self.render_scale);
        let size = (u16::try_from(raster.0)?, u16::try_from(raster.1)?);
        let view = draw_document(
            gpu,
            target,
            self.dims,
            size,
            self.render_scale,
            time,
            &self.runtime_inputs,
        )
        .map_err(|e| JsError::new(&e))?;
        if gpu.present(&self.surface, &self.config, &view).map_err(|e| JsError::new(&e))? {
            self.dedup.record(signature);
            self.frames_drawn += 1;
        }
        Ok(())
    }
}

pub(in crate::web) fn draw_document(
    gpu: &mut Gpu,
    target: &BuildTarget,
    dims: SceneDimensions,
    size: (u16, u16),
    scale: f32,
    time: f64,
    inputs: &RuntimeInputs,
) -> Result<wgpu::TextureView, String> {
    let mut evaluated = Evaluation {
        gpu,
        dims,
        size,
        scale,
    };
    match target {
        BuildTarget::SingleScene(timeline) => evaluated.draw(timeline, time, &[], Some(inputs), 0),
        BuildTarget::MultiScene(composition) => {
            let (scene, local, blend) = composition.evaluate(time);
            let Some(blend) = blend else {
                let timeline = composition.scenes.get(&scene).ok_or("missing active scene")?;
                return evaluated.draw(&timeline.timeline, local, &[], None, 0);
            };
            let from = &composition.scenes[&blend.from_scene].timeline;
            let to = &composition.scenes[&blend.to_scene].timeline;
            let shared = plan_shared_actors(from, blend.from_local, to, blend.to_local);
            let pairs = collect_flip_pairs(
                &shared,
                from,
                blend.from_local,
                to,
                blend.to_local,
                dims,
                DebugRenderOptions::default(),
            );
            let from_view =
                evaluated.draw(from, blend.from_local, &shared.suppressed_from, None, 0)?;
            let to_view = evaluated.draw(to, blend.to_local, &shared.suppressed_to, None, 1)?;
            let output = evaluated.gpu.texture(2, size)?;
            evaluated.gpu.transition(&from_view, &to_view, &output, size, &blend)?;
            if pairs.is_empty() {
                return Ok(output);
            }
            let overlay = encode_flip_overlay(&pairs, blend.progress as f32, blend.easing);
            let view = evaluated.gpu.texture(0, size)?;
            evaluated.gpu.rasterize(&overlay, &view, size, scale)?;
            let composited = evaluated.gpu.texture(1, size)?;
            evaluated.gpu.draw_overlay(&output, &view, &composited, 1.0);
            Ok(composited)
        },
    }
}

struct Evaluation<'a> {
    gpu: &'a mut Gpu,
    dims: SceneDimensions,
    size: (u16, u16),
    scale: f32,
}

impl Evaluation<'_> {
    fn draw(
        &mut self,
        timeline: &Timeline,
        time: f64,
        suppressed: &[String],
        inputs: Option<&RuntimeInputs>,
        output: usize,
    ) -> Result<wgpu::TextureView, String> {
        let debug = DebugRenderOptions::default();
        let mut effects = Effects::default();
        let mut backend: Option<&mut dyn FilterBackend> = Some(&mut effects);
        let program = if let Some(inputs) = inputs {
            timeline
                .evaluate_program_with_inputs(time, self.dims, debug, &mut backend, inputs)
                .map_err(|e| e.to_string())?
        } else {
            timeline.evaluate_program_with_suppressed(
                time,
                self.dims,
                debug,
                &mut backend,
                suppressed,
            )
        };
        if let Some(error) = effects.error {
            return Err(error);
        }
        let view = self.gpu.texture(output, self.size)?;
        let inline = self.gpu.draw_inline(effects.inline, self.scale)?;
        self.gpu.render(
            &program.scene,
            &view,
            self.size,
            kurbo::Affine::scale(f64::from(self.scale)),
            &inline,
        )?;
        self.gpu.draw_effects(&view, self.size, self.scale, effects.layers, &inline)?;
        Ok(view)
    }
}
