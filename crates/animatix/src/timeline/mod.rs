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
mod builtins;
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
pub mod property_registry;
pub(crate) mod value_parser;

// Re-export the generic property read API
pub use property_engine::{
    PropertyValue, read_property_value, read_property_value_or_default,
    property_has_keyframes, property_has_keyframe_at,
    property_keyframe_count, property_keyframe_times,
    property_keyframe_easing,
};

// Re-export property registry types for the GUI
pub use property_registry::{
    PropertySchema, ValueType, ActorField, PropertyFlags,
    lookup_property, allowed_property_indices, PROPERTY_REGISTRY,
};
mod primitive;
pub(crate) mod property_lookup;
mod runtime;
mod index;
mod scene_eval;
mod sequence;
pub mod shapes;
pub mod svg;
pub mod svg_import;
mod timing;
pub use timing::parse_easing_name;
pub mod track;
pub mod utils;
pub mod vello_path;

use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
pub use actor_kind::ActorKind;
use actions::process_action;
use colorscheme::{BuiltInColorscheme, ResolvedColorscheme};
pub use env::{Environment, EvalError, Value};
pub use builtins::load_standard_library;
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
    preserve_discrete_position_state_before,
    resolve_bound_position, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding,
};
pub use position::scene_anchor_point;
pub(crate) use assignments::recompile_text_at_assignment;
pub(crate) use position::preserve_instant_delayed_value;
pub(crate) use primitive::PrimitiveDescriptor;
use property_lookup::{
    assignment_target_key, best_path_suggestion, evaluate_expr_with_lookup_diagnostic,
    for_iter_values, parse_color_in_env_with_lookup_diagnostic,
    set_lookup_color, set_lookup_vec2,
};
pub(crate) use property_lookup::{
    evaluate_expr_with_lookup_diagnostic as lookup_evaluate_expr_with_lookup_diagnostic,
    parse_numeric_vec2_with_lookup_diagnostic as lookup_parse_numeric_vec2_with_lookup_diagnostic,
};
pub use shapes::{
    VectorShapeState, VectorShapeStyle, apply_vector_shape_defaults,
    apply_vector_shape_property, build_shape_vello_path, build_vector_shape_vello_path,
    extract_shape_state_values, finalize_vector_shape_state, parse_path_commands_expr, shape_type_for_actor,
    vector_shape_uses_custom_path, ShapeType,
};
pub use svg::parse_svg;
pub use svg_import::{import_svg, SvgImportError};
pub(crate) use timing::{ModifierHost, ParsedTimingModifiers, parse_timing_modifiers};
use timing::{
    config_string_value, has_non_default_morph_options, parse_stagger_interval_ms,
    push_modifier_diagnostic, push_unknown_target_path_diagnostic,
    push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind,
};
pub use track::{
    ActorCategory, ActorKindId, ActorKindMeta, ShapeKind, ResizeMode, ActorHeader, GeometryTier, StyleTier, ActorPayload,
    AnimationTrack, Interpolate, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor,
    TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE,
    actor_kind_registry, actor_kind_meta, actor_kind_meta_by_name,
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
    /// Cached placement mode from build time to avoid repeated track lookups.
    pub placement_mode: PlacementMode,
}

#[derive(Clone, Debug)]
pub struct ContainerMetadata {
    pub layout_type: LayoutType,
    pub gap: f32,
    pub padding: f32,
    pub align: String,
    pub cols: Option<usize>,
    /// Raw authored child order snapshot for this container.
    ///
    /// This is retained for debugging/tests and for preserving the distinction
    /// between authored scene-graph membership and admitted layout membership.
    /// Layout-admitted children are derived from this on demand via
    /// `Timeline::layout_children_for` or `ContainerMetadata::layout_children`.
    pub child_order: Vec<String>,
}

impl ContainerMetadata {
    /// Compute layout-admitted children on demand from `child_order` + tracks.
    pub fn layout_children(&self, tracks: &BTreeMap<String, AnimationTrack>) -> Vec<ContainerLayoutChild> {
        self.child_order
            .iter()
            .filter_map(|child_label| {
                let track = tracks.get(child_label)?;
                if !track.has_layout_size() {
                    return None;
                }
                Some(ContainerLayoutChild {
                    label: child_label.clone(),
                    placement_mode: track.placement_mode.last(PlacementMode::LayoutManaged),
                })
            })
            .collect()
    }
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

/// A single audio segment to be muxed during export.
#[derive(Clone, Debug)]
pub struct AudioSegment {
    pub source: String,
    pub start_time_s: f64,
    pub duration_s: f64,
    pub volume: f32,
}

/// A piecewise-constant variable track defined by `let` declarations in keyframes.
/// Evaluates to the value of the most recent keyframe at or before the query time.
#[derive(Clone, Debug, Default)]
pub struct VariableTrack {
    pub keyframes: BTreeMap<u64, Value>,
}

impl VariableTrack {
    pub fn new() -> Self {
        Self {
            keyframes: BTreeMap::new(),
        }
    }

