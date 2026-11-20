use animatix::timeline::SceneDimensions;

/// Round the filter dispatch up to the shared 64 scene-pixel buckets. This is
/// also the shader's coordinate space, so changing it alters region-relative
/// effects such as Vignette and Grain even when the harvested area is identical.
pub(crate) fn quantize_region_size(size: SceneDimensions, canvas: SceneDimensions) -> (u32, u32) {
    let step = |v: u32, max: u32| (v.div_ceil(64).max(1) * 64).min(max.max(1));
    (step(size.width, canvas.width), step(size.height, canvas.height))
}
