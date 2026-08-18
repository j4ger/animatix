//! # Timeline Architecture
//!
//! `Timeline` is the compiled animation package: a scene graph (parent→children hierarchy)
//! with keyframed property tracks per actor. The scene graph drives transform/opacity
//! inheritance via DFS; tracks store animated values over time.
//!
//! ## Build-time vs frame-time boundary
//!
//! - **`Timeline::build()`** (in `build.rs`): one-time lowering pass that parses the AST, resolves
//!   imports, expands components, creates tracks, applies layout, compiles text/math/code paths,
//!   and loads assets.
//! - **`Timeline::evaluate()`** (in `runtime.rs` / `scene_eval.rs`): per-frame execution that
//!   samples tracks, runs `always` modifiers, resolves anchors/percent positions, and emits a
//!   `vello::Scene`.
//!
//! ## Submodule responsibilities
//!
//! | Module | Role |
//! |--------|------|
//! | `build.rs` | AST lowering into Timeline |
//! | `frame_env.rs` / `scene_eval.rs` | Frame-time evaluation and render-scene assembly |
//! | `property_track.rs`, `animation_track.rs`, `dispatch.rs`, `actor_kind.rs` | Keyframed property tracks, interpolation, and track dispatch |
//! | `layout.rs` | Container placement (Row, Col, Grid, Stack) |
//! | `colorscheme.rs` | Built-in and inline colorscheme resolution |
//! | `morph.rs` | Path morphing between vector shapes |
//! | `plot.rs` | Adaptive sampling for graph plots |
//! | `utils.rs` | Expression evaluation |
//! | `modifier_runtime/` | IR lowering and interpretation for `always` blocks |
//!
//! ## The compile boundary
//!
//! The practical compile target is the post-expansion program after module loading
//! and component expansion—not the raw parser AST.
/// Timeline action processing (hover, click, etc.).
pub mod actions;
mod actor_kind;
/// Asset loading and caching.
#[cfg(feature = "render")]
pub mod assets;
mod assignments;
mod build;
mod builtins;
pub mod callout_geometry;
pub mod colorscheme;
mod declarations_text;
/// Evaluation environment for expressions.
pub mod env;
pub mod eval_shared;
/// Filter backend and CPU image processing.
#[cfg(feature = "render")]
pub mod filter;
/// Image loading utilities.
#[cfg(feature = "render")]
pub mod image;
pub mod kurbo_shapes;
mod layout;
#[cfg(feature = "render")]
mod media;
/// Modifier statement execution (IR interpreter).
pub mod modifier_exec;
pub(crate) mod modifier_runtime;
/// Path morphing between vector shapes.
pub mod morph;
pub mod path_progress;
pub mod plan;
mod plot;
mod position;
pub(crate) mod property_engine;
pub mod property_registry;
pub mod property_track;
pub(crate) mod taffy_layout;
pub(crate) mod value_parser;

// Re-export the generic property read API
pub use dispatch::{
    property_has_keyframe_at, property_has_keyframes, property_keyframe_count,
    property_keyframe_easing, property_keyframe_times, read_property_value,
    read_property_value_or_default,
};
pub use plan::{DynTrack, PropertyKind, PropertyPlan, PropertySlot};
pub use property_engine::{PropertyValue, read_property_plan_slot, write_property_plan_slot};
// Re-export property registry types for the GUI
pub use property_registry::{
    ActorField, LEGEND_SUM_VARIANTS, PROPERTY_REGISTRY, PropertyFlags, PropertySchema, SumVariant,
    ValueType, allowed_property_indices, lookup_property, property_id, property_name,
    property_schema_by_id,
};
/// Frame evaluation environment construction and modifier execution.
///
/// Provides [`Timeline::build_frame_env`] which assembles the per-frame
/// variable environment (`t`, `scene_width`, track properties, overrides)
/// that drives both rendering and modifier evaluation.
mod frame_env;
mod index;
/// Legend track storage.
pub mod legend;
pub(crate) mod lookup;
mod primitive;
#[cfg(feature = "render")]
mod scene_eval;
#[cfg(feature = "render")]
pub mod scene_program;
mod sequence;
pub use legend::{LegendMode, LegendTracks};

