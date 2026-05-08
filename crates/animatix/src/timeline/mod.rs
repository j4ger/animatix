//! # Timeline Architecture
//!
//! `Timeline` is the compiled animation package: a scene graph (parent→children hierarchy)
//! with keyframed property tracks per actor. The scene graph drives transform/opacity
//! inheritance via DFS; tracks store animated values over time.
//!
//! ## Build-time vs frame-time boundary
//!
//! - **`Timeline::build()`** (in `build.rs`): one-time lowering pass that parses the AST,
//!   resolves imports, expands components, creates tracks, applies layout, compiles
//!   text/math/code paths, and loads assets.
//! - **`Timeline::evaluate()`** (in `runtime.rs` / `scene_eval.rs`): per-frame execution
//!   that samples tracks, runs `always` modifiers, resolves anchors/percent positions,
//!   and emits a `vello::Scene`.
//!
//! ## Submodule responsibilities
//!
//! | Module | Role |
//! |--------|------|
//! | `build.rs` | AST lowering into Timeline |
//! | `runtime.rs` / `scene_eval.rs` | Frame-time evaluation and render-scene assembly |
//! | `track.rs` | Keyframed property tracks and interpolation |
//! | `layout.rs` | Container placement (Row, Col, Grid, Stack) |
//! | `colorscheme.rs` | Built-in and inline colorscheme resolution |
//! | `morph.rs` | Path morphing between vector shapes |
//! | `plot.rs` | Adaptive sampling for graph plots |
//! | `utils.rs` | Expression evaluation |
//! | `modifier_runtime/` | IR and bytecode VM for `always` blocks |
//!
//! ## The compile boundary
//!
//! The practical compile target is the post-expansion program after module loading
//! and component expansion—not the raw parser AST.
pub mod actions;
pub mod assets;
mod actor_kind;
mod assignments;
mod build;
pub mod colorscheme;
mod declarations_text;
pub mod env;
pub mod image;
pub mod kurbo_shapes;
mod layout;
mod media;
pub(crate) mod taffy_layout;
pub(crate) mod modifier_runtime;
pub mod morph;
mod plot;
mod position;
pub(crate) mod property_engine;
pub(crate) mod property_groups;
pub(crate) mod property_registry;
pub(crate) mod value_parser;
mod primitive;
mod property_lookup;
mod runtime;
mod index;
mod scene_eval;
mod sequence;
mod shapes;
pub mod svg;
mod timing;
pub mod track;
pub mod utils;
pub mod vello_path;

use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
pub use actor_kind::ActorKind;
use actions::process_action;
use colorscheme::{BuiltInColorscheme, ResolvedColorscheme};
pub use env::{Environment, EvalError, Value, load_standard_library};
pub use index::TimelineIndex;
pub use image::load_image;
pub use kurbo_shapes::{KurboShape, morph_kurbo_shapes, morph_kurbo_shapes_default};
pub use morph::{MorphOptions, MorphStrategy};
use plot::{
    build_implicit_plot_path, sample_recursive_cartesian, sample_recursive_parametric,
    sample_recursive_polar,
};
use position::{
    apply_explicit_position_binding, mark_track_manual_position,
    preserve_discrete_position_state_before, preserve_instant_delayed_value,
    resolve_bound_position, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding,
};
pub use position::scene_anchor_point;
pub(crate) use primitive::PrimitiveDescriptor;
use property_lookup::{
    assignment_target_key, best_path_suggestion, evaluate_expr_with_lookup_diagnostic,
    for_iter_values, parse_color_in_env_with_lookup_diagnostic, parse_numeric_vec2,
    parse_numeric_vec2_with_lookup_diagnostic, set_lookup_color, set_lookup_scalar,
    set_lookup_vec2,
};
use shapes::{
    VectorShapeState, VectorShapeStyle, apply_vector_shape_defaults,
    apply_vector_shape_property, build_shape_vello_path, build_vector_shape_vello_path,
    finalize_vector_shape_state, parse_point_list_expr, shape_type_for_actor,
    vector_shape_primitive_for_actor_type,
    vector_shape_uses_custom_path,
};
pub use shapes::ShapeType;
pub use svg::parse_svg;
pub(crate) use timing::{ModifierHost, ParsedTimingModifiers, parse_timing_modifiers};
use timing::{
    config_string_value, has_non_default_morph_options, parse_stagger_interval_ms,
    push_modifier_diagnostic, push_unknown_target_path_diagnostic,
    push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind,
};
pub use track::{
    ActorKindId, ShapeKind, ActorHeader, GeometryTier, StyleTier, ActorPayload,
    AnimationTrack, Interpolate, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor,
    TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE,
};
/// Extend a time_ms vector with keyframe times from a property track, if present.
fn extend_track_times<T>(times: &mut Vec<u64>, track: &Option<PropertyTrack<T>>) {
    if let Some(t) = track.as_ref() {
        times.extend(t.keyframes.keys().copied());
    }
}

