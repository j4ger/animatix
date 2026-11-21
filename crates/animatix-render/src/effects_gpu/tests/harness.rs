use animatix::scene::Scene;
use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::{
    EffectChain, EffectId, EffectInstance, EffectParamValue, EffectParams,
};
use wgpu::util::DeviceExt;

use super::{EffectsExtent, EffectsGpu, FORMAT, readback_rgba};
use crate::frame_gpu::FrameGpu;

pub(super) const DIMS: SceneDimensions = SceneDimensions {
    width: 64,
    height: 64,
};

pub(super) struct EffectHarness {
    pub(super) effects: EffectsGpu,
    frame: FrameGpu,
}

pub(super) struct EffectImage {
    pub(super) size: [u32; 2],
    pub(super) rgba: Vec<u8>,
}

impl EffectHarness {
    pub(super) fn new() -> Option<Self> {
        let (device, queue) = pollster::block_on(crate::test_support::create_headless_device(
            "Animatix Effect Pixel Tests",
        ))?;
        Some(Self {
            frame: FrameGpu::new(&device, &queue),
            effects: EffectsGpu::new(device, queue),
        })
    }

    pub(super) fn render(
        &mut self,
        scene: &Scene,
        dims: SceneDimensions,
        chain: &EffectChain,
        scale: f32,
    ) -> EffectImage {
        let size = wgpu::Extent3d {
            width: (dims.width as f32 * scale).round() as u32,
            height: (dims.height as f32 * scale).round() as u32,
            depth_or_array_layers: 1,
        };
        let target = self.effects.device.create_texture(&target_descriptor(size));
        let view = target.create_view(&Default::default());
        self.frame
            .render_into(scene, &view, kurbo::Affine::scale(f64::from(scale)))
            .expect("rasterize the effect input");
        let mut rgba = self.apply(&view, chain, scale);
        for [red, green, blue, alpha] in rgba.as_chunks_mut::<4>().0 {
            let a = u16::from(*alpha);
            // Keep zero-alpha RGB intact so regressions can detect leaked color.
            for channel in [red, green, blue] {
                let straight = (u16::from(*channel) * 255 + a / 2).checked_div(a);
                *channel = straight.map_or(*channel, |value| value.min(255) as u8);
            }
        }
        EffectImage {
            size: [size.width, size.height],
            rgba,
        }
    }

    pub(super) fn apply_pixels(&mut self, pixels: &[u8], chain: &EffectChain) -> Vec<u8> {
        let size = wgpu::Extent3d {
            width: DIMS.width,
            height: DIMS.height,
            depth_or_array_layers: 1,
        };
        let texture = self.effects.device.create_texture_with_data(
            &self.effects.queue,
            &target_descriptor(size),
            wgpu::util::TextureDataOrder::LayerMajor,
            pixels,
        );
        self.apply(&texture.create_view(&Default::default()), chain, 1.0)
    }

    fn apply(&mut self, input: &wgpu::TextureView, chain: &EffectChain, scale: f32) -> Vec<u8> {
        let width = input.texture().width();
        let height = input.texture().height();
        let output = self
            .effects
            .apply(
                input,
                chain,
                EffectsExtent {
                    width,
                    height,
                    origin: [0.0, 0.0],
                },
                scale,
            )
            .expect("run the portable effect chain");
        readback_rgba(
            &self.effects.device,
            &self.effects.queue,
            output.view.texture(),
            width,
            height,
        )
    }
}

fn target_descriptor(size: wgpu::Extent3d) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("Animatix Effect Pixel Test Input"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }
}

pub(super) fn effect_chain(name: &str, values: Vec<EffectParamValue>) -> EffectChain {
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new(name),
            enabled: true,
            params: EffectParams { values },
        }],
        time_ms: 0.0,
        ..Default::default()
    }
}

pub(super) fn blur_chain(radius: f32) -> EffectChain {
    effect_chain("Blur", vec![EffectParamValue::F32(radius)])
}
