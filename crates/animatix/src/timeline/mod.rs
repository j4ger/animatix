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
//! | `property_track.rs`, `animation_track/`, `dispatch.rs`, `actor_kind.rs` | Keyframed property tracks, interpolation, and track dispatch |
//! | `layout.rs` | Container placement (Row, Col, Grid, Stack) |
//! | `colorscheme.rs` | Built-in and inline colorscheme resolution |
//! | `morph.rs` | Path morphing between vector shapes |
//! | `plot/` | Adaptive sampling for graph plots |
//! | `utils.rs` | Expression evaluation |
//! | `modifier_runtime/` | IR lowering and interpretation for `always` blocks |
//!
//! ## The compile boundary
//!
//! The practical compile target is the post-expansion program after module loading
//! and component expansion—not the raw parser AST.
/// Timeline action processing (hover, click, etc.).
pub mod actions;
pub mod actor_caps;
/// Asset loading and caching.
pub mod assets;
mod assignments;
mod build;
mod builtins;
pub mod callout_geometry;
pub(crate) mod camera;
pub mod colorscheme;
pub mod components;
pub use components::{
    AutoColorComponent, ChildOrderComponent, CoordinateContext, CoordinateFrameComponent,
    CoordinateFrameTarget, PersistenceComponent, TranscriptComponent,
};
mod declarations_text;
/// Built-in post-processing effects, chain storage, and the compositing
/// backend boundary.
pub mod effects;
pub mod entity;
pub use entity::{EntityId, EntityStore};
/// Public timeline data types (build quality, layout metadata, scene
/// dimensions, audio segments, variable tracks).
mod types;
pub use types::{
    AudioSegment, BuildQuality, ContainerLayoutChild, ContainerMetadata, DebugRenderOptions,
    LayoutEngine, LayoutType, SceneDimensions, VariableTrack, gap_uniform, padding_uniform,
};
/// The ECS world: entities, component stores, and entity views.
pub mod world;
pub use world::{ComponentStore, EntityMut, EntityRef, RequiredStore, World};
/// Evaluation environment for expressions.
pub mod env;
pub(crate) mod env_keys;
pub mod eval_shared;
pub(crate) mod fn_eval;
/// The key a presented frame is identified by, and the guard that skips a
/// raster when the previous one is still on screen.
pub mod frame_signature;
/// Image loading utilities.
pub mod image;
pub mod inline_keyframes;
pub mod kurbo_shapes;
pub(crate) mod layout;
mod media;
/// Modifier statement execution (IR interpreter).
pub mod modifier_exec;
pub(crate) mod modifier_runtime;
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
// Re-export property registry types for the GUI
pub use property_registry::{
    ActorField, LEGEND_SUM_VARIANTS, PROPERTY_REGISTRY, PropertyFlags, PropertySchema, SumVariant,
    ValueType, allowed_property_indices, lookup_property, property_id, property_name,
    property_schema_by_id,
};
/// Plot-host coordinate frame access surface.
mod coordinate_frame;
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
pub use actor_caps::ActorKind;
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