    /// Evaluate the variable at the given time, returning the most recent keyframe value.
    pub fn evaluate(&self, time_ms: u64) -> Option<Value> {
        self.keyframes
            .range(..=time_ms)
            .next_back()
            .map(|(_, v)| v.clone())
    }
}

/// Trait for types that have a notion of their maximum keyframe time.
pub trait HasDuration {
    /// Returns the maximum keyframe time in milliseconds.
    fn max_keyframe_time_ms(&self) -> u64;
}

impl HasDuration for AnimationTrack {
    fn max_keyframe_time_ms(&self) -> u64 {
        self.max_keyframe_time().unwrap_or(0)
    }
}

impl<T: Interpolate + Clone> HasDuration for PropertyTrack<T> {
    fn max_keyframe_time_ms(&self) -> u64 {
        self.last_keyframe_time().unwrap_or(0)
    }
}

impl HasDuration for VariableTrack {
    fn max_keyframe_time_ms(&self) -> u64 {
        self.keyframes.keys().next_back().copied().unwrap_or(0)
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
    pub font_context: crate::renderer::text::FontContext,
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
    /// Keyframe-scoped variable tracks.
    /// Variables declared via `let` inside keyframes are stored here as
    /// piecewise-constant functions of time, injected into the frame environment
    /// during modifier evaluation.
    pub variable_tracks: BTreeMap<String, VariableTrack>,
    /// Audio segments collected from Audio actor declarations.
    /// These are muxed into the output during video export.
    pub audio_segments: Vec<AudioSegment>,
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
            font_context: self.font_context.clone(),
            child_orders: self.child_orders.clone(),
            text_compiler: std::cell::RefCell::new(self.text_compiler.borrow().clone()),
            frame_cache: std::cell::RefCell::new(None), // cache is not cloned
            hit_regions: std::cell::RefCell::new(Vec::new()),
            variable_tracks: self.variable_tracks.clone(),
            audio_segments: self.audio_segments.clone(),
        }
    }
}

impl Timeline {
    pub fn new() -> Self {
        Self::new_with_font_context(crate::renderer::text::FontContext::new())
    }

    pub fn new_with_font_context(font_context: crate::renderer::text::FontContext) -> Self {
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
            font_context,
            child_orders: BTreeMap::new(),
            text_compiler: std::cell::RefCell::new(crate::renderer::text::TextCompiler::new()),
            frame_cache: std::cell::RefCell::new(None),
            hit_regions: std::cell::RefCell::new(Vec::new()),
            variable_tracks: BTreeMap::new(),
            audio_segments: Vec::new(),
        }
    }

    /// Duration of the authored animation in seconds, derived from the latest
    /// keyframe across all tracks, background, and child order animations.
    pub fn duration_seconds(&self) -> f64 {
        let max_ms = self
            .tracks
            .values()
            .map(|t| t.max_keyframe_time_ms())
            .max()
            .unwrap_or(0)
            .max(self.background_color.max_keyframe_time_ms())
            .max(
                self.child_orders
                    .values()
                    .map(|t| t.max_keyframe_time_ms())
                    .max()
                    .unwrap_or(0),
            )
            .max(
                self.variable_tracks
                    .values()
                    .map(|t| t.max_keyframe_time_ms())
                    .max()
                    .unwrap_or(0),
            );
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
    /// Falls back to the container's authored `child_order`.
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
            .map(|m| m.child_order.clone())
            .unwrap_or_default()
    }

    /// Compute layout-admitted children for a container on demand.
    /// Filters `child_order` by `has_layout_size()` and captures placement mode.
    pub fn layout_children_for(&self, container_label: &str) -> Vec<ContainerLayoutChild> {
        let Some(metadata) = self.container_metadata.get(container_label) else {
            return Vec::new();
        };
        metadata
            .child_order
            .iter()
            .filter_map(|child_label| {
                let track = self.tracks.get(child_label)?;
                if !track.has_layout_size() {
                    return None;
                }
                Some(ContainerLayoutChild {
                    label: child_label.clone(),
                    placement_mode: track.placement_mode.last(PlacementMode::LayoutManaged),
                })
            })
            .collect()
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
        let layout_children = self.layout_children_for(container_label);
        self.layout_engine.compute_layout_for_time(
            metadata,
            &layout_children,
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
    pub fn get_default_color(&self, primitive: &dyn crate::primitives::Primitive, property: &str) -> Option<[f32; 4]> {
        self.colorscheme.default_color_for_primitive(primitive, property)
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
                    let layout_children = self.layout_children_for(node_label.as_str());
                    current_layout_positions = self.layout_engine.compute_layout_for_time(
                        metadata,
                        &layout_children,
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
mod tests;
