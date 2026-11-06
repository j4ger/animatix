//! Helper contexts shared by the native plugin ABI test modules.

use crate::timeline::assets::AssetCache;
use crate::timeline::plan::PropertyPlan;

use super::*;

pub(super) static BUILD_CHILD_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub(super) unsafe extern "C" fn record_build_children(ctx: *mut NativePrimitiveBuildCtx) -> i32 {
    let ctx = unsafe { &*ctx };
    BUILD_CHILD_COUNT
        .store(unsafe { (ctx.get_child_count)(ctx.host) }, std::sync::atomic::Ordering::SeqCst);
    NATIVE_STATUS_OK
}

pub(super) fn sample_evaluate_ctx(asset_cache: &AssetCache) -> EvaluateCtx<'_> {
    sample_evaluate_ctx_with_payload(asset_cache, None)
}

/// Like [`sample_evaluate_ctx`], but with the entity's payload component so
/// native commands that read primitive state (e.g. the actor image) can see it.
pub(super) fn sample_evaluate_ctx_with_payload<'a>(
    asset_cache: &'a AssetCache,
    payload: Option<&'a crate::primitives::PrimitivePayload>,
) -> EvaluateCtx<'a> {
    EvaluateCtx {
        id: crate::timeline::EntityId(0),
        label: "pulse",
        // These helper contexts carry no World identity; the implied type is
        // Rect.
        caps: animatix_std::caps_for_type("Rect").unwrap_or_default(),
        time_ms: 0,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: crate::timeline::SceneDimensions {
            width: 640,
            height: 360,
        },
        background_color: [0.0; 4],
        overrides: None,
        vector_paths: &[],
        asset_cache,
        highlight: None,
        property_plan: Box::leak(Box::new(PropertyPlan::default())),
        world: None,
        // The shape, text, geometry and style tiers live in World; these helper
        // contexts build no World, so default tiers are attached.
        style: Box::leak(Box::new(crate::timeline::animation_track::StyleTracks::default())),
        shape: Some(Box::leak(Box::new(crate::timeline::animation_track::ShapeTracks::default()))),
        text: Some(Box::leak(Box::new(crate::timeline::animation_track::TextTracks::default()))),
        geometry: Box::leak(Box::new(crate::timeline::animation_track::GeometryTracks::default())),
        payload,
        target_resolver: None,
        legend: None,
    }
}

/// Like [`sample_evaluate_ctx`], but with the entity's property plan so native
/// property reads can resolve registry-driven slots.
pub(super) fn sample_evaluate_ctx_with_plan<'a>(
    asset_cache: &'a AssetCache,
    property_plan: &'a PropertyPlan,
) -> EvaluateCtx<'a> {
    let mut ctx = sample_evaluate_ctx(asset_cache);
    ctx.property_plan = property_plan;
    ctx
}

pub(super) unsafe extern "C" fn pulse_evaluate(ctx: *mut NativePrimitiveEvaluateCtx) -> i32 {
    let ctx = unsafe { &*ctx };
    let Some(append_path) = ctx.append_path else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let command = NativePathCommand {
        kind: NATIVE_PATH_ELLIPSE,
        x: 0.0,
        y: 0.0,
        width: 60.0,
        height: 60.0,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 0.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [0.2, 0.8, 1.0, 1.0],
        stroke: [1.0, 1.0, 1.0, 1.0],
        stroke_width: 2.0,
        line_cap: 0,
        line_join: 0,
    };
    unsafe { (append_path)(ctx.host, command) }
}
