use image::GenericImageView;

/// Loaded image data with its natural pixel dimensions.
#[derive(Clone, Debug)]
pub struct SceneImage {
    /// Vello image data buffer.
    pub data: vello::peniko::ImageData,
    /// Natural width and height in pixels.
    pub natural_size: [f32; 2],
}

/// Load an image from disk into a `SceneImage`.
pub fn load_image(path: &str) -> Result<SceneImage, String> {
    let image = image::open(path).map_err(|error| error.to_string())?;
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

        Ok(SceneImage {
            data,
            natural_size: [width as f32, height as f32],
        })
}

/// Alias for load_image to match the naming convention used in assets.rs
pub fn load_image_file(path: &str) -> Result<SceneImage, String> {
    load_image(path)
}
