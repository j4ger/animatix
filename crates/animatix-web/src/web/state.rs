//! Shared state types for the web canvas renderers.

use std::cell::RefCell;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::scene::Scene;
use animatix::timeline::{BuildQuality, RuntimeInputs, SceneDimensions};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::transition::TransitionCompositor;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Process-wide WebGPU context shared by every WebGPU player.
pub(super) struct EngineContext {
    pub(super) _instance: wgpu::Instance,
    pub(super) _adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    /// Loss callbacks can clear only the device generation that fired them.
    pub(super) generation: u64,
    pub(super) core: RefCell<RendererCore>,
    #[cfg(feature = "vello-gpu")]
    pub(super) frame_gpu: RefCell<animatix_render::frame_gpu::FrameGpu>,
}

/// Player and editor state bound to one canvas.
#[wasm_bindgen]
pub struct AmxPlayer {
    pub(super) canvas: HtmlCanvasElement,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) device_generation: u64,
    #[cfg(feature = "vello-gpu")]
    pub(super) webgl: Option<std::rc::Rc<super::webgl::Gpu>>,
    pub(super) filter_backend: Option<GpuFilterBackend>,
    pub(super) filter_backend_to: Option<GpuFilterBackend>,
    pub(super) offscreen: Option<OffscreenTarget>,
    pub(super) offscreen_to: Option<OffscreenTarget>,
    pub(super) composite: Option<OffscreenTarget>,
    pub(super) fonts: Vec<Vec<u8>>,
    pub(super) font_context: Option<FontContext>,
    pub(super) modules: Vec<(String, String)>,
    pub(super) vars: Vec<(String, String)>,
    pub(super) runtime_inputs: RuntimeInputs,
    pub(super) quality: BuildQuality,
    pub(super) compositor: Option<TransitionCompositor>,
    pub(super) dims: SceneDimensions,
    pub(super) duration_s: f64,
    pub(super) target: Option<BuildTarget>,
    pub(super) render_scale: f32,
    pub(super) scaled: Scene,
    pub(super) dedup: animatix::timeline::frame_signature::FrameDedup,
    pub(super) render_generation: u64,
    pub(super) frames_drawn: u64,
    pub(super) frames_deduped: u64,
}

#[derive(Clone)]
pub(super) struct OffscreenTarget {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct FrameTarget {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
}
