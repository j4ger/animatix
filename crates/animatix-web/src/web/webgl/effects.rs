use animatix::timeline::{
    SceneDimensions,
    effects::{EffectChain, EffectRegion, FilterBackend, PendingBackdrop},
    image::SceneImage,
};

#[derive(Default)]
pub(super) struct Effects {
    pub(super) layers: Vec<Layer>,
    pub(super) inline: Vec<Inline>,
    pub(super) error: Option<String>,
}

pub(super) struct Inline {
    pub(super) id: vello_gpu::TextureId,
    pub(super) scene: vello::Scene,
    pub(super) region: EffectRegion,
    pub(super) radius: f32,
}

pub(super) enum Layer {
    Backdrop(PendingBackdrop),
    Scene {
        scene: vello::Scene,
        radius: f32,
        alpha: f32,
    },
}

impl FilterBackend for Effects {
    fn record_inline_filter(
        &mut self,
        dst: &mut vello::Scene,
        source: &vello::Scene,
        region: EffectRegion,
        chain: &EffectChain,
        alpha: f32,
    ) -> Option<Result<(), String>> {
        let radius = match blur_radius(chain) {
            Ok(radius) => radius,
            Err(error) => {
                self.error = Some(error.clone());
                return Some(Err(error));
            },
        };
        let id = vello_gpu::TextureId(self.inline.len() as u64);
        let (width, height) = (f64::from(region.size.width), f64::from(region.size.height));
        let [x, y] = region.origin.map(f64::from);
        dst.record_texture(id.0, kurbo::Rect::new(x, y, x + width, y + height), alpha);
        self.inline.push(Inline {
            id,
            scene: source.clone(),
            region,
            radius,
        });
        Some(Ok(()))
    }

    fn render_scene_to_image_gpu_filtered(
        &mut self,
        _: &vello::Scene,
        _: SceneDimensions,
        _: Option<EffectRegion>,
        _: &EffectChain,
    ) -> Result<SceneImage, String> {
        let error = "WebGL2 experiment cannot yet composite an inline Filter".to_string();
        self.error = Some(error.clone());
        Err(error)
    }

    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        _: SceneDimensions,
        _: Option<EffectRegion>,
        chain: &EffectChain,
        alpha: f32,
    ) -> Result<(), String> {
        match blur_radius(chain) {
            Ok(radius) => {
                self.layers.push(Layer::Scene {
                    scene: scene.clone(),
                    radius,
                    alpha,
                });
                Ok(())
            },
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            },
        }
    }

    fn enqueue_backdrop(&mut self, backdrop: &PendingBackdrop) -> Result<(), String> {
        match blur_radius(&backdrop.chain) {
            Ok(_) => {
                self.layers.push(Layer::Backdrop(backdrop.clone()));
                Ok(())
            },
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            },
        }
    }

    fn enqueue_scene_composite(
        &mut self,
        scene: &vello::Scene,
        _: SceneDimensions,
        origin: [f32; 2],
        alpha: f32,
    ) -> Result<(), String> {
        if origin != [0.0, 0.0] {
            let error = "WebGL2 experiment expects a full-scene overlay".to_string();
            self.error = Some(error.clone());
            return Err(error);
        }
        self.layers.push(Layer::Scene {
            scene: scene.clone(),
            radius: 0.0,
            alpha,
        });
        Ok(())
    }
}

pub(super) fn blur_radius(chain: &EffectChain) -> Result<f32, String> {
    let mut enabled = chain.instances.iter().filter(|effect| effect.enabled);
    match (enabled.next(), enabled.next()) {
        (None, _) => Ok(0.0),
        (Some(effect), None) if effect.id.as_str().eq_ignore_ascii_case("blur") => {
            Ok(effect.params.f32_at(0))
        },
        _ => Err("WebGL2 experiment supports one Blur per effect chain".into()),
    }
}