// ─────────────────────────────────────────────────────────────
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
/// Collect all keyframe times (in seconds) across all property tracks of an
/// entity, using the property registry to discover all possible fields.
/// Used by the GUI to show keyframe markers on the mini timeline and time lens.
pub fn collect_all_keyframe_times(
    id: EntityId,
    effects: Option<&crate::timeline::effects::EffectChainTrack>,
    world: &crate::timeline::World,
) -> Vec<f64> {
    let caps = world.caps(id);
    let actor_type = world.actor_type(id);
    let indices = property_registry::allowed_property_indices(&caps, actor_type);
    let mut times = std::collections::BTreeSet::new();

    for &idx in &indices {
        let schema = &property_registry::PROPERTY_REGISTRY[idx];
        for t in world.property_keyframe_times(id, schema.field) {
            times.insert(t);
        }
    }

    // Effect chain parameters live outside the property registry.
    if let Some(effects) = effects {
        for t in effects.keyframe_times() {
            times.insert(t);
        }
    }

    times.into_iter().map(|ms| ms as f64 / 1000.0).collect()
}

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
/// Trait for types that have a notion of their maximum keyframe time.
pub trait HasDuration {
    /// Returns the maximum keyframe time in milliseconds.
    fn max_keyframe_time_ms(&self) -> u64;
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

/// Static subtree cache value: cached scene, precise bounds, and observed items.
///
/// Items are only collected when the program API requested them. Cache entries
/// are therefore keyed by `collect_items` so a later program call cannot reuse a
/// scene-only entry and report empty items.
type StaticSubtreeEntry = (
    vello::Scene,
    // Bounds captured for this subtree as `(bounds_slot, rect)` pairs.
    Vec<(u32, kurbo::Rect)>,
    Vec<crate::timeline::scene_program::SceneItem>,
    // Hit regions captured for this subtree (only populated when the cache
    // key's `compute_hit_regions` was set — the GUI always sets it).
    Vec<(EntityId, kurbo::Rect)>,
);

/// Compiled animation package containing the full scene graph, tracks, and
/// evaluation state.
#[derive(Clone)]
pub struct Timeline {
    /// The ECS world: dense entity storage plus every component store.
    ///
    /// Mutating entity data after the first evaluation must invalidate the
    /// frame cache, so post-build code goes through [`Self::entity_mut`],
    /// [`Self::world_mut`] or [`Self::tracks_mut`] (all invalidate). Direct
    /// access to the inner store is for the build phase, which runs before any
    /// cached frame exists.
    pub(crate) world: World,
    pub(crate) background_color: PropertyTrack<[f32; 4]>,
    pub(crate) env: Environment,
    /// Scope stack for `Stmt::Block` (function-call expansion). Each entry
    /// records the `let` names bound inside that block so they can be removed
    /// on exit; bindings inside a block stay local and never leak into the
    /// scene, and never write scene variable tracks.
    pub(crate) block_scope: Vec<std::collections::HashSet<String>>,
    /// Runtime primitive registry used by builds that supply extensions.
    pub(crate) primitive_registry: std::sync::Arc<crate::primitives::PrimitiveRegistry>,
    /// Optional extension context used during build.
    extensions: Option<std::sync::Arc<crate::extension_context::ExtensionRegistry>>,
    /// Whether any actor here is an effect scope (`Filter`, `Mask`, and any
    /// primitive whose caps say so).
    ///
    /// A frame-cache entry replays a built vello scene, which cannot carry the
    /// filter backend's pending-composite side channel — so replaying is only
    /// safe while nothing in the scene would have consulted the backend at all,
    /// and that is exactly this flag. Without it the cache has to bail whenever a
    /// backend is merely *present*, and the web runtime passes one on every
    /// frame, which is how browsers ended up re-evaluating from scratch.
    pub(crate) has_effect_scopes: bool,
    /// P2.22: Frozen Arc reference to the base environment entries (stdlib +
    /// colorscheme). Avoids copying ~90 entries on every [`Timeline::build_frame_env`].
    env_base: std::sync::Arc<std::collections::HashMap<String, Value>>,
    /// PF-6: pooled frame environment handed back by the scene-eval path after
    /// each frame (see `build_frame_env_internal`). Retaining the environment
    /// keeps the overrides map's table capacity and every key `String`
    /// allocation alive across frames, so a steady-state evaluate loop builds
    /// its environment allocation-free. `invalidate_frame_cache` drops it —
    /// every timeline mutation funnels through that method, which restores
    /// fresh-build semantics whenever the injection key set may have changed.
    env_pool: std::cell::RefCell<Option<crate::timeline::Environment>>,
    pub(crate) modifiers: Vec<Stmt>,
    /// Bare names any `always` block writes (`freq = ...`, `let freq = ...`).
    /// Computed once at the end of the build from `modifiers`; a plot closure
    /// capturing one of these must resample per frame so the frame value
    /// shadows its build-time capture (spec §14 "Runtime parameters").
    pub(crate) frame_written_vars: std::collections::HashSet<String>,
    /// Lowered modifier IR programs. Populated during build.
    pub modifier_programs: Vec<ModifierIrProgram>,
    /// Actor targets written by modifier programs (e.g. `"circle0"` or array prefix `"ring"`).
    pub(crate) modifier_written_targets: std::collections::HashSet<String>,
    colorscheme: ResolvedColorscheme,
    external_colorschemes: std::collections::HashMap<String, ResolvedColorscheme>,
    pub(crate) export_preset: Option<String>,
    /// Scene resolution declared via top-level `config { resolution: (w, h) }`,
    /// so export tooling can default its canvas to the authored size.
    pub(crate) resolution: Option<(u32, u32)>,
    /// Duration declared via `config { duration: N }`, in seconds. When present
    /// it *overrides* the keyframe-inferred length rather than extending it, so
    /// the timeline ends there even if keyframes continue past it — the
    /// contract `docs/spec.md` documents and the composition path has always
    /// honoured per scene. See [`Timeline::playback_duration_seconds`].
    pub(crate) declared_duration_s: Option<f64>,
    /// Next auto-color slot handed out to a `color: auto` declaration.
    ///
    /// A scene-level resource (not a component): it is an allocation counter,
    /// not per-entity state. The assigned slot itself lives in the entity's
    /// [`AutoColorComponent`].
    pub(crate) next_auto_color_index: usize,
    /// Memoized layout-admitted children per container (see
    /// `Timeline::layout_children_for`). Cleared by `invalidate_frame_cache`
    /// alongside the layout engine cache, whose admission contract it shares.
    pub(crate) layout_children_cache:
        std::cell::RefCell<std::collections::HashMap<String, Arc<Vec<ContainerLayoutChild>>>>,
    pub(crate) layout_engine: LayoutEngine,
    pub(crate) dynamic_layout: bool,
    /// Declared via `config { seamless_loop: true }`: the scene is replayed, so
    /// the build checks that its keyframed values wrap at the seam.
    pub(crate) seamless_loop: bool,
    /// Tempo declared via `config { bpm: N }`, the length of one beat in
    /// milliseconds. `None` falls back to the default tempo.
    pub(crate) beat_ms: Option<f64>,
    /// Names supplied on the command line via `--set NAME=VALUE`. They seed the
    /// build environment before the walk and are re-applied after it, so a
    /// template's own top-level `let` acts as a default rather than a blocker.
    pub(crate) cli_defines: std::collections::HashSet<String>,
    pub(crate) asset_cache: std::sync::Arc<assets::AssetCache>,
    pub(crate) font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    /// Build quality level used during timeline construction (Phase 6.3).
    /// Affects plot sampling fidelity: Draft for GUI editing, Production for export.
    pub(crate) build_quality: BuildQuality,
    /// Default opacity for first actor declarations without explicit `opacity` property.
    /// Set to 0.0 for pre-keyframe declarations (actors are hidden until entrance action).
    /// Set to 1.0 for declarations inside keyframes (actors are visible immediately).
    pub(crate) default_opacity: f32,
    /// Whether `persist camera` marked the scene camera to be carried.
    ///
    /// The camera is not an entity and owns no track, so its persistence flag
    /// is a scene resource rather than a [`PersistenceComponent`]; every other
    /// target stores the flag on its entity.
    pub(crate) camera_persistent: bool,
    /// Interactive pause points in milliseconds marked by `#step` or `pause`.
    pub pause_points: Vec<u64>,
    /// Runtime text compiler with cache. Enables `always` blocks to change
    /// text content / font_family / font_size and have glyphs recompiled on-demand.
    text_compiler: std::cell::RefCell<crate::renderer::text::TextCompiler>,
    /// Per-frame evaluation caches and transient state. Reset on clone.
    pub(crate) eval_caches: EvalCaches,
    /// True once any track carries a `blend:` value. The frame path checks
    /// this before its per-node blend lookup, so scenes that never author a
    /// blend pay one bool read instead of a track probe per node.
    pub(crate) blend_used: std::cell::Cell<bool>,
    /// The scene camera — the transform applied to every root node. Written by
    /// `camera.at` / `camera.zoom` / `camera.rotation` assignments.
    pub(crate) camera: camera::Camera,
    /// True once the scene addresses the camera from a keyframe **or** a
    /// modifier block. Gates the per-frame camera sampling and forces the
    /// static-subtree encoding cache off (a cached encoding cannot be
    /// re-transformed when it is appended).
    pub(crate) camera_used: std::cell::Cell<bool>,
    /// Whether **any** actor declares `camera_follow: false`. The root loop
    /// asks this before it looks anything up, so the common scene pays nothing
    /// per frame for the opt-out existing. A scene that pins a non-root actor
    /// finds nothing here — that declaration is reported at build
    /// (`inapplicable-property`) rather than silently accepted.
    pub(crate) camera_exempt_present: std::cell::Cell<bool>,
    /// Keyframe-scoped variable tracks.
    /// Variables declared via `let` inside keyframes are stored here as
    /// piecewise-constant functions of time, injected into the frame environment
    /// during modifier evaluation.
    pub(crate) variable_tracks: BTreeMap<String, VariableTrack>,
    /// Actor labels referenced by any expression in the built program
    /// (see `build::referenced_roots`). `build_eval_env` injects only these
    /// labels' properties, turning environment construction from
    /// O(declarations²) into O(declarations × referenced). `None` disables
    /// filtering (inject everything) for Timelines built without an AST scan.
    pub(crate) referenced_roots: Option<std::collections::HashSet<String>>,
    /// Whether any expression in the scene references `scene.stats` (STAT-1).
    pub(crate) stats_used: bool,
    /// Whether `is_animating` or a property reference (`&actor.prop`) was used.
    pub(crate) is_animating_used: bool,
    /// Actor labels that appeared as bare identifiers in expressions without a dot accessor.
    pub(crate) wildcard_actors: std::collections::HashSet<String>,
    /// Map from actor label to the specific property names referenced in expressions.
    pub(crate) referenced_properties:
        std::collections::HashMap<String, std::collections::HashSet<String>>,
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
    /// Scene-only cache hits clone `program.scene` directly; the full program
    /// is only cloned back on the observable (collect_items) restore path.
    program: crate::timeline::scene_program::SceneProgram,
    /// Per-frame world-space bounds as `(bounds_slot, rect)` pairs, stored so
    /// a scene-only cache hit can restore `precise_bounds` without re-encoding
    /// the scene. Kept separate from `program.precise_bounds` (which stays an
    /// empty map on the scene-only path); restoring is a flat `Vec` copy.
    precise_bounds: Vec<(u32, kurbo::Rect)>,
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
pub(crate) struct EvalCaches {
    frame_cache: std::cell::RefCell<Option<FrameCacheEntry>>,
    /// Bumped by `invalidate_frame_cache` (every in-place mutation funnels
    /// through it); exported as `Timeline::content_epoch` for frame
    /// signatures.
    content_epoch: std::cell::Cell<u64>,
    transform_cache: std::cell::RefCell<Vec<Option<scene_eval::TransformCacheEntry>>>,
    /// Memoized `is_transform_static` answers (`has_any_keyframes` negated),
    /// cleared by `invalidate_frame_cache`. Indexed by `EntityId.index()`.
    transform_static_flags: std::cell::RefCell<Vec<Option<bool>>>,
    static_subtree_cache: std::cell::RefCell<
        std::collections::HashMap<
            (EntityId, SceneDimensions, bool, DebugRenderOptions),
            StaticSubtreeEntry,
        >,
    >,
    /// Memoized `is_static_subtree` answers, cleared whenever the timeline is
    /// mutated. Indexed by `EntityId.index()`.
    static_subtree_flags: std::cell::RefCell<Vec<Option<bool>>>,
    scene_buffer: std::cell::RefCell<Option<vello::Scene>>,
    /// Per-frame hit regions keyed by entity id. The label view is materialized
    /// only by [`Timeline::hit_regions`] at the GUI boundary.
    hit_regions: std::cell::RefCell<Vec<(EntityId, kurbo::Rect)>>,
    overrides_buffer: std::cell::RefCell<crate::timeline::modifier_runtime::ir::ModifierOverrides>,
    /// Precise world-space bounds for the current frame, indexed by
    /// `World::bounds_slots` slot ids (PF-6: per-node write is a `Vec` store,
    /// stash/restore are flat copies, no string keys).
    precise_bounds: std::cell::RefCell<BoundsTable>,
    /// Label↔slot identity for [`Self::precise_bounds`]: sorted track labels,
    /// rebuilt lazily after invalidation. `None` until first needed.
    bounds_registry: std::cell::RefCell<Option<BoundsRegistry>>,
    pub(crate) runtime_diagnostics: std::cell::RefCell<Vec<crate::diagnostics::Diagnostic>>,
    /// Authored solo gate for the current frame. Empty (and allocation-free)
    /// in the common case where no actor declares `solo: true`.
    pub(crate) solo: std::cell::RefCell<SoloState>,
    /// Whether any actor is soloed this frame; a plain `Cell<bool>` load
    /// keeps the per-node gate free in the common case.
    pub(crate) solo_active: std::cell::Cell<bool>,
    /// Whether resolving the solo gate has to walk the tracks at all.
    ///
    /// `None` means "unknown, derive it" (initial state and what
    /// `invalidate_frame_cache` resets to); the common scene settles on
    /// `Some(false)` after one walk. A timeline with `always` blocks always
    /// reports `true` — an always block can write the flag per frame
    /// without going through a mutation.
    pub(crate) solo_scan_needed: std::cell::Cell<Option<bool>>,
    /// Memoized [`Self::needs_frame_env`] answers (the raw computation walks
    /// every entity's payload; the frame path consulted it up to 3×/frame).
    needs_frame_env: std::cell::Cell<Option<bool>>,
    /// Precomputed frame-env injection plan; see `timeline_impl::cache`.
    env_injection_plan: std::cell::RefCell<Option<std::sync::Arc<timeline_impl::EnvInjectionPlan>>>,
    /// Resolved primitive handles per entity, keyed by the actor type each
    /// was resolved from; see `timeline_impl::primitive_cache`.
    primitive_handles: std::cell::RefCell<Vec<crate::primitives::PrimitiveCacheEntry>>,
    /// The scene's evaluated background color for the current frame, sampled
    /// once and read per node by the primitive `EvaluateCtx` (legend
    /// label-contrast).
    background_color: std::cell::Cell<[f32; 4]>,
}

/// Per-frame resolution of the authored `solo` flags.
///
/// `visible` holds every soloed actor *and its ancestors*, so traversal still
/// reaches a soloed descendant through a container that does not draw itself;
/// `soloed` names the actors that actually draw. Both are `None` when nothing
/// is soloed, which keeps the common path free of set lookups.
#[derive(Default)]
pub(crate) struct SoloState {
    visible: Option<std::collections::HashSet<String>>,
    soloed: Option<std::collections::HashSet<String>>,
}

impl SoloState {
    pub(crate) fn is_active(&self) -> bool {
        self.visible.is_some()
    }

