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
pub use actor_kind::ActorKind;
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
    VectorShapeState, VectorShapeStyle, apply_vector_shape_defaults,
    apply_vector_shape_property, build_shape_vello_path, build_vector_shape_vello_path,
    finalize_vector_shape_state, parse_point_list_expr, shape_type_for_actor,
    vector_shape_exposes_tip_size, vector_shape_primitive_for_actor_type,
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
    AnimationTrack, Interpolate, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor,
    TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE,
};
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

#[derive(Clone, Debug)]
pub struct ContainerMetadata {
    pub layout_type: LayoutType,
    pub gap: f32,
    pub align: String,
    pub cols: Option<usize>,
    pub child_order: Vec<String>,
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
    /// Frame evaluation cache: avoids re-evaluating when time and dimensions match.
    frame_cache: std::cell::RefCell<Option<FrameCacheEntry>>,
}

/// Cache entry for frame evaluation results.
#[derive(Clone)]
pub(crate) struct FrameCacheEntry {
    time_ms: u64,
    dimensions: SceneDimensions,
    has_modifiers: bool,
    has_dynamic_layout: bool,
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
            frame_cache: std::cell::RefCell::new(None), // cache is not cloned
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
            frame_cache: std::cell::RefCell::new(None),
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

    /// Returns all keyframe time positions across all tracks, in seconds.
    /// Used by the GUI timeline scrubber to show keyframe markers.
    pub fn keyframe_times_s(&self) -> Vec<f64> {
        let mut times_ms = Vec::new();
        for track in self.tracks.values() {
            // Collect times from each property track
            if let Some(pos) = track.position.as_ref() {
                times_ms.extend(pos.keyframes.keys().copied());
            }
            if let Some(size) = track.size.as_ref() {
                times_ms.extend(size.keyframes.keys().copied());
            }
            if let Some(color) = track.color.as_ref() {
                times_ms.extend(color.keyframes.keys().copied());
            }
            if let Some(opacity) = track.opacity.as_ref() {
                times_ms.extend(opacity.keyframes.keys().copied());
            }
            if let Some(text) = track.text_paths.as_ref() {
                times_ms.extend(text.keyframes.keys().copied());
            }
            if let Some(vec) = track.vector_paths.as_ref() {
                times_ms.extend(vec.keyframes.keys().copied());
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

    /// Returns the list of root actor labels (actors with no parent).
    pub fn root_actor_labels(&self) -> &[String] {
        &self.root_nodes
    }

    /// Returns a reference to the track for the given label, if it exists.
    pub fn get_track(&self, label: &str) -> Option<&AnimationTrack> {
        self.tracks.get(label)
    }

    /// Returns the appropriate default color for a primitive type and property,
    /// based on the current colorscheme.
    pub fn get_default_color(&self, primitive_type: &str, property: &str) -> Option<[f32; 4]> {
        self.colorscheme.default_color_for_primitive(primitive_type, property)
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
                        },
                        Property {
                            name: "text.primary".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.9),
                                Expr::Num(0.95),
                                Expr::Num(1.0),
                            ]),
                        },
                    ],
                ),
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("test-scheme".to_string()),
                }],
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
                        },
                        Property {
                            name: "text.primary".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.85),
                                Expr::Num(0.9),
                                Expr::Num(0.95),
                            ]),
                        },
                    ],
                ),
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("test-scheme-let".to_string()),
                }],
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
                        },
                        Property {
                            name: "scene.background".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(0.5),
                                Expr::Num(0.5),
                                Expr::Num(0.5),
                            ]),
                        },
                    ],
                ),
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("child".to_string()),
                }],
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
                        },
                    ],
                ),
            },
            Stmt::Config {
                settings: vec![Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("auto-test".to_string()),
                }],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "a".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "b".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let mut timeline = report.output;

        let color_a = timeline.auto_color_for_label("a");
        let color_b = timeline.auto_color_for_label("b");

        assert_eq!(color_a, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(color_b, Some([0.0, 1.0, 0.0, 1.0]));
    }
}