/// Vector shape definitions and rendering.
pub mod shapes;
/// SVG parsing and manipulation utilities.
#[cfg(feature = "svg")]
pub mod svg;
#[cfg(feature = "svg")]
pub mod svg_import;
mod timing;
pub use timing::parse_easing_name;
/// Keyframed property tracks, tier sub-structs, and helpers.
pub mod animation_track;
/// Field dispatch and track access for property animation.
pub mod dispatch;
/// Scene persistence: CarryBag, CarryEntry, snapshot helpers.
pub mod persistence;
pub mod utils;
/// Vello path wrapper with fill/stroke.
pub mod vello_path;

use actions::process_action_with_extensions;
pub use actor_kind::ActorKind;
pub(crate) use assignments::recompile_text_at_assignment;
pub use builtins::load_standard_library;
use colorscheme::{BuiltInColorscheme, ResolvedColorscheme};
pub use env::{CapturedEnv, Environment, EvalError, Value};
#[cfg(feature = "render")]
pub use image::load_image;
pub use index::TimelineIndex;
pub use kurbo_shapes::{KurboShape, morph_kurbo_shapes, morph_kurbo_shapes_default};
use lookup::{
    assignment_target_key, best_path_suggestion, evaluate_expr_with_lookup_diagnostic,
    for_iter_values, parse_color_in_env_with_lookup_diagnostic, set_lookup_color, set_lookup_vec2,
};
pub(crate) use lookup::{
    evaluate_expr_with_lookup_diagnostic as lookup_evaluate_expr_with_lookup_diagnostic,
    parse_numeric_vec2_with_lookup_diagnostic as lookup_parse_numeric_vec2_with_lookup_diagnostic,
};
pub use morph::{
    MorphOptions, MorphStrategy, evaluate_paths_with_options, interpolate_text_paths,
    interpolate_vello_paths,
};
use plot::{sample_recursive_cartesian, sample_recursive_parametric, sample_recursive_polar};
pub(crate) use position::preserve_instant_delayed_value;
pub use position::scene_anchor_point;
use position::{
    apply_explicit_position_binding, mark_track_manual_position,
    preserve_discrete_position_state_before, resolve_bound_position,
    resolve_position_binding_with_lookup_diagnostic, set_track_position_binding,
};
pub(crate) use primitive::PrimitiveDescriptor;
pub use shapes::{
    ShapeType, VectorShapeState, VectorShapeStyle, apply_vector_shape_defaults,
    apply_vector_shape_property, build_shape_vello_path, build_vector_shape_vello_path,
    default_stroke_width, extract_shape_state_values, finalize_vector_shape_state,
    parse_path_commands_expr, shape_type_for_actor, vector_shape_uses_custom_path,
};
#[cfg(feature = "svg")]
pub use svg::parse_svg;
#[cfg(feature = "svg")]
pub use svg_import::{SvgImportError, import_svg};
pub(crate) use timing::{
    ModifierHost, ParsedTimingModifiers, config_string_value, parse_duration_literal,
    parse_timing_modifiers,
};

use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};

// ─────────────────────────────────────────────────────────────
// Build quality levels (Phase 6.3)
// ─────────────────────────────────────────────────────────────

/// Controls plot sampling fidelity during timeline build.
///
/// - `Draft` — fastest, used during live GUI editing.
/// - `Preview` — balanced, used when paused or scrubbing.
/// - `Production` — maximum fidelity, used for export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BuildQuality {
    /// Fastest quality for live GUI editing.
    #[default]
    Draft,
    /// Balanced quality for paused/scrubbing preview.
    Preview,
    /// Maximum fidelity for export.
    Production,
}

