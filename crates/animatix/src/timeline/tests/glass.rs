//! `Glass` — what a backdrop-blur scope asks the renderer for.
//!
//! The GPU half of the feature cannot be observed from this crate, so these test
//! the seam: the scope must queue a backdrop pass over the right rect *before*
//! the composite that carries its own children, because queue order is what
//! becomes z-order once the frame is split.

use super::*;
use crate::timeline::SceneDimensions;
use crate::timeline::effects::{
    EffectChain, EffectRegion, FilterBackend, PendingBackdrop, PendingLayer,
};
use crate::timeline::image::SceneImage;

#[derive(Default)]
struct SpyBackend {
    backdrops: Vec<PendingBackdrop>,
    overlays: usize,
    in_scene_calls: usize,
}

impl FilterBackend for SpyBackend {
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        _scene: &vello::Scene,
        _dimensions: SceneDimensions,
        _region: Option<EffectRegion>,
        _chain: &EffectChain,
    ) -> Result<SceneImage, String> {
        self.in_scene_calls += 1;
        Err("spy backend does not render".to_string())
    }

    fn enqueue_backdrop(&mut self, backdrop: &PendingBackdrop) -> Result<(), String> {
        self.backdrops.push(backdrop.clone());
        Ok(())
    }

    fn enqueue_scene_composite(
        &mut self,
        _scene: &vello::Scene,
        _dimensions: SceneDimensions,
        _origin: [f32; 2],
        _alpha: f32,
    ) -> Result<(), String> {
        self.overlays += 1;
        Ok(())
    }

    fn take_pending_composites(&mut self) -> Vec<PendingLayer> {
        Vec::new()
    }
}

fn build(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new()).output
}

fn scene() -> SceneDimensions {
    SceneDimensions {
        width: 640,
        height: 360,
    }
}

const GLASS_SCENE: &str = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
plate: Rect, size: (600, 200), at: (320, 180), color: accent.primary
card: Glass, at: (320, 180), size: (200, 100), corner_radius: 12 {
    frost: Blur, radius: 18
    label: Text, text: "sharp", font_size: 20, at: (0, 0)
}
"#;

#[test]
fn a_glass_scope_queues_one_backdrop_and_one_overlay() {
    let timeline = build(GLASS_SCENE);
    let mut spy = SpyBackend::default();
    let mut backend: Option<&mut dyn FilterBackend> = Some(&mut spy);
    timeline.evaluate_program_with_debug(
        0.0,
        scene(),
        crate::timeline::DebugRenderOptions::default(),
        &mut backend,
    );

    assert_eq!(spy.backdrops.len(), 1, "the panel must ask for exactly one backdrop pass");
    assert_eq!(spy.overlays, 1, "the scope's children ride in one overlay");
    assert_eq!(spy.in_scene_calls, 0, "a backdrop must not go through the readback path");
    let card = timeline.tracks.get("card").expect("the Glass scope has a track");
    assert_eq!(
        card.children,
        vec!["label".to_string()],
        "the build must attach the panel's content children to the scope, or the          overlay composites an empty scene and the text disappears"
    );
}

#[test]
fn the_backdrop_region_pads_beyond_the_panel_it_clips_to() {
    let timeline = build(GLASS_SCENE);
    let mut spy = SpyBackend::default();
    let mut backend: Option<&mut dyn FilterBackend> = Some(&mut spy);
    timeline.evaluate_program_with_debug(
        0.0,
        scene(),
        crate::timeline::DebugRenderOptions::default(),
        &mut backend,
    );

    let backdrop = spy.backdrops.first().expect("a Glass scope queues a backdrop");
    // The panel: at (320, 180) with size (200, 100) on a 640x360 stage.
    assert_eq!(backdrop.clip.origin, [220.0, 130.0]);
    assert_eq!((backdrop.clip.size.width, backdrop.clip.size.height), (200, 100));
    assert!(
        backdrop.region.size.width >= backdrop.clip.size.width,
        "the sampled region must reach past the panel edge so the blur has real \
         pixels to read: {:?}",
        backdrop.region.size
    );
    assert!(
        backdrop.region.origin[0] <= backdrop.clip.origin[0],
        "the sampled region starts at or before the panel"
    );
    assert_eq!(
        backdrop.corner_radius, 12.0,
        "the composite is clipped to the panel's own corners"
    );
    let blur = backdrop
        .chain
        .instances
        .iter()
        .find(|instance| instance.enabled)
        .expect("the declared stage reaches the chain");
    assert_eq!(blur.id.as_str(), "Blur", "the frost is the scope's chain");
}