    /// `true` when `label` is a soloed actor itself.
    pub(crate) fn is_soloed(&self, label: &str) -> bool {
        self.soloed.as_ref().is_some_and(|set| set.contains(label))
    }

    /// `true` when `label` is a soloed actor or an ancestor of one.
    pub(crate) fn is_reachable(&self, label: &str) -> bool {
        self.visible.as_ref().is_some_and(|set| set.contains(label))
    }
}

/// Dense per-frame precise-bounds storage (PF-6 slot-id design).
///
/// `slots` is indexed by the `World::bounds_slots` slot id, which `ensure_bounds_registry`
/// stamps once per rebuild (sorted `tracks` order — deterministic and stable for the
/// lifetime of one `Timeline`). `written` lists the slots touched this frame so the
/// per-frame clear, the frame-cache stash, and the restore are all flat
/// `Vec<(u32, kurbo::Rect)>` work with no string keys, hashing, or per-key allocation.
#[derive(Default)]
struct BoundsTable {
    slots: Vec<Option<kurbo::Rect>>,
    written: Vec<u32>,
}

impl BoundsTable {
    /// Record bounds for `slot`, appending to [`Self::written`] on first write.
    fn write(&mut self, slot: u32, rect: kurbo::Rect) {
        let index = slot as usize;
        if index >= self.slots.len() {
            // Only reachable if tracks changed without `invalidate_frame_cache`
            // (every public mutator funnels through it). Log rather than panic.
            tracing::warn!("bounds slot {slot} outside registry; bounds dropped for this frame");
            return;
        }
        if self.slots[index].is_none() {
            self.written.push(slot);
        }
        self.slots[index] = Some(rect);
    }