impl BuildQuality {
    /// Apply quality scaling to plot sampling parameters.
    pub fn scale_plot_params(
        &self,
        tolerance: &mut f64,
        max_depth: &mut usize,
        resolution: &mut usize,
    ) {
        match self {
            BuildQuality::Draft => {
                *tolerance = (*tolerance * 4.0).max(0.5);
                *max_depth = max_depth.saturating_sub(4).max(6);
                *resolution = (*resolution / 4).max(16);
            },
            BuildQuality::Preview => {
                *tolerance = (*tolerance * 2.0).max(0.5);
                *max_depth = max_depth.saturating_sub(2).max(6);
                *resolution = (*resolution / 2).max(16);
            },
            BuildQuality::Production => {
                // No scaling — use values as-is (already clamped by caller)
            },
        }
    }
}
pub use actor_kind::{
    ActorCategory, ActorKindId, ActorKindMeta, ShapeKind, actor_kind_meta, actor_kind_meta_by_name,
    actor_kind_registry,
};
pub use animation_track::{
    ActionCategory, ActionEvent, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, PlacementMode,
    PositionBinding, ResizeMode, SceneAnchor,
};
pub use dispatch::{AnimationTrack, TrackFieldMut, TrackFieldRef};
pub use property_track::{Easing, Interpolate, PropertyTrack, TrackAccessor};
use timing::{
    has_non_default_morph_options, parse_stagger_interval_ms, push_modifier_diagnostic,
    push_unknown_target_path_diagnostic, push_unsupported_stagger_statement_diagnostic,
    sequence_stmt_kind,
};
/// Collect all keyframe times (in seconds) across all property tracks of an
/// `AnimationTrack`, using the property registry to discover all possible fields.
/// Used by the GUI to show keyframe markers on the mini timeline and time lens.
pub fn collect_all_keyframe_times(track: &AnimationTrack) -> Vec<f64> {
    let indices = property_registry::allowed_property_indices(track.kind);
    let mut times = std::collections::BTreeSet::new();

    for &idx in &indices {
        let schema = &property_registry::PROPERTY_REGISTRY[idx];
        for t in property_keyframe_times(track, schema.field) {
            times.insert(t);
        }
    }

    times.into_iter().map(|ms| ms as f64 / 1000.0).collect()
}

use std::collections::{BTreeMap, BTreeSet};

pub use utils::{evaluate_expr, parse_color, parse_color_in_env, resolve_color_in_env, time_to_ms};
pub use vello_path::VelloPath;

use crate::ast::{Expr, Modifier, Stmt};
use crate::easing::*;
use crate::timeline::modifier_runtime::ir::ModifierIrProgram;

/// Layout strategy for container actors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutType {
    /// Horizontal left-to-right flow.
    Row,
    /// Vertical top-to-bottom flow.
    Col,
    /// CSS-like grid with configurable columns.
    Grid,
    /// Absolute/manual positioning.
    Stack,
}

impl LayoutType {
    /// Parse a container type string into a `LayoutType`.
    ///
    /// Defaults to `Row` for unrecognized names.
    pub fn from_container_ty(container_ty: &str) -> Self {
        match container_ty {
            "Row" => Self::Row,
            "Col" => Self::Col,
            "Grid" => Self::Grid,
            "Stack" => Self::Stack,
            _ => Self::Row,
        }
    }

    /// Return the string representation of this layout type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Col => "Col",
            Self::Grid => "Grid",
            Self::Stack => "Stack",
        }
    }
}

/// A child actor admitted into a container's layout system.
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

/// Metadata describing the layout configuration of a container actor.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerMetadata {
    /// Row, Col, Grid, or Stack.
    pub layout_type: LayoutType,
    /// Gap between children in logical pixels.
    /// `[0]` = main-axis gap, `[1]` = cross-axis gap.
    pub gap: [f32; 2],
    /// Padding inside the container bounds in logical pixels.
    /// `[left, top, right, bottom]`.
    pub padding: [f32; 4],
    /// Cross-axis alignment string (e.g. "center", "start").
    pub align: String,
    /// Vertical alignment string ("center", "baseline", "top", "bottom").
    /// Default is "center" for backward compatibility.
    pub vertical_align: String,
    /// Number of columns when `layout_type` is `Grid`.
    pub cols: Option<usize>,
    /// Raw authored child order snapshot for this container.
    ///
    /// This is retained for debugging/tests and for preserving the distinction
    /// between authored scene-graph membership and admitted layout membership.
    /// Layout-admitted children are derived from this on demand via
    /// `Timeline::layout_children_for` or `ContainerMetadata::layout_children`.
    pub child_order: Vec<String>,
}

/// Create a uniform `[f32; 2]` gap array where both axes use the same value.
pub fn gap_uniform(v: f32) -> [f32; 2] {
    [v, v]
}

/// Create a uniform `[f32; 4]` padding array where all sides use the same value.
pub fn padding_uniform(v: f32) -> [f32; 4] {
    [v, v, v, v]
}

impl ContainerMetadata {
    /// Compute layout-admitted children on demand from `child_order` + tracks.
    pub fn layout_children(
        &self,
        tracks: &BTreeMap<String, AnimationTrack>,
    ) -> Vec<ContainerLayoutChild> {
        self.child_order
            .iter()
            .filter_map(|child_label| {
                let track = tracks.get(child_label)?;
                if !track.has_layout_size() {
                    return None;
                }
                Some(ContainerLayoutChild {
                    label: child_label.clone(),
                    placement_mode: track
                        .geometry
                        .placement_mode
                        .last(PlacementMode::LayoutManaged),
                })
            })
            .collect()
    }
}

