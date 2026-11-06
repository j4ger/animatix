//! Timeline: the compiled animation package — a parent→children scene graph with
//! keyframed property tracks per actor. The graph drives transform/opacity
//! inheritance; tracks store animated values over time.
//!
//! `build/` lowers the AST into a `Timeline` once; `scene_eval/` samples tracks
//! per frame, runs `always` modifiers, resolves anchors, and emits a
//! `vello::Scene`. The compile target is the post-expansion program, not the raw
//! parser AST.
/// Timeline action processing (hover, click, etc.).
pub mod actions;
pub mod actor_caps;
/// Asset loading and caching.
pub mod assets;
mod assignments;
pub mod behavior;
pub use behavior::{BehaviorDef, BehaviorId, BehaviorRegistry};
mod build;
pub use build::{BuildOptions, CarryInput};
mod builtins;
pub mod callout_geometry;
pub(crate) mod camera;
pub mod colorscheme;
pub mod components;
pub use components::{
    AutoColorComponent, ChildOrderComponent, CoordinateContext, CoordinateFrameComponent,
    CoordinateFrameTarget, LocalBoundsComponent, PersistenceComponent, TranscriptComponent,
};
mod declarations_text;
/// Post-processing effects, chain storage, and the compositing backend boundary.
pub mod effects;
pub mod entity;
pub use entity::{EntityId, EntityStore};
/// Public timeline data types: build quality, layout, scene dimensions, audio, variable tracks.
mod types;
pub use types::{
    AudioSegment, BuildQuality, ContainerLayoutChild, ContainerMetadata, DebugRenderOptions,
    HasDuration, LayoutEngine, LayoutType, SceneDimensions, VariableTrack,
    collect_all_keyframe_times, gap_uniform, padding_uniform,
};
/// The ECS world: entities, component stores, and entity views.
pub mod world;
pub use world::{ComponentStore, EntityMut, EntityRef, RequiredStore, World};
/// Evaluation environment for expressions.
pub mod env;
pub(crate) mod env_keys;
pub mod eval_shared;
pub(crate) mod fn_eval;
/// Identifies a presented frame and skips a raster while the previous one is on screen.
pub mod frame_signature;
/// Image loading utilities.
pub mod image;
pub mod inline_keyframes;
pub mod kurbo_shapes;
pub(crate) mod layout;
mod media;
/// Modifier statement execution (IR interpreter).
pub mod modifier_exec;
/// Modifier program runtime: lowered IR types, lowering, and execution.
pub mod modifier_runtime;
/// Path morphing between vector shapes.
pub mod morph;
pub mod path_progress;
pub mod plan;
pub(crate) mod plot;
pub(crate) use build::plot::{BarChartLayout, parse_bar_chart_data_expr};
pub(crate) use plot::BarDataTransition;
mod position;
pub(crate) mod property_engine;
pub mod property_registry;
pub mod property_track;
pub(crate) mod scene_stats;
pub(crate) mod taffy_layout;
pub(crate) mod value_parser;

pub use plan::{DynTrack, PropertyKind, PropertyPlan, PropertySlot};
pub(crate) use property_engine::values_wrap;
pub use property_engine::{
    PropertyValue, read_property_plan_slot, write_property_field, write_property_plan_slot,
};
pub use property_registry::{
    ActorField, LEGEND_SUM_VARIANTS, PROPERTY_REGISTRY, PropertyFlags, PropertySchema, SumVariant,
    ValueType, allowed_property_indices, lookup_property, property_id, property_name,
    property_schema_by_id,
};
/// Plot-host coordinate frame access surface.
mod coordinate_frame;
/// Frame environment construction ([`Timeline::build_frame_env`]) and modifier execution.
mod frame_env;
mod index;
/// Legend track storage.
pub mod legend;
pub(crate) mod lookup;
mod primitive;
mod scene_eval;
pub mod scene_program;
mod sequence;
pub use legend::{LegendMode, LegendTracks};

