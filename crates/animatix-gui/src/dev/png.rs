//! PNG saving shared by the screenshot session and the `widget-screenshot`
//! binary, so both write pixels through one resize/encode path.

/// Save an egui `ColorImage` as a PNG at logical-pixel size.
///
/// Screenshots arrive at physical resolution; downscaling by
/// `pixels_per_point` keeps the saved image at the size the layout was
/// reviewed at. Parent directories are created on demand.
pub fn save_screenshot(
    image: &egui::ColorImage,
    pixels_per_point: f32,
    path: &std::path::Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }

    let [img_w, img_h] = image.size;
    let logical_w = (img_w as f32 / pixels_per_point).round() as u32;
    let logical_h = (img_h as f32 / pixels_per_point).round() as u32;

    let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let src = image::RgbaImage::from_raw(img_w as u32, img_h as u32, raw)
        .ok_or("invalid screenshot buffer")?;
    let resized = image::imageops::resize(
        &src,
        logical_w.max(1),
        logical_h.max(1),
        image::imageops::FilterType::Lanczos3,
    );

    resized.save(path).map_err(|e| format!("save png: {e}"))?;
    Ok(())
}