/// Layout engine for computing child positions inside containers.
///
/// Caches layout results per-container to avoid redundant Taffy computation
/// when children's layout sizes haven't changed between frames.
#[derive(Clone, Debug)]
pub struct LayoutEngine {
    /// Per-container layout cache keyed by all layout inputs.
    pub(crate) cache: std::cell::RefCell<
        std::collections::HashMap<
            crate::timeline::layout::LayoutCacheKey,
            crate::timeline::layout::LayoutCacheEntry,
        >,
    >,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Width and height of the output scene in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneDimensions {
    /// Scene width in pixels.
    pub width: u32,
    /// Scene height in pixels.
    pub height: u32,
}

/// Optional debug overlays for the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct DebugRenderOptions {
    /// Draw bounding-box outlines around actors.
    pub draw_bounds: bool,
    /// P2.24: When true, compute hit regions during evaluation.
    /// The GUI sets this to true when it needs click-to-select data.
    pub compute_hit_regions: bool,
    /// Show layout-specific debug info (container labels, slot outlines, sizes).
    pub draw_layout_debug: bool,
    /// Show padding and gap regions as semi-transparent overlays.
    pub draw_spacing: bool,
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
    /// Path or identifier of the audio asset.
    pub source: String,
    /// Start time within the global timeline in seconds.
    pub start_time_s: f64,
    /// Playback duration in seconds.
    pub duration_s: Option<f64>,
    /// Playback volume multiplier (1.0 = full volume).
    pub volume: f32,
}

/// A piecewise-constant variable track defined by `let` declarations in keyframes.
/// Evaluates to the value of the most recent keyframe at or before the query time.
#[derive(Clone, Debug, Default)]
pub struct VariableTrack {
    /// Map from time in milliseconds to the variable value at that keyframe.
    pub keyframes: BTreeMap<u64, Value>,
}

