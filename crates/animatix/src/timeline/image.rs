#[cfg(feature = "image-decode")]
use image::GenericImageView;

/// Loaded image data with its natural pixel dimensions.
#[derive(Clone, Debug)]
pub struct SceneImage {
    /// Vello image data buffer.
    pub data: peniko::ImageData,
    /// Natural width and height in pixels.
    pub natural_size: [f32; 2],
}

impl SceneImage {
    /// Wrap tightly packed RGBA8 pixels for the renderer.
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            data: peniko::ImageData {
                data: pixels.into(),
                format: peniko::ImageFormat::Rgba8,
                alpha_type: peniko::ImageAlphaType::Alpha,
                width,
                height,
            },
            natural_size: [width as f32, height as f32],
        }
    }
}

#[cfg(feature = "image-decode")]
fn scene_image(image: image::DynamicImage) -> SceneImage {
    let (width, height) = image.dimensions();
    SceneImage::from_rgba8(width, height, image.into_rgba8().into_raw())
}

/// Load an image from disk into a `SceneImage`.
#[cfg(feature = "image-decode")]
pub fn load_image(path: &str) -> Result<SceneImage, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    load_image_from_bytes(&bytes)
}

/// Alias for load_image to match the naming convention used in assets.rs
#[cfg(feature = "image-decode")]
pub fn load_image_file(path: &str) -> Result<SceneImage, String> {
    load_image(path)
}

/// Decode an image from in-memory bytes into a `SceneImage` (the web player
/// fetches the encoded bytes and hands them over).
#[cfg(feature = "image-decode")]
pub fn load_image_from_bytes(bytes: &[u8]) -> Result<SceneImage, String> {
    super::media_capability::image(bytes)
}

#[cfg(feature = "image-decode")]
pub(super) fn decode_bytes(bytes: &[u8]) -> Result<SceneImage, String> {
    let image = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    Ok(scene_image(image))
}
