use image::GenericImageView;

#[derive(Clone)]
pub struct SceneImage {
    pub data: vello::peniko::ImageData,
    pub natural_size: [f32; 2],
}

pub fn load_image(path: &str) -> Option<SceneImage> {
    let image = image::open(path).ok()?;
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    let raw = rgba.into_raw();

    let data = vello::peniko::ImageData {
        data: raw.into(),
        format: vello::peniko::ImageFormat::Rgba8,
        alpha_type: vello::peniko::ImageAlphaType::Alpha,
        width,
        height,
    };

    Some(SceneImage {
        data,
        natural_size: [width as f32, height as f32],
    })
}