    /// Clear the current frame's entries. Sparse: touches only written slots.
    fn clear_frame(&mut self) {
        for &slot in &self.written {
            self.slots[slot as usize] = None;
        }
        self.written.clear();
    }

    /// `(slot, bounds)` pairs for every recorded entry, optionally starting
    /// from a `written` offset (used to snapshot a single static subtree).
    /// Pre-sized from `written` so the frame-end stash is one allocation
    /// (the filter-map iterator alone would grow the Vec by doubling).
    fn pairs_from(&self, from: usize) -> Vec<(u32, kurbo::Rect)> {
        let mut pairs = Vec::with_capacity(self.written.len() - from);
        pairs.extend(self.written[from..].iter().filter_map(|&slot| {
            let rect = self.slots[slot as usize]?;
            Some((slot, rect))
        }));
        pairs
    }
}

/// Sorted entity labels giving the `World::bounds_slots` values their
/// meaning; rebuilt whenever `invalidate_frame_cache` clears it.
#[derive(Clone)]
struct BoundsRegistry {
    /// Slot index → label, in `tracks` (BTreeMap) order, so slot numbering is
    /// deterministic. `bounds_slot` lookups by label binary-search this.
    labels: Vec<String>,
}

impl EvalCaches {
    /// Stamp `World::bounds_slots` for every entity and (re)build the
    /// label registry. Cheap no-op while the registry is valid; invalidated by
    /// `invalidate_frame_cache` (the single mutation funnel) and by
    /// `Clone` (EvalCaches reset to `Default`).
    fn ensure_bounds_registry(&self, world: &crate::timeline::World) {
        if self.bounds_registry.borrow().is_some() {
            return;
        }
        let labels: Vec<String> = world.tracks.keys().cloned().collect();
        {
            let mut table = self.precise_bounds.borrow_mut();
            table.clear_frame();
            table.slots.clear();
            table.slots.resize(labels.len(), None);
        }
        for (slot, (_, id)) in world.tracks.iter().enumerate() {
            world.set_bounds_slot(id, slot as u32);
        }
        *self.bounds_registry.borrow_mut() = Some(BoundsRegistry { labels });
    }
}

impl Clone for EvalCaches {
    fn clone(&self) -> Self {
        Self::default()
    }
}

mod timeline_impl;

#[cfg(test)]
mod tests;