pub mod path_data;
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
pub(crate) use assignments::recompile_text_at_assignment;
pub use builtins::load_standard_library;
use colorscheme::{BuiltInColorscheme, ResolvedColorscheme};
pub use env::{CapturedEnv, Environment, EvalError, Value};
#[cfg(feature = "image-decode")]
pub use image::load_image;
pub use index::TimelineIndex;
pub use kurbo_shapes::{KurboShape, morph_kurbo_shapes, morph_kurbo_shapes_default};
use lookup::{
    assignment_target_key, best_path_suggestion, evaluate_expr_with_lookup_diagnostic,
    for_iter_values, parse_color_in_env_with_lookup_diagnostic, set_lookup_color_into,
    set_lookup_vec2_into,
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
pub(crate) use primitive::PrimitiveFamilyDescriptor;
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

pub use actor_caps::{ActorCaps, ActorCategory, ShapeKind, TextKind};
pub use animation_track::{
    ActionCategory, ActionEvent, ConnectorEndpoint, ConnectorRouting, DEFAULT_LAYOUT_HALF_SIZE,
    DEFAULT_WHITE, GeometryTracks, PlacementMode, PositionBinding, ResizeMode, SceneAnchor,
    ShapeTracks, StyleTracks, TextTracks,
};
pub use animatix_std::{caps_for_type, caps_from_info};
pub use dispatch::{TrackFieldMut, TrackFieldRef};
pub use property_track::{Easing, Interpolate, PropertyTrack, TrackAccessor};
use timing::{
    ParsedStaggerModifiers, StaggerMetric, StaggerOrigin, has_non_default_morph_options,
    parse_stagger_modifiers, push_modifier_diagnostic, push_unknown_target_path_diagnostic,
    push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind,
};

pub use crate::primitives::primitive_info_by_name;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub use utils::{
    evaluate_expr, parse_color, parse_color_in_env, resolve_color_in_env, time_to_ms,
    time_to_ms_at_beat,
};
pub use vello_path::{GradientExtend, GradientShape, GradientSpace, GradientSpec, VelloPath};

use crate::ast::{Expr, Modifier, Stmt};
use crate::easing::*;
use crate::timeline::modifier_runtime::ir::ModifierIrProgram;

/// Static subtree cache value: scene, precise bounds, observed items, and hit
/// regions. Keyed by `collect_items`, so a scene-only entry is never reused by a
/// program call that must report items.
type StaticSubtreeEntry = (
    vello::Scene,
    // Bounds captured for this subtree as `(bounds_slot, rect)` pairs.
    Vec<(u32, kurbo::Rect)>,
    Vec<crate::timeline::scene_program::SceneItem>,
    // Hit regions; only populated when the cache key requested them.
    Vec<(EntityId, kurbo::Rect)>,
);

/// Compiled animation package: scene graph, tracks, and evaluation state.
#[derive(Clone)]
pub struct Timeline {
    /// The ECS world. Post-build mutation must go through [`Self::entity_mut`] or
    /// [`Self::world_mut`] so the frame cache is invalidated; direct store access
    /// is for the build phase, which runs before any cached frame exists.
    pub(crate) world: World,
    pub(crate) background_color: PropertyTrack<[f32; 4]>,
    pub(crate) env: Environment,
    /// Scope stack for `Stmt::Block` expansion: `let` names bound inside a block,
    /// removed on exit so they never leak into the scene or variable tracks.
    pub(crate) block_scope: Vec<std::collections::HashSet<String>>,
    /// Runtime primitive registry used by builds that supply extensions.
    pub(crate) primitive_registry: std::sync::Arc<crate::primitives::PrimitiveRegistry>,
    /// Dense, O(1) indexable behavior registry for built-ins and extensions.
    pub(crate) behavior_registry: std::sync::Arc<crate::timeline::behavior::BehaviorRegistry>,
    /// Optional extension context used during build.
    extensions: Option<std::sync::Arc<crate::extension_context::ExtensionRegistry>>,
    /// Whether any actor here is an effect scope (`Filter`, `Mask`, or a primitive
    /// whose caps say so). A frame-cache entry replays a built scene, which cannot
    /// carry the backend's pending-composite side channel, so caching is only safe
    /// while nothing would have consulted the backend — exactly this flag.
    pub(crate) has_effect_scopes: bool,
    /// Frozen base environment entries (stdlib + colorscheme); avoids copying ~90 entries per frame.
    env_base: std::sync::Arc<crate::timeline::env::EnvMap>,
    /// Pooled frame environment handed back after each frame, keeping the
    /// overrides map and key `String`s alive so a steady-state loop allocates
    /// nothing. `invalidate_frame_cache` drops it to restore fresh-build semantics.
    env_pool: std::cell::RefCell<Option<crate::timeline::Environment>>,
    pub(crate) modifiers: Vec<Stmt>,
    /// Bare names any `always` block writes. A plot closure capturing one must
    /// resample per frame so the frame value shadows its build-time capture.
    pub(crate) frame_written_vars: std::collections::HashSet<String>,
    /// Lowered modifier IR programs. Populated during build.
    pub modifier_programs: Vec<ModifierIrProgram>,
    /// Actor targets written by modifier programs (e.g. `"circle0"` or array prefix `"ring"`).
    pub(crate) modifier_written_targets: std::collections::HashSet<String>,
    colorscheme: ResolvedColorscheme,
    external_colorschemes: std::collections::HashMap<String, ResolvedColorscheme>,
    pub(crate) export_preset: Option<String>,
    /// `config { resolution: (w, h) }`, so export tooling can default its canvas to the authored size.
    pub(crate) resolution: Option<(u32, u32)>,
    /// `config { duration: N }` in seconds. It *overrides* the keyframe-inferred
    /// length rather than extending it, so the timeline ends there even if
    /// keyframes continue past it — see [`Timeline::playback_duration_seconds`].
    pub(crate) declared_duration_s: Option<f64>,
    /// Next auto-color slot for `color: auto`: a scene-level allocation counter,
    /// not per-entity state (the slot itself lives in [`AutoColorComponent`]).
    pub(crate) next_auto_color_index: usize,
    /// Memoized layout-admitted children per container. Cleared by
    /// `invalidate_frame_cache` with the layout engine cache it shares.
    pub(crate) layout_children_cache:
        std::cell::RefCell<std::collections::HashMap<EntityId, Arc<Vec<ContainerLayoutChild>>>>,
    pub(crate) layout_engine: LayoutEngine,
    pub(crate) dynamic_layout: bool,
    /// `config { seamless_loop: true }`: the build checks that keyframed values wrap at the seam.
    pub(crate) seamless_loop: bool,
    /// `config { bpm: N }` as one beat in milliseconds; `None` uses the default tempo.
    pub(crate) beat_ms: Option<f64>,
    /// `--set NAME=VALUE` names, applied before and after the walk so a template's
    /// own top-level `let` acts as a default rather than a blocker.
    pub(crate) cli_defines: std::collections::HashSet<String>,
    pub(crate) asset_cache: std::sync::Arc<assets::AssetCache>,
    pub(crate) font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    /// Build quality: plot sampling fidelity — Draft for GUI editing, Production for export.
    pub(crate) build_quality: BuildQuality,
    /// Default opacity for first-actor declarations without explicit `opacity`:
    /// 0.0 before the first keyframe (hidden until entrance), 1.0 inside one.
    pub(crate) default_opacity: f32,
    /// `persist camera` flag. The camera is not an entity and owns no track, so
    /// this is a scene resource, not a [`PersistenceComponent`].
    pub(crate) camera_persistent: bool,
    /// Interactive pause points in milliseconds marked by `#step` or `pause`.
    pub pause_points: Vec<u64>,
    /// Runtime text compiler with cache; `always` blocks can change text content, font family, or size.
    text_compiler: std::cell::RefCell<crate::renderer::text::TextCompiler>,
    /// Per-frame evaluation caches and transient state. Reset on clone.
    pub(crate) eval_caches: EvalCaches,
    /// True once any track carries `blend:`. The frame path checks this before
    /// its per-node blend lookup, so non-blend scenes pay one bool read per node.
    pub(crate) blend_used: std::cell::Cell<bool>,
    /// Scene camera transform applied to every root node; written by `camera.*` assignments.
    pub(crate) camera: camera::Camera,
    /// True once the camera is addressed from a keyframe **or** a modifier block.
    /// Gates per-frame camera sampling and forces the static-subtree encoding
    /// cache off (a cached encoding cannot be re-transformed when appended).
    pub(crate) camera_used: std::cell::Cell<bool>,
    /// Whether **any** actor declares `camera_follow: false`, so the root loop can
    /// skip the lookup when none does. Pinning a non-root actor is reported at
    /// build (`inapplicable-property`) rather than silently accepted.
    pub(crate) camera_exempt_present: std::cell::Cell<bool>,
    /// Keyframe-scoped variable tracks: `let` inside keyframes becomes a
    /// piecewise-constant function of time injected into the frame environment.
    pub(crate) variable_tracks: BTreeMap<String, VariableTrack>,
    /// Actor labels referenced by any expression in the built program
    /// (`build::referenced_roots`). Injecting only these turns env construction
    /// from O(declarations²) into O(declarations × referenced); `None` injects
    /// everything (Timelines built without an AST scan).
    pub(crate) referenced_roots: Option<std::collections::HashSet<String>>,
    /// Whether any expression in the scene references `scene.stats`.
    pub(crate) stats_used: bool,
    /// Whether `is_animating` or a property reference (`&actor.prop`) was used.
    pub(crate) is_animating_used: bool,
    /// Actor labels that appeared as bare identifiers in expressions without a dot accessor.
    pub(crate) wildcard_actors: std::collections::HashSet<String>,
    /// Map from actor label to the specific property names referenced in expressions.
    pub(crate) referenced_properties:
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Audio segments from Audio actors, muxed into video export.
    pub(crate) audio_segments: Vec<AudioSegment>,
    /// Action events collected during build, for GUI timeline visualization.
    pub action_events: Vec<crate::timeline::animation_track::ActionEvent>,
    /// Cache for static plot paths keyed by parameter hash.
    /// Survives across rebuilds when the GUI copies it from the old timeline.
    pub plot_path_cache:
        std::collections::HashMap<u64, Vec<crate::timeline::vello_path::VelloPath>>,
    /// Hash of the build-time modifier AST; skips IR re-lowering when modifiers haven't changed.
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
    /// Scene-only cache hits clone `program.scene` directly; the full program is
    /// cloned only on the observable (`collect_items`) restore path.
    program: crate::timeline::scene_program::SceneProgram,
    /// Per-frame world-space bounds as `(bounds_slot, rect)` pairs, so a
    /// scene-only hit can restore `precise_bounds` without re-encoding. Kept
    /// separate from `program.precise_bounds`, which stays empty on that path.
    precise_bounds: Vec<(u32, kurbo::Rect)>,
    /// Whether the cached program was requested with observable item collection.
    collect_items: bool,
}

/// Memoized build-fixed classification for one entity: whether its transform
/// inputs are time-invariant and whether its whole subtree is. Both survive
/// until the next invalidation; one slot table halves the per-frame bookkeeping.
#[derive(Clone, Copy, Default)]
pub(crate) struct EntityStaticFlags {
    /// Memoized `is_transform_static` answer.
    pub(crate) transform: Option<bool>,
    /// Memoized `is_static_subtree_id` answer.
    pub(crate) subtree: Option<bool>,
}

/// Keyed by `(body_pointer_as_usize, kind_or_arity)`.
pub(crate) type FastPlotCache = std::cell::RefCell<
    std::collections::HashMap<
        (usize, usize),
        Option<std::sync::Arc<crate::timeline::plot::FastPlotProgram>>,
    >,
>;

/// Per-frame evaluation caches and transient state.
///
/// These are deliberately reset (not cloned) when a [`Timeline`] is cloned, so
/// a cloned timeline starts with clean frame state while sharing the compiled
/// scene data. `Clone` therefore returns `Default` rather than copying cache
/// contents.
#[derive(Default)]
pub(crate) struct EvalCaches {
    frame_cache: std::cell::RefCell<Option<FrameCacheEntry>>,
    /// Bumped by `invalidate_frame_cache`; exported as `Timeline::content_epoch` for frame signatures.
    content_epoch: std::cell::Cell<u64>,
    transform_cache: std::cell::RefCell<Vec<Option<scene_eval::TransformCacheEntry>>>,
    /// Memoized transform/subtree classification; cleared by `invalidate_frame_cache`.
    static_flags: std::cell::RefCell<Vec<EntityStaticFlags>>,
    static_subtree_cache: std::cell::RefCell<
        std::collections::HashMap<
            (EntityId, SceneDimensions, bool, DebugRenderOptions),
            StaticSubtreeEntry,
        >,
    >,
    scene_buffer: std::cell::RefCell<Option<vello::Scene>>,
    /// Per-frame hit regions by entity id; [`Timeline::hit_regions`] materializes the label view.
    hit_regions: std::cell::RefCell<Vec<(EntityId, kurbo::Rect)>>,
    overrides_buffer: std::cell::RefCell<crate::timeline::modifier_runtime::ir::ModifierOverrides>,
    /// Precise world-space bounds for the current frame, indexed by
    /// `World::bounds_slots` ids (flat `Vec` stores, no string keys).
    precise_bounds: std::cell::RefCell<BoundsTable>,
    /// Label↔slot identity for [`Self::precise_bounds`]; rebuilt lazily after invalidation.
    bounds_registry: std::cell::RefCell<Option<BoundsRegistry>>,
    pub(crate) runtime_diagnostics: std::cell::RefCell<Vec<crate::diagnostics::Diagnostic>>,
    /// Authored solo gate; empty (and allocation-free) when nothing is soloed.
    pub(crate) solo: std::cell::RefCell<SoloState>,
    /// Whether any actor is soloed this frame; a bool load keeps the per-node gate free.
    pub(crate) solo_active: std::cell::Cell<bool>,
    /// Whether resolving the solo gate has to walk tracks at all. `None` means
    /// "unknown, derive it" (initial state, reset by `invalidate_frame_cache`);
    /// the common scene settles on `Some(false)`. A timeline with `always` blocks
    /// always reports `true` — such a block can write the flag per frame.
    pub(crate) solo_scan_needed: std::cell::Cell<Option<bool>>,
    /// Memoized [`Self::needs_frame_env`] answer; cleared by `invalidate_frame_cache`.
    needs_frame_env: std::cell::Cell<Option<bool>>,
    /// Precomputed frame-env injection plan; see `timeline_impl::cache`.
    env_injection_plan: std::cell::RefCell<Option<std::sync::Arc<timeline_impl::EnvInjectionPlan>>>,
    /// Resolved primitive handles per entity; see `timeline_impl::primitive_cache`.
    primitive_handles: std::cell::RefCell<Vec<crate::primitives::PrimitiveCacheEntry>>,
    /// The scene's background color for this frame, sampled once and read by the
    /// primitive `EvaluateCtx` (legend label contrast).
    background_color: std::cell::Cell<[f32; 4]>,
    /// Cache for compiled fast plot expressions across frames.
    pub(crate) fast_plot_programs: FastPlotCache,
    /// Content epoch at which `fast_plot_programs` was last verified/cleared.
    pub(crate) fast_plot_epoch: std::cell::Cell<u64>,
}

/// Per-frame resolution of the authored `solo` flags: `visible` holds soloed
/// actors *and their ancestors* (traversal must reach a soloed descendant
/// through a non-drawing container), `soloed` the actors that draw. Both are
/// id-keyed and `None` when nothing is soloed, keeping the common path free of
/// lookups.
#[derive(Default)]
pub(crate) struct SoloState {
    visible: Option<std::collections::HashSet<EntityId>>,
    soloed: Option<std::collections::HashSet<EntityId>>,
}

/// Dense per-frame precise-bounds storage (slot-id design).
///
/// `slots` is indexed by `World::bounds_slots` ids stamped once per rebuild in
/// sorted `tracks` order; `written` lists this frame's slots so clear, stash, and
/// restore are flat `Vec<(u32, kurbo::Rect)>` work — no string keys or hashing.
#[derive(Default)]
struct BoundsTable {
    slots: Vec<Option<kurbo::Rect>>,
    written: Vec<u32>,
}

/// Sorted entity labels giving `World::bounds_slots` values their meaning; rebuilt on invalidation.
#[derive(Clone)]
struct BoundsRegistry {
    /// Slot index → label in `tracks` (BTreeMap) order, making slot numbering deterministic.
    labels: Vec<String>,
}

mod bounds;
mod caches;
mod solo;
mod timeline_impl;

#[cfg(test)]
mod tests;