pub use utils::{evaluate_expr, parse_color, parse_color_in_env, resolve_color_in_env, time_to_ms};
pub use vello_path::VelloPath;

use crate::ast::{Expr, Modifier, Stmt};
use crate::timeline::modifier_runtime::ir::ModifierIrProgram;
use crate::easing::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutType {
    Row,
    Col,
    Grid,
    Stack,
}

impl LayoutType {
    pub fn from_container_ty(container_ty: &str) -> Self {
        match container_ty {
            "Row" => Self::Row,
            "Col" => Self::Col,
            "Grid" => Self::Grid,
            "Stack" => Self::Stack,
            _ => Self::Row,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerLayoutChild {
    /// Layout-admitted child label.
    ///
    /// This is a validated subset of scene-graph children used only for layout
    /// membership/order. Children excluded here still remain in `track.children`
    /// for scene traversal/rendering.
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct ContainerMetadata {
    pub layout_type: LayoutType,
    pub gap: f32,
    pub align: String,
    pub cols: Option<usize>,
    /// Raw authored child order snapshot for this container.
    ///
    /// This is retained for debugging/tests and for preserving the distinction
    /// between authored scene-graph membership and admitted layout membership.
    pub child_order: Vec<String>,
    /// Layout-admitted subset in authored order.
    ///
    /// Layout computation uses this field, while scene traversal/rendering still
    /// uses `track.children`.
    pub layout_children: Vec<ContainerLayoutChild>,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct DebugRenderOptions {
    pub draw_bounds: bool,
}

impl Default for SceneDimensions {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

pub struct Timeline {
    pub tracks: BTreeMap<String, AnimationTrack>,
    pub background_color: PropertyTrack<[f32; 4]>,
    pub root_nodes: Vec<String>,
    pub anon_counter: usize,
    pub env: Environment,
    pub modifiers: Vec<Stmt>,
    pub modifier_programs: Vec<ModifierIrProgram>,
    colorscheme: ResolvedColorscheme,
    external_colorschemes: std::collections::HashMap<String, ResolvedColorscheme>,
    auto_color_assignments: BTreeMap<String, usize>,
    next_auto_color_index: usize,
    pub container_metadata: BTreeMap<String, ContainerMetadata>,
    pub layout_engine: LayoutEngine,
    pub dynamic_layout: bool,
    pub asset_cache: assets::AssetCache,
    /// Per-container child order animations.
    /// Key: container label. Value: track of child label orderings.
    pub child_orders: BTreeMap<String, PropertyTrack<Vec<String>>>,
    /// Runtime text compiler with cache. Enables `always` blocks to change
    /// text content / font_family / font_size and have glyphs recompiled on-demand.
    pub text_compiler: std::cell::RefCell<crate::renderer::text::TextCompiler>,
    /// Frame evaluation cache: avoids re-evaluating when time and dimensions match.
    frame_cache: std::cell::RefCell<Option<FrameCacheEntry>>,
    /// Per-actor world-space bounding boxes from the last evaluate call.
    /// Each entry is (actor_label, world_bounds). Populated during evaluate.
    pub hit_regions: std::cell::RefCell<Vec<(String, kurbo::Rect)>>,
}

/// Cache entry for frame evaluation results.
#[derive(Clone)]
pub(crate) struct FrameCacheEntry {
    time_ms: u64,
    dimensions: SceneDimensions,
    has_modifiers: bool,
    has_dynamic_layout: bool,
    has_child_orders: bool,
    scene: vello::Scene,
}

impl Clone for Timeline {
    fn clone(&self) -> Self {
        Self {
            tracks: self.tracks.clone(),
            background_color: self.background_color.clone(),
            root_nodes: self.root_nodes.clone(),
            anon_counter: self.anon_counter,
            env: self.env.clone(),
            modifiers: self.modifiers.clone(),
            modifier_programs: self.modifier_programs.clone(),
            colorscheme: self.colorscheme.clone(),
            external_colorschemes: self.external_colorschemes.clone(),
            auto_color_assignments: self.auto_color_assignments.clone(),
            next_auto_color_index: self.next_auto_color_index,
            container_metadata: self.container_metadata.clone(),
            layout_engine: self.layout_engine.clone(),
            dynamic_layout: self.dynamic_layout,
            asset_cache: self.asset_cache.clone(),
            child_orders: self.child_orders.clone(),
            text_compiler: std::cell::RefCell::new(self.text_compiler.borrow().clone()),
            frame_cache: std::cell::RefCell::new(None), // cache is not cloned
            hit_regions: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl Timeline {
    pub fn new() -> Self {
        let mut bg_track = PropertyTrack::new([0.0, 0.0, 0.0, 1.0]);
        bg_track.add_keyframe(0, [0.0, 0.0, 0.0, 1.0], Easing::Linear);
        Self {
            tracks: BTreeMap::new(),
            background_color: bg_track,
            root_nodes: Vec::new(),
            anon_counter: 0,
            env: Environment::new(),
            modifiers: Vec::new(),
            modifier_programs: Vec::new(),
            colorscheme: BuiltInColorscheme::DefaultDark.resolved(),
            external_colorschemes: std::collections::HashMap::new(),
            auto_color_assignments: BTreeMap::new(),
            next_auto_color_index: 0,
            container_metadata: BTreeMap::new(),
            layout_engine: LayoutEngine,
            dynamic_layout: false,
            asset_cache: assets::AssetCache::new(),
            child_orders: BTreeMap::new(),
            text_compiler: std::cell::RefCell::new(crate::renderer::text::TextCompiler::new()),
            frame_cache: std::cell::RefCell::new(None),
            hit_regions: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Duration of the authored animation in seconds, derived from the latest
    /// keyframe across all tracks, background, and child order animations.
    pub fn duration_seconds(&self) -> f64 {
        let max_track_ms = self
            .tracks
            .values()
            .filter_map(|track| track.max_keyframe_time())
            .max()
            .unwrap_or(0);
        let max_bg_ms = self.background_color.last_keyframe_time().unwrap_or(0);
        let max_order_ms = self
            .child_orders
            .values()
            .filter_map(|track| track.last_keyframe_time())
            .max()
            .unwrap_or(0);
        let max_ms = max_track_ms.max(max_bg_ms).max(max_order_ms);
        (max_ms as f64) / 1000.0
    }

    /// Returns all keyframe time positions across all tracks, in seconds.
    /// Used by the GUI timeline scrubber to show keyframe markers.
    pub fn keyframe_times_s(&self) -> Vec<f64> {
        let mut times_ms = Vec::new();
        for track in self.tracks.values() {
            // Geometry
            extend_track_times(&mut times_ms, &track.position);
            extend_track_times(&mut times_ms, &track.motion_offset);
            extend_track_times(&mut times_ms, &track.rotation);
            extend_track_times(&mut times_ms, &track.scale);
            extend_track_times(&mut times_ms, &track.placement_mode);
            extend_track_times(&mut times_ms, &track.position_binding);
            extend_track_times(&mut times_ms, &track.size);
            // Layout
            if let Some(ls) = track.layout_size.as_ref() {
                times_ms.extend(ls.keyframes.keys().copied());
            }
            // Style
            extend_track_times(&mut times_ms, &track.color);
            extend_track_times(&mut times_ms, &track.opacity);
            extend_track_times(&mut times_ms, &track.stroke_width);
            extend_track_times(&mut times_ms, &track.stroke_color);
            extend_track_times(&mut times_ms, &track.stroke_progress);
            extend_track_times(&mut times_ms, &track.fill_opacity);
            extend_track_times(&mut times_ms, &track.morph_options);
            // Text
            extend_track_times(&mut times_ms, &track.text_content);
            extend_track_times(&mut times_ms, &track.font_family);
            extend_track_times(&mut times_ms, &track.font_size);
            if let Some(tp) = track.text_paths.as_ref() {
                times_ms.extend(tp.keyframes.keys().copied());
            }
            // Vector paths
            if let Some(vp) = track.vector_paths.as_ref() {
                times_ms.extend(vp.keyframes.keys().copied());
            }
            // Shape-specific
            extend_track_times(&mut times_ms, &track.shape_type);
            extend_track_times(&mut times_ms, &track.line_from);
            extend_track_times(&mut times_ms, &track.line_to);
            extend_track_times(&mut times_ms, &track.arc_angles);
            extend_track_times(&mut times_ms, &track.points);
            // Image
            if let Some(im) = track.image.as_ref() {
                times_ms.extend(im.keyframes.keys().copied());
            }
        }
        times_ms.sort_unstable();
        times_ms.dedup();
        times_ms.into_iter().map(|ms| ms as f64 / 1000.0).collect()
    }

    /// Returns true if an actor with the given label exists.
    pub fn has_actor(&self, label: &str) -> bool {
        self.tracks.contains_key(label)
    }

    /// Returns an iterator over all track labels.
    pub fn actor_labels(&self) -> impl Iterator<Item = &String> {
        self.tracks.keys()
    }

    /// Finds the common parent container of two child labels.
    /// Returns `None` if no shared parent exists.
    pub fn find_common_parent(&self,
        child_a: &str,
        child_b: &str,
    ) -> Option<String> {
        for (label, track) in &self.tracks {
            if track.children.contains(&child_a.to_string())
                && track.children.contains(&child_b.to_string())
            {
                return Some(label.clone());
            }
        }
        None
    }

    /// Returns the effective child order for a container at the given time.
    /// Falls back to the container's authored `layout_children` order.
    pub fn get_child_order(&self,
        container_label: &str,
        time_ms: u64,
    ) -> Vec<String> {
        if let Some(track) = self.child_orders.get(container_label) {
            let value = track.evaluate(time_ms);
            if !value.is_empty() {
                return value;
            }
        }
        self.container_metadata
            .get(container_label)
            .map(|m| m.layout_children.iter().map(|c| c.label.clone()).collect())
            .unwrap_or_default()
    }

    /// Computes layout positions for a container using a specific child order.
    fn compute_layout_positions_for_order(
        &self,
        metadata: &ContainerMetadata,
        order: &[String],
        time_ms: u64,
    ) -> std::collections::BTreeMap<String, [f32; 2]> {
        use crate::timeline::layout::ChildExtent;

        let child_extents: Vec<ChildExtent> = order
            .iter()
            .filter_map(|label| {
                let track = self.tracks.get(label)?;
                Some(ChildExtent {
                    label: label.clone(),
                    half_size: track.layout_size_get(time_ms)?,
                    placement_mode: track.placement_mode.get(time_ms, PlacementMode::LayoutManaged),
                })
            })
            .collect();

        let positions = LayoutEngine::compute_positions(metadata, &child_extents);

        let mut result = std::collections::BTreeMap::new();
        for (i, child) in child_extents.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                result.insert(child.label.clone(), positions[i]);
            }
        }
        result
    }

    /// Computes layout positions with animated child-order transitions.
    /// If a `child_orders` transition is active, interpolates positions between
    /// the old and new orders. Otherwise delegates to static layout.
    pub fn compute_animated_layout(
        &self,
        container_label: &str,
        time_ms: u64,
    ) -> std::collections::BTreeMap<String, [f32; 2]> {
        let Some(metadata) = self.container_metadata.get(container_label) else {
            return std::collections::BTreeMap::new();
        };

        // Check for child_orders track
        if let Some(track) = self.child_orders.get(container_label) {
            let prev = track.keyframes.range(..=time_ms).next_back();
            let next = track.keyframes.range(time_ms..).next();

            match (prev, next) {
                (Some((&t1, (order1, easing1))), Some((&t2, (order2, _)))) if t1 != t2 => {
                    // Between two keyframes: interpolate
                    let t = ((time_ms - t1) as f32 / (t2 - t1) as f32).clamp(0.0, 1.0);
                    let eased_t = apply_easing(t, *easing1);

                    let pos1 = self.compute_layout_positions_for_order(metadata, order1, time_ms);
                    let pos2 = self.compute_layout_positions_for_order(metadata, order2, time_ms);

                    let mut result = std::collections::BTreeMap::new();
                    for label in order1.iter().chain(order2.iter()) {
                        let p1 = pos1.get(label).copied().unwrap_or([0.0, 0.0]);
                        let p2 = pos2.get(label).copied().unwrap_or([0.0, 0.0]);
                        result.insert(label.clone(), p1.interpolate(&p2, eased_t));
                    }
                    return result;
                }
                (Some((_, (order, _))), _) => {
                    // At or after a keyframe (or at the only keyframe): use it directly
                    return self.compute_layout_positions_for_order(metadata, order, time_ms);
                }
                (None, Some((&t, (order, easing)))) => {
                    // Before the first keyframe: interpolate from default_value to first keyframe
                    let t = (time_ms as f32 / t as f32).clamp(0.0, 1.0);
                    let eased_t = apply_easing(t, *easing);

                    let pos1 = self.compute_layout_positions_for_order(
                        metadata,
                        &track.default_value,
                        time_ms,
                    );
                    let pos2 = self.compute_layout_positions_for_order(metadata, order, time_ms);

                    let mut result = std::collections::BTreeMap::new();
                    for label in track.default_value.iter().chain(order.iter()) {
                        let p1 = pos1.get(label).copied().unwrap_or([0.0, 0.0]);
                        let p2 = pos2.get(label).copied().unwrap_or([0.0, 0.0]);
                        result.insert(label.clone(), p1.interpolate(&p2, eased_t));
                    }
                    return result;
                }
                (None, None) => {
                    // Empty track — should not happen, but fall through
                }
            }
        }

        // No child_orders track — delegate to static layout
        self.layout_engine.compute_layout_for_time(
            metadata,
            time_ms,
            &self.tracks,
        )
    }

    /// Returns the list of root actor labels (actors with no parent).
    pub fn root_actor_labels(&self) -> &[String] {
        &self.root_nodes
    }

    /// Returns the hit regions from the last evaluate call.
    /// Each entry is (actor_label, world_bounds) in scene coordinates.
    pub fn hit_regions(&self) -> Vec<(String, kurbo::Rect)> {
        self.hit_regions.borrow().clone()
    }

    /// Returns a reference to the track for the given label, if it exists.
    pub fn get_track(&self, label: &str) -> Option<&AnimationTrack> {
        self.tracks.get(label)
    }

    /// Invalidate the frame evaluation cache.
    ///
    /// Call this after mutating track data (e.g. adding keyframes or changing
    /// default values) so the next `evaluate()` produces a fresh scene instead
    /// of returning a stale cached one.
    pub fn invalidate_frame_cache(&self) {
        *self.frame_cache.borrow_mut() = None;
    }

    /// Returns the appropriate default color for a primitive type and property,
    /// based on the current colorscheme.
    pub fn get_default_color(&self, primitive_type: &str, property: &str) -> Option<[f32; 4]> {
        self.colorscheme.default_color_for_primitive(primitive_type, property)
    }

    /// Compute the world-space affine transform for a given actor at the given time.
    ///
    /// Walks the scene-graph from the root to the target actor, accumulating
    /// position (resolved through `resolve_bound_position`), rotation, scale,
    /// and motion offset — exactly matching the renderer's transform chain.
    pub fn actor_world_affine(
        &self,
        label: &str,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
    ) -> Option<kurbo::Affine> {
        let path = self.find_path_to_actor(label)?;

        let mut parent_transform = kurbo::Affine::IDENTITY;
        let mut current_layout_positions: BTreeMap<String, [f32; 2]> = BTreeMap::new();

        for node_label in &path {
            let track = self.tracks.get(node_label)?;

            let placement_mode = track.placement_mode.get(time_ms, PlacementMode::LayoutManaged);
            let mut base_position = track.position.get(time_ms, [0.0, 0.0]);

            if self.dynamic_layout {
                if let Some(layout_pos) = current_layout_positions.get(node_label.as_str()) {
                    if placement_mode == PlacementMode::LayoutManaged {
                        base_position = *layout_pos;
                    }
                }
            }

            let binding = track.position_binding.get(time_ms, PositionBinding::Absolute);
            let position =
                resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
            let motion_offset = track.motion_offset.get(time_ms, [0.0, 0.0]);
            let rotation = track.rotation.get(time_ms, 0.0) as f64;
            let scale = track.scale.get(time_ms, 1.0) as f64;

            parent_transform = parent_transform
                * kurbo::Affine::translate((
                    position[0] as f64 + motion_offset[0] as f64,
                    position[1] as f64 + motion_offset[1] as f64,
                ))
                * kurbo::Affine::rotate(rotation)
                * kurbo::Affine::scale(scale);

            // Compute layout positions for this node's children (needed for next iteration)
            if self.dynamic_layout {
                if let Some(metadata) = self.container_metadata.get(node_label.as_str()) {
                    current_layout_positions = self.layout_engine.compute_layout_for_time(
                        metadata,
                        time_ms,
                        &self.tracks,
                    );
                } else {
                    current_layout_positions = BTreeMap::new();
                }
            }
        }

        Some(parent_transform)
    }

    /// Find the path of actor labels from a root node down to `target`.
    fn find_path_to_actor(&self, target: &str) -> Option<Vec<String>> {
        for root in &self.root_nodes {
            if let Some(path) = self.find_path_from(root, target) {
                return Some(path);
            }
        }
        None
    }

    fn find_path_from(&self, current: &str, target: &str) -> Option<Vec<String>> {
        if current == target {
            return Some(vec![current.to_string()]);
        }
        if let Some(track) = self.tracks.get(current) {
            for child in &track.children {
                if let Some(mut path) = self.find_path_from(child, target) {
                    path.insert(0, current.to_string());
                    return Some(path);
                }
            }
        }
        None
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinaryOp, Property};

    #[test]
    fn test_for_iter_values_supports_tuple_literals() {
        let env = Environment::new();
        let values = for_iter_values(
            &Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]),
            &env,
        );

        assert_eq!(
            values,
            vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]
        );
    }

    #[test]
    fn test_apply_modifier_stmt_supports_conditionals_statelessly() {
        let mut timeline = Timeline::new();
        load_standard_library(&mut timeline.env);

        let modifier = Stmt::Conditional {
            condition: Expr::Binary(
                Box::new(Expr::Ident("t".to_string())),
                BinaryOp::Lt,
                Box::new(Expr::Num(1.0)),
            ),
            then_branch: vec![Stmt::Assignment {
                target: vec!["pulse".to_string()],
                property: "opacity".to_string(),
                value: Expr::Num(1.0),
                modifiers: vec![],
                value_span: None,
                span: None,
            }],
            else_branch: Some(vec![Stmt::Assignment {
                target: vec!["pulse".to_string()],
                property: "opacity".to_string(),
                value: Expr::Num(0.0),
                modifiers: vec![],
                value_span: None,
                span: None,
            }]),
            span: None,
        };

        let mut first_overrides = std::collections::HashMap::new();
        let mut first_env =
            timeline.frame_eval_env(500, SceneDimensions::default(), &first_overrides);
        timeline.apply_modifier_stmt(
            &modifier,
            500,
            SceneDimensions::default(),
            &mut first_env,
            &mut first_overrides,
        );

        let mut second_overrides = std::collections::HashMap::new();
        let mut second_env =
            timeline.frame_eval_env(1500, SceneDimensions::default(), &second_overrides);
        timeline.apply_modifier_stmt(
            &modifier,
            1500,
            SceneDimensions::default(),
            &mut second_env,
            &mut second_overrides,
        );

        let mut repeat_overrides = std::collections::HashMap::new();
        let mut repeat_env =
            timeline.frame_eval_env(500, SceneDimensions::default(), &repeat_overrides);
        timeline.apply_modifier_stmt(
            &modifier,
            500,
            SceneDimensions::default(),
            &mut repeat_env,
            &mut repeat_overrides,
        );

        assert_eq!(first_overrides["pulse"]["opacity"], Value::Num(1.0));
        assert_eq!(second_overrides["pulse"]["opacity"], Value::Num(0.0));
        assert_eq!(first_overrides, repeat_overrides);
    }

    #[test]
    fn test_colorscheme_primitive_declaration() {
        let ast = vec![
            Stmt::LetDecl { is_pub: false,
                name: "test-scheme".to_string(),
                value: Expr::Construct(
                    "Colorscheme".to_string(),
                    vec![
                        Property {
                            name: "scene.background".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.1),
                                Expr::Num(0.2),
                                Expr::Num(0.3),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "text.primary".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.9),
                                Expr::Num(0.95),
                                Expr::Num(1.0),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                ),
                span: None,
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("test-scheme".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let timeline = report.output;

        assert_eq!(timeline.colorscheme.name, "test-scheme");
        assert_eq!(
            timeline.colorscheme.color("scene.background"),
            Some([0.1, 0.2, 0.3, 1.0])
        );
        assert_eq!(
            timeline.colorscheme.color("text.primary"),
            Some([0.9, 0.95, 1.0, 1.0])
        );
    }

    #[test]
    fn test_colorscheme_let_declaration() {
        let ast = vec![
            Stmt::LetDecl { is_pub: false,
                name: "test-scheme-let".to_string(),
                value: Expr::Construct(
                    "Colorscheme".to_string(),
                    vec![
                        Property {
                            name: "scene.background".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.15),
                                Expr::Num(0.25),
                                Expr::Num(0.35),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "text.primary".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.85),
                                Expr::Num(0.9),
                                Expr::Num(0.95),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                ),
                span: None,
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("test-scheme-let".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let timeline = report.output;

        assert_eq!(timeline.colorscheme.name, "test-scheme-let");
        assert_eq!(
            timeline.colorscheme.color("scene.background"),
            Some([0.15, 0.25, 0.35, 1.0])
        );
        assert_eq!(
            timeline.colorscheme.color("text.primary"),
            Some([0.85, 0.9, 0.95, 1.0])
        );
    }

    #[test]
    fn test_colorscheme_inheritance() {
        let ast = vec![
            Stmt::LetDecl { is_pub: false,
                name: "child".to_string(),
                value: Expr::Construct(
                    "Colorscheme".to_string(),
                    vec![
                        Property {
                            name: "extends".to_string(),
                            value: Expr::Str("default-dark".to_string()),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "scene.background".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.5),
                                Expr::Num(0.5),
                                Expr::Num(0.5),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                ),
                span: None,
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("child".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let timeline = report.output;

        assert_eq!(timeline.colorscheme.name, "child");
        assert_eq!(
            timeline.colorscheme.color("scene.background"),
            Some([0.5, 0.5, 0.5, 1.0])
        );
        assert_eq!(
            timeline.colorscheme.color("text.primary"),
            Some([1.0, 1.0, 1.0, 1.0])
        );
    }

    #[test]
    fn test_colorscheme_auto_cycle() {
        let ast = vec![
            Stmt::LetDecl { is_pub: false,
                name: "auto-test".to_string(),
                value: Expr::Construct(
                    "Colorscheme".to_string(),
                    vec![
                        Property {
                            name: "auto".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Tuple(vec![
                                    Expr::Num(1.0),
                                    Expr::Num(0.0),
                                    Expr::Num(0.0),
                                ]),
                                Expr::Tuple(vec![
                                    Expr::Num(0.0),
                                    Expr::Num(1.0),
                                    Expr::Num(0.0),
                                ]),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                ),
                span: None,
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("auto-test".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "a".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "b".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let mut timeline = report.output;

        let color_a = timeline.auto_color_for_label("a");
        let color_b = timeline.auto_color_for_label("b");

        assert_eq!(color_a, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(color_b, Some([0.0, 1.0, 0.0, 1.0]));
    }

    #[test]
    fn test_runtime_text_recompilation() {
        let ast = vec![
            Stmt::Config {
                settings: vec![
                    Property {
                        name: "colorscheme".to_string(),
                        value: Expr::Str("editorial-dark".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                span: None,
            },
            Stmt::Keyframe {
                time: crate::ast::Time::Seconds(0.0),
                body: vec![
                    Stmt::ActorDecl {
                        is_pub: false,
                        label: "counter".to_string(),
                        ty: "Text".to_string(),
                        props: vec![
                            Property {
                                name: "text".to_string(),
                                value: Expr::Str("0.00".to_string()),
                                value_span: None,
                                trailing_comment: None,
                            },
                            Property {
                                name: "font_size".to_string(),
                                value: Expr::Num(48.0),
                                value_span: None,
                                trailing_comment: None,
                            },
                            Property {
                                name: "font_family".to_string(),
                                value: Expr::Str("Open Sans".to_string()),
                                value_span: None,
                                trailing_comment: None,
                            },
                            Property {
                                name: "color".to_string(),
                                value: Expr::Tuple(vec![
                                    Expr::Num(1.0),
                                    Expr::Num(1.0),
                                    Expr::Num(1.0),
                                    Expr::Num(1.0),
                                ]),
                                value_span: None,
                                trailing_comment: None,
                            },
                        ],
                        modifiers: vec![],
                        children: vec![],
                        span: None,
                    },
                    Stmt::Always {
                        body: vec![Stmt::Assignment {
                            target: vec!["counter".to_string()],
                            property: "text".to_string(),
                            value: Expr::Call(
                                "format".to_string(),
                                vec![Expr::Str("t={}".to_string()), Expr::Ident("t".to_string())],
                            ),
                            modifiers: vec![],
                            value_span: None,
                            span: None,
                        }],
                        span: None,
                    },
                ],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let timeline = report.output;

        // Evaluate at t=0s and t=1.5s
        let _scene_0s = timeline.evaluate(0.0, SceneDimensions { width: 400, height: 200 });
        let _scene_1_5s = timeline.evaluate(1.5, SceneDimensions { width: 400, height: 200 });

        // The text compiler should have cached entries for both times
        let cache_len = timeline.text_compiler.borrow().cache_len();
        assert!(
            cache_len >= 2,
            "TextCompiler should have at least 2 cache entries for different times, got {}",
            cache_len
        );
    }
}
