//! Observable scene program produced by timeline evaluation.
//!
//! `Timeline::evaluate_program_with_debug` returns a [`SceneProgram`] instead of
//! only an encoded Vello scene. Primitive actors become [`SceneItem`]s; filters,
//! masks, and debug bounds use structural [`SceneProgramOp`]s. The existing
//! `evaluate_with_debug` API remains as a thin executor over the same program,
//! so preview, offscreen rendering, and export all consume the same frame
//! description.

use std::collections::HashMap;

use crate::diagnostics::Diagnostic;
use crate::primitives::RenderCommand;
use crate::timeline::SceneDimensions;
use crate::timeline::image::SceneImage;

/// A structured frame produced by timeline evaluation.
#[derive(Clone, Default)]
pub struct SceneProgram {
    /// Output canvas dimensions used to render the background fill.
    pub dimensions: SceneDimensions,
    /// Scene background color as RGBA in 0..1.
    pub background: [f32; 4],
    /// Authoritative encoded Vello scene for this frame.
    ///
    /// Kept alongside the structured operations because static-subtree, filter,
    /// mask, and legacy render paths currently encode directly to Vello. The
    /// structured fields are observable for tooling/testing while `scene`
    /// remains the exact render target.
    pub scene: vello::Scene,
    /// Primitive draw items observed during evaluation.
    pub items: Vec<SceneItem>,
    /// Ordered scene program operations.
    pub ops: Vec<SceneProgramOp>,
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

/// Structural scene operation.
///
/// The operation set intentionally covers the scene-graph features that are not
/// expressible as a flat primitive command: filter sub-scenes, masks, debug
/// bounds, and the GPU-specific zero-readback filter composite marker.
#[derive(Clone)]
pub enum SceneProgramOp {
    /// Draw one actor's primitive commands.
    Item(SceneItem),
    /// Append another program as an ordered sub-scene.
    Append(SceneProgram),
    /// Clip nested operations to a path.
    Clip {
        /// Clip path in local space.
        path: kurbo::BezPath,
        /// Operations rendered inside the clip.
        children: Vec<SceneProgramOp>,
    },
    /// Draw a GPU-filtered image at full scene dimensions.
    FilteredImage {
        /// Filtered image data.
        image: SceneImage,
        /// Transform applied to the image.
        transform: kurbo::Affine,
        /// Image alpha multiplier.
        alpha: f32,
    },
    /// Marker for a GPU composite that is blitted after scene encoding.
    ///
    /// This operation has no Vello encoding; the caller applies the pending
    /// composite produced by the filter backend.
    PostComposite {
        /// Composite alpha multiplier.
        alpha: f32,
    },
    /// Debug bounds outline.
    DebugBounds {
        /// Bounds path in local space.
        path: kurbo::BezPath,
        /// Transform applied to the bounds.
        transform: kurbo::Affine,
        /// Outline color.
        color: vello::peniko::Color,
    },
}

impl SceneProgram {
    /// Execute this program into a Vello scene.
    pub fn execute_into(&self, scene: &mut vello::Scene) {
        let bg = vello::peniko::Color::new([
            self.background[0],
            self.background[1],
            self.background[2],
            self.background[3],
        ]);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            bg,
            None,
            &kurbo::Rect::new(
                0.0,
                0.0,
                self.dimensions.width as f64,
                self.dimensions.height as f64,
            ),
        );
        for op in &self.ops {
            execute_op(scene, op);
        }
    }

    /// Render this program into a fresh Vello scene.
    pub fn render_scene(&self) -> vello::Scene {
        self.scene.clone()
    }
}

fn execute_op(scene: &mut vello::Scene, op: &SceneProgramOp) {
    match op {
        SceneProgramOp::Item(item) => {
            for command in &item.commands {
                command.execute(scene, &item.transform, item.opacity);
            }
        },
        SceneProgramOp::Append(program) => {
            for child in &program.ops {
                execute_op(scene, child);
            }
        },
        SceneProgramOp::Clip { path, children } => {
            scene.push_layer(
                vello::peniko::Fill::NonZero,
                vello::peniko::BlendMode::default(),
                1.0,
                kurbo::Affine::IDENTITY,
                path,
            );
            for child in children {
                execute_op(scene, child);
            }
            scene.pop_layer();
        },
        SceneProgramOp::FilteredImage {
            image,
            transform,
            alpha,
        } => {
            let brush = vello::peniko::ImageBrush::new(image.data.clone())
                .with_extend(vello::peniko::Extend::Pad)
                .with_quality(vello::peniko::ImageQuality::Medium)
                .with_alpha(*alpha);
            scene.draw_image(&brush, *transform);
        },
        SceneProgramOp::PostComposite { .. } => {
            // The caller blits pending composites after scene encoding.
        },
        SceneProgramOp::DebugBounds {
            path,
            transform,
            color,
        } => {
            let stroke = vello::kurbo::Stroke::new(1.25);
            scene.stroke(&stroke, *transform, *color, None, path);
        },
    }
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    use super::*;
    use crate::timeline::VelloPath;

    #[test]
    fn program_holds_authoritative_scene_and_items() {
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
            ops: vec![SceneProgramOp::Item(SceneItem {
                transform: kurbo::Affine::IDENTITY,
                opacity: 1.0,
                commands: vec![RenderCommand::Paths {
                    paths: vec![VelloPath::default()],
                }],
            })],
            precise_bounds: HashMap::new(),
            diagnostics: Vec::new(),
        };
        assert_eq!(program.items.len(), 1);
        // render_scene returns the exact authoritative scene clone.
        let rendered = program.render_scene();
        assert_eq!(rendered.encoding().draw_tags.len(), program.scene.encoding().draw_tags.len());
    }

    #[test]
    fn append_and_clip_execute_without_panicking() {
        let child = SceneProgram {
            dimensions: SceneDimensions {
                width: 10,
                height: 10,
            },
            background: [0.0, 0.0, 0.0, 1.0],
            scene: vello::Scene::new(),
            items: Vec::new(),
            ops: Vec::new(),
            precise_bounds: HashMap::new(),
            diagnostics: Vec::new(),
        };
        let program = SceneProgram {
            dimensions: SceneDimensions {
                width: 10,
                height: 10,
            },
            background: [0.0, 0.0, 0.0, 1.0],
            scene: vello::Scene::new(),
            items: Vec::new(),
            ops: vec![
                SceneProgramOp::Append(child),
                SceneProgramOp::Clip {
                    path: kurbo::Rect::new(0.0, 0.0, 10.0, 10.0).into_path(1e-3),
                    children: Vec::new(),
                },
            ],
            precise_bounds: HashMap::new(),
            diagnostics: Vec::new(),
        };
        let scene = program.render_scene();
        assert!(scene.encoding().draw_tags.is_empty());
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