/// A `Glass` scope must frost whoever declares it, not only the scene's last
/// root actor.
///
/// The root loop passes `allow_pending_composites` down from
/// `can_post_composite_filter`, whose rule is "may I blit after the frame, given
/// something renders after me?" — for a `Filter` that is a real hazard, and the
/// scope falls back to the inline readback path. `Glass` has no such fallback:
/// reading that flag here silently disabled **every** panel that was not the
/// scene's last root, drawing the card with no frost at all and saying nothing.
/// Blitting after the frame is what a backdrop *is*, so the flag must not gate
/// it — the frost is defined as the finished frame under the panel's rect.
#[test]
fn a_glass_scope_still_frosts_when_something_renders_after_it() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
plate: Rect, size: (600, 200), at: (320, 180), color: accent.primary
card: Glass, at: (320, 180), size: (200, 100), corner_radius: 12 {
    frost: Blur, radius: 18
    label: Text, text: "sharp", font_size: 20
}
hud: Rect, size: (80, 20), at: (60, 40), color: accent.warning
"#,
    );
    assert_ne!(
        timeline.root_nodes.last().map(String::as_str),
        Some("card"),
        "the scene must actually put something after the scope, or this test          measures the case that already worked"
    );

    let mut spy = SpyBackend::default();
    let mut backend: Option<&mut dyn FilterBackend> = Some(&mut spy);
    timeline.evaluate_program_with_debug(
        0.0,
        scene(),
        crate::timeline::DebugRenderOptions::default(),
        &mut backend,
    );

    assert_eq!(spy.backdrops.len(), 1, "a later sibling must not cost the panel its frost");
    assert_eq!(spy.overlays, 1, "and its children still ride above it");
}

#[test]
fn a_glass_scope_without_stages_costs_no_backdrop() {
    let timeline = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
plain: Glass, at: (320, 180), size: (200, 100) {
    label: Text, text: "no frost", font_size: 20
}
"#,
    );
    let mut spy = SpyBackend::default();
    let mut backend: Option<&mut dyn FilterBackend> = Some(&mut spy);
    timeline.evaluate_program_with_debug(
        0.0,
        scene(),
        crate::timeline::DebugRenderOptions::default(),
        &mut backend,
    );

    assert!(spy.backdrops.is_empty(), "an empty chain must not pay for a copy and a pass");
    assert_eq!(spy.overlays, 0);
}

/// A scope that paints nothing must not invite the paint properties.
///
/// `Glass` carries no `ShapeKind`, the same choice `Filter` and `Mask` make, so
/// the shape predicates (`Applicable::AllShapes*`, which key off
/// `caps.shape.is_some()`) do not accept it — and the checker names the drop
/// instead of letting `fill_opacity: 0.5` silently do nothing. This is the guard
/// that keeps the surface decision from drifting back: re-adding
/// `.with_shape(ShapeKind::Rect)` to the catalog card makes these two warnings
/// disappear and fails here.
#[test]
fn a_glass_scope_names_the_surface_properties_it_cannot_use() {
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (320, 200) }
c: Glass, at: (160, 100), size: (120, 60), fill_opacity: 0.5, stroke_width: 3 {
    f: Blur, radius: 10
}
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    let named: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::InapplicableProperty)
        .map(|d| d.message.clone())
        .collect();
    let joined = named.join("\n");
    for dropped in ["fill_opacity", "stroke_width"] {
        assert!(
            joined.contains(dropped) && joined.contains("Glass"),
            "`Glass` must name the surface property it drops ({dropped}): {named:?}"
        );
    }
    assert_eq!(named.len(), 2, "exactly the two surface properties: {named:?}");

    // And the property it *does* consume stays quiet: `corner_radius` is the
    // frost's clip, not a paint.
    let rounded = source.replace("fill_opacity: 0.5, stroke_width: 3", "corner_radius: 12");
    let (rast, rerr) = animatix_syntax::parser::parse_source(&rounded);
    assert!(rerr.is_empty(), "parse errors: {rerr:?}");
    let rrep =
        Timeline::build_with_diagnostics(&rast.expect("AST"), &std::collections::HashMap::new());
    assert!(
        rrep.diagnostics
            .iter()
            .all(|d| d.code != crate::diagnostics::DiagnosticCode::InapplicableProperty),
        "corner_radius is consumed by the frost clip: {:?}",
        rrep.diagnostics
    );
}
