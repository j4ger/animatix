use super::lookup::parse_numeric_vec2_with_lookup_diagnostic;
use super::{
    DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, Diagnostic, Easing, Environment, ModifierHost,
    ParsedTimingModifiers, PositionBinding, ShapeType, Timeline, Value, VectorShapeState,
    VectorShapeStyle, assignment_target_key, best_path_suggestion, build_shape_vello_path,
    build_vector_shape_vello_path, evaluate_expr, evaluate_expr_with_lookup_diagnostic,
    mark_track_manual_position, parse_color_in_env_with_lookup_diagnostic, parse_timing_modifiers,
    preserve_discrete_position_state_before, preserve_instant_delayed_value,
    push_unknown_target_path_diagnostic, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding,
};
use crate::ast::TargetSegment;
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::primitives::AssignmentCtx;
use crate::renderer::error::RenderError;
use crate::timeline::VelloPath;
use crate::timeline::build::build_graph_axis_paths;
use crate::timeline::property_engine::parse_property_value;
use crate::timeline::property_registry::{PropertyFlags, lookup_property};
use crate::timeline::property_track::TrackAccessor;

mod api;
mod effects;
mod position;
mod rebuild;

use effects::write_effect_stage_param;
use rebuild::{
    affects_shape_geometry, handle_size_assignment,
    push_unsupported_assignment_property_diagnostic, rebuild_vector_paths, scale_plot_curve_paths,
};

/// Build-fixed inputs of one assignment statement, resolved once before any
/// mutable component borrow by [`Timeline::prepare_assignment`].
struct AssignmentPlan {
    target_key: String,
    subject: String,
    eval_env: Environment,
    /// Build time of the statement in milliseconds (the un-offset clock the
    /// primitive context carries alongside `t_start_ms`).
    time_ms: u64,
    func_blend_mode: crate::timeline::plot::FuncBlendMode,
    t_start_ms: u64,
    t_end_ms: u64,
    duration_ms: f64,
    instant_delayed: bool,
    easing: Easing,
    is_size_property: bool,
    child_labels: Vec<String>,
    coordinate_frame: Option<crate::timeline::CoordinateFrameComponent>,
    caps: animatix_core::caps::ActorCaps,
    actor_type: String,
    track_id: crate::timeline::EntityId,
}

// Helper: text / math / code assignments → text paths

mod special;
mod text;
pub(crate) use text::recompile_text_at_assignment;