impl VariableTrack {
    /// Evaluate the variable at the given time, returning the most recent keyframe value.
    pub fn evaluate(&self, time_ms: u64) -> Option<Value> {
        self.keyframes.range(..=time_ms).next_back().map(|(_, v)| v.clone())
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

impl<T: Interpolate> HasDuration for PropertyTrack<T> {
    fn max_keyframe_time_ms(&self) -> u64 {
        self.last_keyframe_time().unwrap_or(0)
    }
}

impl HasDuration for VariableTrack {
    fn max_keyframe_time_ms(&self) -> u64 {
        self.keyframes.keys().next_back().copied().unwrap_or(0)
    }
}

/// Cached transform entry: (time_ms, parent_transform_coeffs, node_transform).
type TransformCacheEntry = (u64, [f64; 6], scene_eval::NodeTransform);

/// Static subtree cache value: cached scene, precise bounds, and observed items.
///
/// Items are only collected when the program API requested them. Cache entries
/// are therefore keyed by `collect_items` so a later program call cannot reuse a
/// scene-only entry and report empty items.
type StaticSubtreeEntry = (
    vello::Scene,
    Vec<(String, kurbo::Rect)>,
    Vec<crate::timeline::scene_program::SceneItem>,
);

/// Compiled animation package containing the full scene graph, tracks, and
/// evaluation state.
#[derive(Clone)]
pub struct Timeline {
    pub(crate) tracks: BTreeMap<String, AnimationTrack>,
    pub(crate) background_color: PropertyTrack<[f32; 4]>,
    pub(crate) root_nodes: Vec<String>,
    pub(crate) env: Environment,
    /// Runtime primitive registry used by builds that supply extensions.
    pub(crate) primitive_registry: std::sync::Arc<crate::primitives::PrimitiveRegistry>,
    /// Optional extension context used during build.
    extensions: Option<std::sync::Arc<crate::extension_context::ExtensionRegistry>>,
    /// P2.22: Frozen Arc reference to the base environment entries (stdlib +
    /// colorscheme). Avoids copying ~90 entries on every [`Timeline::build_frame_env`].
    env_base: std::sync::Arc<std::collections::HashMap<String, Value>>,
    pub(crate) modifiers: Vec<Stmt>,
    /// Lowered modifier IR programs. Populated during build.
    pub modifier_programs: Vec<ModifierIrProgram>,
    colorscheme: ResolvedColorscheme,
    external_colorschemes: std::collections::HashMap<String, ResolvedColorscheme>,
    pub(crate) export_preset: Option<String>,
    pub(crate) auto_color_assignments: BTreeMap<String, usize>,
    pub(crate) next_auto_color_index: usize,
    pub(crate) container_metadata: BTreeMap<String, ContainerMetadata>,
    pub(crate) layout_engine: LayoutEngine,
    pub(crate) dynamic_layout: bool,
    pub(crate) asset_cache: std::sync::Arc<assets::AssetCache>,
    pub(crate) font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    /// Build quality level used during timeline construction (Phase 6.3).
    /// Affects plot sampling fidelity: Draft for GUI editing, Production for export.
    pub(crate) build_quality: BuildQuality,
    /// Default opacity for first actor declarations without explicit `opacity` property.
    /// Set to 0.0 for pre-keyframe declarations (actors are hidden until entrance action).
    /// Set to 1.0 for declarations inside keyframes (actors are visible immediately).
    pub(crate) default_opacity: f32,
    /// Per-container child order animations.
    /// Key: container label. Value: track of child label orderings.
    pub(crate) child_orders: BTreeMap<String, PropertyTrack<Vec<String>>>,
    /// Persistence flags set by `persist`/`remove` actions.
    /// `true` = actor should be carried into the next scene; `false` = not carried.
    pub(crate) persistence_flags: BTreeMap<String, bool>,
    /// Runtime text compiler with cache. Enables `always` blocks to change
    /// text content / font_family / font_size and have glyphs recompiled on-demand.
    text_compiler: std::cell::RefCell<crate::renderer::text::TextCompiler>,
    /// Per-frame evaluation caches and transient state. Reset on clone.
    eval_caches: EvalCaches,
    /// Keyframe-scoped variable tracks.
    /// Variables declared via `let` inside keyframes are stored here as
    /// piecewise-constant functions of time, injected into the frame environment
    /// during modifier evaluation.
    pub(crate) variable_tracks: BTreeMap<String, VariableTrack>,
    /// Audio segments collected from Audio actor declarations.
    /// These are muxed into the output during video export.
    pub(crate) audio_segments: Vec<AudioSegment>,
    /// Action events collected during build, for GUI timeline visualization.
    pub action_events: Vec<crate::timeline::animation_track::ActionEvent>,
    /// Cache for static plot paths keyed by parameter hash (Phase 6.4).
    /// Survives across rebuilds when the GUI copies it from the old timeline.
    pub plot_path_cache:
        std::collections::HashMap<u64, Vec<crate::timeline::vello_path::VelloPath>>,
    /// Hash of the modifier AST statements collected during build.
    /// Used to skip IR re-lowering when modifiers haven't changed.
    pub modifier_hash: u64,
}

/// Cache entry for frame evaluation results.
#[derive(Clone)]
pub(crate) struct FrameCacheEntry {
    time_ms: u64,
    dimensions: SceneDimensions,
    has_modifiers: bool,
    has_dynamic_layout: bool,
    has_child_orders: bool,
    /// Structured frame program, including the authoritative encoded scene.
    program: crate::timeline::scene_program::SceneProgram,
    /// Whether the cached program was requested with observable item collection.
    collect_items: bool,
}

/// Per-frame evaluation caches and transient state.
///
/// These are deliberately reset (not cloned) when a [`Timeline`] is cloned, so
/// a cloned timeline starts with clean frame state while sharing the compiled
/// scene data. `Clone` therefore returns `Default` rather than copying cache
/// contents.
#[derive(Default)]
struct EvalCaches {
    frame_cache: std::cell::RefCell<Option<FrameCacheEntry>>,
    transform_cache: std::cell::RefCell<std::collections::HashMap<String, TransformCacheEntry>>,
    static_subtree_cache: std::cell::RefCell<
        std::collections::HashMap<
            (String, SceneDimensions, bool, DebugRenderOptions),
            StaticSubtreeEntry,
        >,
    >,
    scene_buffer: std::cell::RefCell<Option<vello::Scene>>,
    hit_regions: std::cell::RefCell<Vec<(String, kurbo::Rect)>>,
    precise_bounds_cache: std::cell::RefCell<std::collections::HashMap<String, kurbo::Rect>>,
    runtime_diagnostics: std::cell::RefCell<Vec<crate::diagnostics::Diagnostic>>,
}

impl Clone for EvalCaches {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Timeline {
    /// Create a new empty timeline with a fresh font context.
    pub fn new() -> Self {
        Self::new_with_font_context(std::sync::Arc::new(crate::renderer::text::FontContext::new()))
    }

    /// Create a new empty timeline with the given font context.
    pub fn new_with_font_context(
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    ) -> Self {
        let mut bg_track = PropertyTrack::new([0.0, 0.0, 0.0, 1.0]);
        bg_track.add_keyframe(0, [0.0, 0.0, 0.0, 1.0], Easing::Linear);
        Self {
            tracks: BTreeMap::new(),
            background_color: bg_track,
            root_nodes: Vec::new(),
            env: Environment::new(),
            primitive_registry: std::sync::Arc::new(crate::primitives::PrimitiveRegistry::new()),
            extensions: None,
            env_base: std::sync::Arc::new(std::collections::HashMap::new()),
            modifiers: Vec::new(),
            modifier_programs: Vec::new(),
            colorscheme: BuiltInColorscheme::DefaultDark.resolved(),
            external_colorschemes: std::collections::HashMap::new(),
            export_preset: None,
            auto_color_assignments: BTreeMap::new(),
            next_auto_color_index: 0,
            container_metadata: BTreeMap::new(),
            layout_engine: LayoutEngine::new(),
            dynamic_layout: false,
            asset_cache: std::sync::Arc::new(assets::AssetCache::new()),
            font_context,
            build_quality: BuildQuality::Production,
            default_opacity: 1.0,
            child_orders: BTreeMap::new(),
            persistence_flags: BTreeMap::new(),
            text_compiler: std::cell::RefCell::new(crate::renderer::text::TextCompiler::new()),
            eval_caches: EvalCaches::default(),
            variable_tracks: BTreeMap::new(),
            audio_segments: Vec::new(),
            action_events: Vec::new(),
            plot_path_cache: std::collections::HashMap::new(),
            modifier_hash: 0,
        }
    }

    /// Runtime primitive registry snapshot used by this build.
    pub fn primitive_registry_snapshot(
        &self,
    ) -> std::sync::Arc<crate::primitives::PrimitiveRegistry> {
        std::sync::Arc::clone(&self.primitive_registry)
    }

    /// External property descriptors installed on this build.
    pub fn extension_property_specs(&self) -> Vec<crate::extension_context::ExtensionPropertySpec> {
        self.extensions
            .as_ref()
            .map(|ctx| ctx.property_specs().to_vec())
            .unwrap_or_default()
    }

    /// Unified descriptors for extension-only properties.
    pub fn extension_property_descriptors(
        &self,
    ) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.extensions
            .as_ref()
            .map(|ctx| ctx.extension_property_descriptors())
            .unwrap_or_default()
    }

    /// Unified property descriptor view for built-in and extension properties.
    ///
    /// This is the tooling-facing migration point: consumers that need one
    /// property table can use this instead of reading built-in and extension
    /// tables separately.
    pub fn property_descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        match self.extensions.as_ref() {
            Some(registry) => registry.property_descriptors(),
            None => crate::extension_context::PropertyRegistry::builtin_descriptors(),
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
            .max(self.child_orders.values().map(|t| t.max_keyframe_time_ms()).max().unwrap_or(0))
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
    ///
    /// Uses the property registry to discover all applicable fields per actor kind,
    /// plus cross-track sources (background_color, child_orders, variable_tracks)
    /// and dynamic plot parameter tracks that the registry cannot enumerate statically.
    pub fn keyframe_times_s(&self) -> Vec<f64> {
        let mut times_ms: BTreeSet<u64> = BTreeSet::new();
        for track in self.tracks.values() {
            // Registry-driven: covers all applicable fields for this actor kind
            for &time_s in collect_all_keyframe_times(track).iter() {
                let ms = (time_s * 1000.0) as u64;
                times_ms.insert(ms);
            }
            // D4 exception: dynamic plot parameter tracks (not statically representable)
            for pt in track.plot_param_tracks.values() {
                times_ms.extend(pt.keyframes.keys().copied());
            }
            // Tagged union tracks are registry-representable, but include them
            // directly as a defensive catch for properties without a schema.
            for pt in track.tagged_tracks.values().flatten() {
                times_ms.extend(pt.keyframes.keys().copied());
            }
        }
        // Cross-track sources
        times_ms.extend(self.background_color.keyframes.keys().copied());
        for t in self.child_orders.values() {
            times_ms.extend(t.keyframes.keys().copied());
        }
        for t in self.variable_tracks.values() {
            times_ms.extend(t.keyframes.keys().copied());
        }
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
    pub fn find_common_parent(&self, child_a: &str, child_b: &str) -> Option<String> {
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
    pub fn get_child_order(&self, container_label: &str, time_ms: u64) -> Vec<String> {
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
                    placement_mode: track
                        .geometry
                        .placement_mode
                        .last(PlacementMode::LayoutManaged),
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
                    placement_mode: track
                        .geometry
                        .placement_mode
                        .get(time_ms, PlacementMode::LayoutManaged),
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
                },
                (Some((_, (order, _))), _) => {
                    // At or after a keyframe (or at the only keyframe): use it directly
                    return self.compute_layout_positions_for_order(metadata, order, time_ms);
                },
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
                },
                (None, None) => {
                    // Empty track — should not happen, but fall through
                },
            }
        }

        // No child_orders track — delegate to static layout
        let layout_children = self.layout_children_for(container_label);
        self.layout_engine.compute_layout_for_time(
            container_label,
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
        self.eval_caches.hit_regions.borrow().clone()
    }

    /// Returns a reference to the track for the given label, if it exists.
    pub fn get_track(&self, label: &str) -> Option<&AnimationTrack> {
        self.tracks.get(label)
    }

    /// Returns a mutable reference to the track for the given label, if it exists.
    ///
    /// Invalidates the frame cache before returning, because any mutation to
    /// the track can change the rendered scene at the cached time.
    pub fn get_track_mut(&mut self, label: &str) -> Option<&mut AnimationTrack> {
        self.invalidate_frame_cache();
        self.tracks.get_mut(label)
    }

    /// Check if a keyframe exists for the given actor and property at `time_ms`.
    pub fn has_keyframe_at(&self, actor: &str, property: &str, time_ms: u64) -> bool {
        self.tracks
            .get(actor)
            .map(|t| t.has_keyframe_at(property, time_ms))
            .unwrap_or(false)
    }

    /// List all keyframe times (in ms) for the given actor and property.
    pub fn list_keyframes(&self, actor: &str, property: &str) -> Vec<u64> {
        self.tracks.get(actor).map(|t| t.list_keyframes(property)).unwrap_or_default()
    }

    /// Evaluate the background color at the given time.
    pub fn background_color_at(&self, time_ms: u64) -> [f32; 4] {
        self.background_color.evaluate(time_ms)
    }

    /// Returns a reference to all tracks.
    pub fn tracks(&self) -> &BTreeMap<String, AnimationTrack> {
        &self.tracks
    }

    /// Returns a mutable reference to all tracks.
    ///
    /// Invalidates the frame cache before returning, because the caller can
    /// mutate any track through this map.
    pub fn tracks_mut(&mut self) -> &mut BTreeMap<String, AnimationTrack> {
        self.invalidate_frame_cache();
        &mut self.tracks
    }

    /// Returns a reference to the container metadata map.
    pub fn container_metadata(&self) -> &BTreeMap<String, ContainerMetadata> {
        &self.container_metadata
    }

    /// Returns a mutable reference to the container metadata map.
    ///
    /// Invalidates the frame cache before returning, because layout metadata
    /// changes affect rendered positions.
    pub fn container_metadata_mut(&mut self) -> &mut BTreeMap<String, ContainerMetadata> {
        self.invalidate_frame_cache();
        &mut self.container_metadata
    }

    /// Returns a reference to the asset cache.
    pub fn asset_cache(&self) -> &assets::AssetCache {
        &self.asset_cache
    }

    /// Returns a cloneable handle to the asset cache for carrying into rebuilds.
    pub fn asset_cache_arc(&self) -> std::sync::Arc<assets::AssetCache> {
        self.asset_cache.clone()
    }

    /// Iterate over asset path → actor labels that reference it.
    pub fn asset_usage(&self) -> impl Iterator<Item = (&String, &BTreeSet<String>)> {
        self.asset_cache.asset_usage()
    }

    /// Returns a reference to the environment.
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Returns a mutable reference to the environment.
    ///
    /// Invalidates the frame cache before returning, because environment
    /// changes can affect modifier/plot evaluation on the next frame.
    pub fn env_mut(&mut self) -> &mut Environment {
        self.invalidate_frame_cache();
        &mut self.env
    }

    /// Returns true if any actor has a procedural plot that requires per-frame
    /// evaluation.  Static plots (no `t` reference, no animated params, no func
    /// transitions) always use their cached build-time paths and do not force a
    /// frame environment to be constructed.
    pub(crate) fn has_procedural_plots(&self) -> bool {
        self.tracks.values().any(|t| {
            t.procedural_plot.as_ref().is_some_and(|pp| pp.is_dynamic())
                || !t.func_transitions.is_empty()
        })
    }

    /// Returns true if frame environment is needed for evaluation.
    /// Frame environment is needed when modifiers or procedural plots exist.
    pub(crate) fn needs_frame_env(&self) -> bool {
        !self.modifiers.is_empty()
            || !self.modifier_programs.is_empty()
            || self.has_procedural_plots()
    }

    /// Returns true if the actor and all its descendants have no keyframes
    /// and the timeline has no modifiers that could affect them.
    /// Fully-static subtrees can have their rendered output cached (P2.17).
    pub(crate) fn is_static_subtree(&self, label: &str) -> bool {
        // Conservative: if any modifiers exist, we can't safely cache because
        // modifiers might change actor properties at frame time. Child-order
        // animations live outside `AnimationTrack` keyframe detection and can
        // change layout output, so they also disable this cache.
        if self.needs_frame_env() || !self.child_orders.is_empty() {
            return false;
        }
        let Some(track) = self.tracks.get(label) else {
            return true;
        };
        if track.has_any_keyframes() || track.procedural_plot.is_some() {
            return false;
        }
        track.children.iter().all(|child| self.is_static_subtree(child))
    }

    /// Invalidate the frame evaluation cache.
    ///
    /// Call this after mutating track data (e.g. adding keyframes or changing
    /// default values) so the next `evaluate()` produces a fresh scene instead
    /// of returning a stale cached one. Public mutable track/metadata/env
    /// accessors invoke this automatically.
    pub fn invalidate_frame_cache(&self) {
        *self.eval_caches.frame_cache.borrow_mut() = None;
        *self.eval_caches.static_subtree_cache.borrow_mut() = std::collections::HashMap::new();
        *self.eval_caches.transform_cache.borrow_mut() = std::collections::HashMap::new();
        *self.eval_caches.precise_bounds_cache.borrow_mut() = std::collections::HashMap::new();
        self.layout_engine.invalidate_cache();
    }

    /// Returns a reference to the audio segments collected during build.
    pub fn audio_segments(&self) -> &[AudioSegment] {
        &self.audio_segments
    }

    /// Return runtime diagnostics produced during the most recent frame
    /// evaluation (modifier errors, etc.). Empty if evaluation succeeded
    /// without issues.
    pub fn runtime_diagnostics(&self) -> Vec<crate::diagnostics::Diagnostic> {
        self.eval_caches.runtime_diagnostics.borrow().clone()
    }

    /// Clear runtime diagnostics. Called automatically at the start of each
    /// `evaluate()` call.
    pub fn clear_runtime_diagnostics(&self) {
        self.eval_caches.runtime_diagnostics.borrow_mut().clear();
    }

    /// Returns the name of the currently active colorscheme.
    pub fn colorscheme_name(&self) -> &str {
        &self.colorscheme.name
    }

    /// Returns the configured named export preset, if any.
    pub fn export_preset(&self) -> Option<&str> {
        self.export_preset.as_deref()
    }

    /// Returns the appropriate default color for a primitive type and property,
    /// based on the current colorscheme.
    pub fn get_default_color(
        &self,
        primitive: &dyn crate::primitives::Primitive,
        property: &str,
    ) -> Option<[f32; 4]> {
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

            let placement_mode =
                track.geometry.placement_mode.get(time_ms, PlacementMode::LayoutManaged);
            let mut base_position = track.geometry.position.get(time_ms, [0.0, 0.0]);

            if self.dynamic_layout {
                if let Some(layout_pos) = current_layout_positions.get(node_label.as_str()) {
                    if placement_mode == PlacementMode::LayoutManaged {
                        base_position = *layout_pos;
                    }
                }
            }

            let binding = track.geometry.position_binding.get(time_ms, PositionBinding::Absolute);
            let position =
                resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
            let motion_offset = track.geometry.motion_offset.get(time_ms, [0.0, 0.0]);
            let rotation = track.geometry.rotation.get(time_ms, 0.0) as f64;
            let scale = track.geometry.scale.get(time_ms, 1.0) as f64;

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
                        node_label.as_str(),
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
