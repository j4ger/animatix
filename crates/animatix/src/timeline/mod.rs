pub mod actions;
mod assignments;
mod build;
pub mod colorscheme;
mod declarations_text;
pub mod env;
pub mod image;
pub mod kurbo_shapes;
mod layout;
mod media;
pub(crate) mod modifier_runtime;
pub mod morph;
mod plot;
mod position;
mod primitive;
mod property_lookup;
mod runtime;
mod scene_eval;
mod sequence;
mod shapes;
pub mod svg;
mod timing;
pub mod track;
pub mod utils;
pub mod vello_path;

use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use actions::process_action;
use colorscheme::{BuiltInColorscheme, ResolvedColorscheme};
pub use env::{Environment, EvalError, Value, load_standard_library};
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
    resolve_bound_position, resolve_position_binding_with_lookup_diagnostic, scene_anchor_point,
    set_track_position_binding,
};
pub(crate) use primitive::PrimitiveDescriptor;
use property_lookup::{
    assignment_target_key, best_path_suggestion, evaluate_expr_with_lookup_diagnostic,
    for_iter_values, parse_color_in_env_with_lookup_diagnostic, parse_numeric_vec2,
    parse_numeric_vec2_with_lookup_diagnostic, set_lookup_color, set_lookup_scalar,
    set_lookup_vec2,
};
use shapes::{
    SHAPE_GRAPH, SHAPE_PLOT, VectorShapeState, VectorShapeStyle, apply_vector_shape_defaults,
    apply_vector_shape_property, build_shape_vello_path, build_vector_shape_vello_path,
    finalize_vector_shape_state, shape_type_for_actor, vector_shape_exposes_tip_size,
    vector_shape_primitive_for_actor_type, vector_shape_uses_custom_path,
};
pub use svg::parse_svg;
pub(crate) use timing::{ModifierHost, ParsedTimingModifiers, parse_timing_modifiers};
use timing::{
    config_string_value, has_non_default_morph_options, parse_stagger_interval_ms,
    push_modifier_diagnostic, push_unknown_target_path_diagnostic,
    push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind,
};
pub use track::{
    AnimationTrack, Interpolate, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor,
};
pub use utils::{evaluate_expr, parse_color, parse_color_in_env, resolve_color_in_env, time_to_ms};
pub use vello_path::VelloPath;

use crate::ast::{Expr, Modifier, Stmt};
use crate::easing::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct SceneNode {
    pub label: String,
    pub children: Vec<String>,
}

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

#[derive(Clone)]
pub struct Timeline {
    pub tracks: BTreeMap<String, AnimationTrack>,
    pub background_color: PropertyTrack<[f32; 4]>,
    pub nodes: BTreeMap<String, SceneNode>,
    pub root_nodes: Vec<String>,
    pub anon_counter: usize,
    pub env: Environment,
    pub modifiers: Vec<Stmt>,
    colorscheme: ResolvedColorscheme,
    auto_color_assignments: BTreeMap<String, usize>,
    next_auto_color_index: usize,
}

impl Timeline {
    pub fn new() -> Self {
        let mut bg_track = PropertyTrack::new([0.0, 0.0, 0.0, 1.0]);
        bg_track.add_keyframe(0, [0.0, 0.0, 0.0, 1.0], Easing::Linear);
        Self {
            tracks: BTreeMap::new(),
            background_color: bg_track,
            nodes: BTreeMap::new(),
            root_nodes: Vec::new(),
            anon_counter: 0,
            env: Environment::raw_new(),
            modifiers: Vec::new(),
            colorscheme: BuiltInColorscheme::DefaultDark.resolved(),
            auto_color_assignments: BTreeMap::new(),
            next_auto_color_index: 0,
        }
    }

    pub fn duration_seconds(&self) -> f64 {
        let max_track_ms = self
            .tracks
            .values()
            .filter_map(|track| track.max_keyframe_time())
            .max()
            .unwrap_or(0);
        let max_bg_ms = self.background_color.last_keyframe_time().unwrap_or(0);
        (max_track_ms.max(max_bg_ms) as f64) / 1000.0
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
    use crate::ast::BinaryOp;

    #[test]
    fn test_for_iter_values_supports_tuple_literals() {
        let env = Environment::raw_new();
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
            }],
            else_branch: Some(vec![Stmt::Assignment {
                target: vec!["pulse".to_string()],
                property: "opacity".to_string(),
                value: Expr::Num(0.0),
                modifiers: vec![],
            }]),
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
}
