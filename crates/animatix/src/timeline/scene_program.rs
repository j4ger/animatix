//! Observable scene program produced by timeline evaluation.
//!
//! `Timeline::evaluate_program_with_debug` returns a [`SceneProgram`] instead of
//! only an encoded Vello scene. Primitive actors become [`SceneItem`]s, while
//! the authoritative encoded scene remains the exact render target for filters,
//! masks, static subtrees, and legacy paths. The existing `evaluate_with_debug`
//! API remains as a thin scene-only wrapper over the same program, so preview,
//! offscreen rendering, and export all consume the same frame description.

use std::collections::HashMap;

use crate::diagnostics::Diagnostic;
use crate::primitives::RenderCommand;
use crate::timeline::SceneDimensions;

/// A structured frame produced by timeline evaluation.
#[derive(Clone, Default)]
pub struct SceneProgram {
    /// Output canvas dimensions used to render the background fill.
    pub dimensions: SceneDimensions,
    /// Scene background color as RGBA in 0..1.
    pub background: [f32; 4],
    /// Authoritative encoded Vello scene for this frame.
    ///
    /// Kept as the exact render target because static-subtree, filter, mask,
    /// and legacy render paths encode directly to Vello. The structured
    /// `items` field is observable for tooling/testing while `scene` remains
    /// what is actually drawn.
    pub scene: vello::Scene,
    /// Primitive draw items observed during evaluation.
    pub items: Vec<SceneItem>,
    /// Precise world-space actor bounds discovered during evaluation.
    pub precise_bounds: HashMap<String, kurbo::Rect>,
    /// Runtime diagnostics produced while evaluating this frame.
    pub diagnostics: Vec<Diagnostic>,
}

/// A single primitive actor item with its local transform and inherited opacity.
#[derive(Clone, Debug)]
pub struct SceneItem {
    /// Local-to-world transform for the actor.
    pub transform: kurbo::Affine,
    /// Inherited opacity multiplier.
    pub opacity: f32,
    /// Draw commands emitted by the primitive.
    pub commands: Vec<RenderCommand>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{SceneDimensions, VelloPath};

    #[test]
    fn program_holds_scene_and_items() {
        let program = SceneProgram {
            dimensions: SceneDimensions {
                width: 10,
                height: 10,
            },
            background: [0.0, 0.0, 0.0, 1.0],
            scene: vello::Scene::new(),
            items: vec![SceneItem {
                transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                commands: vec![RenderCommand::Paths {
                    paths: vec![VelloPath::default()],
                }],
            }],
            precise_bounds: HashMap::new(),
            diagnostics: Vec::new(),
        };
        assert_eq!(program.items.len(), 1);
        assert_eq!(program.items[0].transform, kurbo::Affine::IDENTITY);
    }

    #[test]
    fn timeline_program_collects_primitive_items() {
        let mut timeline = crate::timeline::Timeline::new();
        let mut track = crate::timeline::AnimationTrack::new("box".to_string());
        track.first_seen_ms = 0;
        track.shape.shape_type = Some({
            let mut t = crate::timeline::PropertyTrack::new(crate::timeline::ShapeType::Rect);
            t.add_keyframe(0, crate::timeline::ShapeType::Rect, crate::timeline::Easing::Linear);
            t
        });
        track.geometry.size = Some({
            let mut t = crate::timeline::PropertyTrack::new([50.0, 50.0]);
            t.add_keyframe(0, [50.0, 50.0], crate::timeline::Easing::Linear);
            t
        });
        track.style.color = Some({
            let mut t = crate::timeline::PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
            t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], crate::timeline::Easing::Linear);
            t
        });
        timeline.tracks_mut().insert("box".to_string(), track);
        timeline.root_nodes.push("box".to_string());

        let mut fb = None;
        let program = timeline.evaluate_program_with_debug(
            0.0,
            SceneDimensions {
                width: 100,
                height: 100,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut fb,
        );
        assert!(!program.items.is_empty(), "primitive evaluation should be observable");
        assert!(!program.scene.encoding().is_empty());
    }
}
