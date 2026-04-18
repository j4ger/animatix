pub mod actions;
pub mod colorscheme;
pub mod env;
pub mod image;
pub mod kurbo_shapes;
mod layout;
mod lookup;
pub mod morph;
mod plot;
mod position;
mod render;
mod runtime;
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
pub use kurbo_shapes::{KurboShape_, morph_kurbo_shapes, morph_kurbo_shapes_default};
use lookup::{
    assignment_target_key, best_path_suggestion, evaluate_expr_with_lookup_diagnostic,
    for_iter_values, parse_color_in_env_with_lookup_diagnostic, parse_numeric_vec2,
    parse_numeric_vec2_with_lookup_diagnostic, set_lookup_color, set_lookup_scalar,
    set_lookup_vec2,
};
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
use shapes::{
    SHAPE_ARROW, SHAPE_GRAPH, SHAPE_PATH, SHAPE_PLOT, SHAPE_POLYGON, build_shape_vello_path,
    parse_path_commands_expr, parse_point_list_expr, regular_polygon_points, shape_type_for_actor,
    styled_vello_path,
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

    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_diagnostics(ast).output
    }

    pub fn build_with_diagnostics(ast: &[Stmt]) -> BuildReport<Self> {
        let mut timeline = Self::new();
        load_standard_library(&mut timeline.env);
        timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
        let mut current_time_ms = 0.0;
        let mut diagnostics = Vec::new();

        for stmt in ast {
            if let Stmt::Config { settings } = stmt {
                timeline.apply_config_settings(settings, &mut diagnostics);
            }
        }

        for stmt in ast {
            match stmt {
                Stmt::Config { .. } => {}
                Stmt::Keyframe { time, body } => {
                    current_time_ms = time_to_ms(time);
                    timeline.process_body(current_time_ms, body, None, &mut diagnostics);
                }
                Stmt::RelativeKeyframe { offset, body } => {
                    current_time_ms += time_to_ms(offset);
                    timeline.process_body(current_time_ms, body, None, &mut diagnostics);
                }
                Stmt::ActorDecl { .. }
                | Stmt::Assignment { .. }
                | Stmt::Sequence { .. }
                | Stmt::Stagger { .. } => {
                    timeline.process_body(current_time_ms, &[stmt.clone()], None, &mut diagnostics);
                }
                _ => {}
            }
        }
        BuildReport::new(timeline, diagnostics)
    }

    fn apply_colorscheme(&mut self, colorscheme: ResolvedColorscheme) {
        colorscheme.seed_environment(&mut self.env);
        let background = colorscheme
            .color("scene.background")
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let mut bg_track = PropertyTrack::new(background);
        bg_track.add_keyframe(0, background, Easing::Linear);
        self.background_color = bg_track;
        self.colorscheme = colorscheme;
    }

    fn apply_config_settings(
        &mut self,
        settings: &[crate::ast::Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for setting in settings {
            if setting.name != "colorscheme" {
                continue;
            }

            let Some(raw_name) = config_string_value(&setting.value) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidConfigValue,
                        DiagnosticPhase::Build,
                        "Config key 'colorscheme' expects a built-in scheme name string such as \"editorial-dark\".".to_string(),
                    )
                    .with_subject("colorscheme"),
                );
                continue;
            };

            let Some(built_in) = BuiltInColorscheme::from_name(&raw_name) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownColorscheme,
                        DiagnosticPhase::Build,
                        format!(
                            "Unknown colorscheme '{raw_name}'; using the default-dark built-in scheme instead."
                        ),
                    )
                    .with_subject("colorscheme"),
                );
                continue;
            };

            self.apply_colorscheme(built_in.resolved());
        }
    }

    fn auto_color_for_label(&mut self, label: &str) -> Option<[f32; 4]> {
        if self.colorscheme.auto_cycle.is_empty() {
            return None;
        }

        let slot = if let Some(slot) = self.auto_color_assignments.get(label) {
            *slot
        } else {
            let slot = self.next_auto_color_index;
            self.auto_color_assignments.insert(label.to_string(), slot);
            self.next_auto_color_index += 1;
            slot
        };

        Some(self.colorscheme.auto_cycle[slot % self.colorscheme.auto_cycle.len()])
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

    fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        if !self.nodes.contains_key(&label) {
            self.nodes.insert(
                label.clone(),
                SceneNode {
                    label: label.clone(),
                    children: Vec::new(),
                },
            );
            if let Some(parent) = parent_label {
                if let Some(p) = self.nodes.get_mut(parent) {
                    if !p.children.contains(&label) {
                        p.children.push(label.clone());
                    }
                }
            } else {
                if !self.root_nodes.contains(&label) {
                    self.root_nodes.push(label.clone());
                }
            }
        }
    }

    /// Apply layout algorithm for Row and Col containers.
    /// Computes and sets child positions based on container type, gap, and alignment.
    ///
    /// - `gap`: spacing between children (default 0.0)
    /// - `align`: alignment perpendicular to the layout axis.
    ///   For Row: "center" (default), "start" (top), "end" (bottom)
    ///   For Col: "center" (default), "start" (left), "end" (right)
    fn process_inline_items(
        &mut self,
        time_ms: f64,
        items: &[crate::ast::InlineItem],
        parent_label: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for item in items {
            match item {
                crate::ast::InlineItem::Anonymous {
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    let id = format!("__anon_{}", self.anon_counter);
                    self.anon_counter += 1;
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        label: id.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                }
                crate::ast::InlineItem::Labeled {
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        label: label.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                }
            }
        }
    }

    fn process_body(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for stmt in body {
            match stmt {
                Stmt::Text {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_text".to_string());
                    let eval_env = self.build_eval_env(time_ms as u64);
                    self.add_node(label_str.clone(), parent_label);
                    let had_text_paths = self
                        .tracks
                        .get(&label_str)
                        .map(|track| !track.text_paths.keyframes.is_empty())
                        .unwrap_or(false);
                    let ParsedTimingModifiers {
                        duration_ms,
                        delay_ms,
                        easing,
                        morph_options,
                    } = parse_timing_modifiers(
                        modifiers,
                        ModifierHost::Text,
                        Some(&label_str),
                        diagnostics,
                    );
                    let t_start_ms = (time_ms + delay_ms) as u64;
                    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
                    let supports_morph_options = had_text_paths && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Morph-specific modifiers on text declaration require a re-declaration with non-zero duration; ignoring them for now."
                            ),
                            Some(&label_str),
                        );
                    }

                    let mut text_content = String::new();
                    let mut font_size = 48.0;
                    let mut color = typst::visualize::Color::from_u8(255, 255, 255, 255);
                    let mut initial_track_color: Option<[f32; 4]> = None;
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;

                    for prop in props {
                        let prop_subject = format!("{}.{}", label_str, prop.name);
                        match prop.name.as_str() {
                            "text" => {
                                text_content = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .map(|v| v.as_str())
                                .unwrap_or_default();
                            }
                            "font_size" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let resolved_color = if matches!(&prop.value, Expr::Ident(name) if name == "auto")
                                {
                                    self.auto_color_for_label(&label_str).or_else(|| {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                DiagnosticCode::UnknownColorReference,
                                                DiagnosticPhase::Build,
                                                format!(
                                                    "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.",
                                                    label_str
                                                ),
                                            )
                                            .with_subject(&prop_subject),
                                        );
                                        None
                                    })
                                } else {
                                    parse_color_in_env_with_lookup_diagnostic(
                                        &label_str,
                                        "color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                };

                                if let Some(c) = resolved_color {
                                    initial_track_color = Some(c);
                                    color = typst::visualize::Color::from_u8(
                                        (c[0] * 255.0) as u8,
                                        (c[1] * 255.0) as u8,
                                        (c[2] * 255.0) as u8,
                                        (c[3] * 255.0) as u8,
                                    );
                                }
                            }
                            "at" => {
                                at_expr = Some(prop.value.clone());
                            }
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str.clone()));

                    if let Some(track_color) = initial_track_color {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_instant_delayed_value(&mut track.color, t_start_ms);
                        }
                        track
                            .color
                            .add_keyframe(t_start_ms, track_color, Easing::Linear);
                    }

                    if let Some((binding, position)) =
                        resolve_position_binding_with_lookup_diagnostic(
                            at_expr.as_ref(),
                            anchor_expr.as_ref(),
                            offset_expr.as_ref(),
                            &eval_env,
                            diagnostics,
                            &label_str,
                        )
                    {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_discrete_position_state_before(track, t_start_ms);
                            preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        }
                        apply_explicit_position_binding(track, t_start_ms, binding, position);
                    }

                    let frame =
                        crate::renderer::text::compile_text(&text_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);
                    let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

                    if duration_ms > 0.0 {
                        let start_val = track.evaluate_text_paths(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                        let start_size = track.size.evaluate(t_start_ms);
                        track
                            .size
                            .add_keyframe(t_start_ms, start_size, Easing::Linear);
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
                        preserve_instant_delayed_value(&mut track.size, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                    track.size.add_keyframe(t_end_ms, new_half_size, easing);
                }
                Stmt::Math {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_math".to_string());
                    let eval_env = self.build_eval_env(time_ms as u64);
                    self.add_node(label_str.clone(), parent_label);
                    let had_text_paths = self
                        .tracks
                        .get(&label_str)
                        .map(|track| !track.text_paths.keyframes.is_empty())
                        .unwrap_or(false);
                    let ParsedTimingModifiers {
                        duration_ms,
                        delay_ms,
                        easing,
                        morph_options,
                    } = parse_timing_modifiers(
                        modifiers,
                        ModifierHost::Math,
                        Some(&label_str),
                        diagnostics,
                    );
                    let t_start_ms = (time_ms + delay_ms) as u64;
                    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
                    let supports_morph_options = had_text_paths && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Morph-specific modifiers on math declaration require a re-declaration with non-zero duration; ignoring them for now."
                            ),
                            Some(&label_str),
                        );
                    }

                    let mut latex_content = String::new();
                    let mut font_size = 48.0;
                    let mut color = typst::visualize::Color::from_u8(255, 255, 255, 255);
                    let mut initial_track_color: Option<[f32; 4]> = None;
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;

                    for prop in props {
                        let prop_subject = format!("{}.{}", label_str, prop.name);
                        match prop.name.as_str() {
                            "latex" | "math" => {
                                latex_content = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .map(|v| v.as_str())
                                .unwrap_or_default();
                            }
                            "font_size" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let resolved_color = if matches!(&prop.value, Expr::Ident(name) if name == "auto")
                                {
                                    self.auto_color_for_label(&label_str).or_else(|| {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                DiagnosticCode::UnknownColorReference,
                                                DiagnosticPhase::Build,
                                                format!(
                                                    "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.",
                                                    label_str
                                                ),
                                            )
                                            .with_subject(&prop_subject),
                                        );
                                        None
                                    })
                                } else {
                                    parse_color_in_env_with_lookup_diagnostic(
                                        &label_str,
                                        "color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                };

                                if let Some(c) = resolved_color {
                                    initial_track_color = Some(c);
                                    color = typst::visualize::Color::from_u8(
                                        (c[0] * 255.0) as u8,
                                        (c[1] * 255.0) as u8,
                                        (c[2] * 255.0) as u8,
                                        (c[3] * 255.0) as u8,
                                    );
                                }
                            }
                            "at" => {
                                at_expr = Some(prop.value.clone());
                            }
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str.clone()));

                    if let Some(track_color) = initial_track_color {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_instant_delayed_value(&mut track.color, t_start_ms);
                        }
                        track
                            .color
                            .add_keyframe(t_start_ms, track_color, Easing::Linear);
                    }

                    if let Some((binding, position)) =
                        resolve_position_binding_with_lookup_diagnostic(
                            at_expr.as_ref(),
                            anchor_expr.as_ref(),
                            offset_expr.as_ref(),
                            &eval_env,
                            diagnostics,
                            &label_str,
                        )
                    {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_discrete_position_state_before(track, t_start_ms);
                            preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        }
                        apply_explicit_position_binding(track, t_start_ms, binding, position);
                    }

                    let frame =
                        crate::renderer::text::compile_math(&latex_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);
                    let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

                    if duration_ms > 0.0 {
                        let start_val = track.evaluate_text_paths(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                        let start_size = track.size.evaluate(t_start_ms);
                        track
                            .size
                            .add_keyframe(t_start_ms, start_size, Easing::Linear);
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
                        preserve_instant_delayed_value(&mut track.size, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                    track.size.add_keyframe(t_end_ms, new_half_size, easing);
                }
                Stmt::Code {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_code".to_string());
                    let eval_env = self.build_eval_env(time_ms as u64);
                    self.add_node(label_str.clone(), parent_label);
                    let had_text_paths = self
                        .tracks
                        .get(&label_str)
                        .map(|track| !track.text_paths.keyframes.is_empty())
                        .unwrap_or(false);
                    let ParsedTimingModifiers {
                        duration_ms,
                        delay_ms,
                        easing,
                        morph_options,
                    } = parse_timing_modifiers(
                        modifiers,
                        ModifierHost::Code,
                        Some(&label_str),
                        diagnostics,
                    );
                    let t_start_ms = (time_ms + delay_ms) as u64;
                    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
                    let supports_morph_options = had_text_paths && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Morph-specific modifiers on code declaration require a re-declaration with non-zero duration; ignoring them for now."
                            ),
                            Some(&label_str),
                        );
                    }

                    let mut code_content = String::new();
                    let mut font_size = 24.0;
                    let mut color = typst::visualize::Color::from_u8(255, 255, 255, 255);
                    let mut initial_track_color: Option<[f32; 4]> = None;
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;

                    for prop in props {
                        let prop_subject = format!("{}.{}", label_str, prop.name);
                        match prop.name.as_str() {
                            "code" => {
                                code_content = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .map(|v| v.as_str())
                                .unwrap_or_default();
                            }
                            "font_size" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let resolved_color = if matches!(&prop.value, Expr::Ident(name) if name == "auto")
                                {
                                    self.auto_color_for_label(&label_str).or_else(|| {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                DiagnosticCode::UnknownColorReference,
                                                DiagnosticPhase::Build,
                                                format!(
                                                    "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.",
                                                    label_str
                                                ),
                                            )
                                            .with_subject(&prop_subject),
                                        );
                                        None
                                    })
                                } else {
                                    parse_color_in_env_with_lookup_diagnostic(
                                        &label_str,
                                        "color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                };

                                if let Some(c) = resolved_color {
                                    initial_track_color = Some(c);
                                    color = typst::visualize::Color::from_u8(
                                        (c[0] * 255.0) as u8,
                                        (c[1] * 255.0) as u8,
                                        (c[2] * 255.0) as u8,
                                        (c[3] * 255.0) as u8,
                                    );
                                }
                            }
                            "at" => {
                                at_expr = Some(prop.value.clone());
                            }
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str.clone()));

                    if let Some(track_color) = initial_track_color {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_instant_delayed_value(&mut track.color, t_start_ms);
                        }
                        track
                            .color
                            .add_keyframe(t_start_ms, track_color, Easing::Linear);
                    }

                    if let Some((binding, position)) =
                        resolve_position_binding_with_lookup_diagnostic(
                            at_expr.as_ref(),
                            anchor_expr.as_ref(),
                            offset_expr.as_ref(),
                            &eval_env,
                            diagnostics,
                            &label_str,
                        )
                    {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_discrete_position_state_before(track, t_start_ms);
                            preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        }
                        apply_explicit_position_binding(track, t_start_ms, binding, position);
                    }

                    let frame =
                        crate::renderer::text::compile_code(&code_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);
                    let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

                    if duration_ms > 0.0 {
                        let start_val = track.evaluate_text_paths(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                        let start_size = track.size.evaluate(t_start_ms);
                        track
                            .size
                            .add_keyframe(t_start_ms, start_size, Easing::Linear);
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
                        preserve_instant_delayed_value(&mut track.size, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                    track.size.add_keyframe(t_end_ms, new_half_size, easing);
                }
                Stmt::Svg {
                    label,
                    url,
                    at,
                    scale,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_svg".to_string());
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str));

                    track
                        .position
                        .add_keyframe(time_ms as u64, [at.0, at.1], Easing::Linear);

                    let svg_content = std::fs::read_to_string(url).unwrap_or_else(|e| {
                        eprintln!("Failed to read SVG file {}: {}", url, e);
                        String::new()
                    });

                    if !svg_content.is_empty() {
                        let mut parsed_paths = crate::timeline::svg::parse_svg(&svg_content);
                        if *scale != 1.0 {
                            let affine = kurbo::Affine::scale(*scale as f64);
                            for vp in &mut parsed_paths {
                                vp.path.apply_affine(affine);
                            }
                        }
                        let measured_half_size =
                            crate::timeline::svg::measure_svg_paths(&parsed_paths);
                        track
                            .size
                            .add_keyframe(time_ms as u64, measured_half_size, Easing::Linear);
                        track.svg_paths = parsed_paths;
                    }
                }
                Stmt::Image {
                    label,
                    url,
                    at,
                    size,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_image".to_string());
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str));

                    track
                        .position
                        .add_keyframe(time_ms as u64, [at.0, at.1], Easing::Linear);

                    if let Some(image) = crate::timeline::image::load_image(url) {
                        let display_size = size
                            .map(|(width, height)| [width / 2.0, height / 2.0])
                            .unwrap_or([image.natural_size[0] / 2.0, image.natural_size[1] / 2.0]);

                        track
                            .size
                            .add_keyframe(time_ms as u64, display_size, Easing::Linear);
                        track
                            .image
                            .add_keyframe(time_ms as u64, Some(image), Easing::Linear);
                    } else {
                        eprintln!("Failed to load image file {}", url);
                    }
                }
                Stmt::ActorDecl {
                    is_pub: _,
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    self.add_node(label.clone(), parent_label);

                    let mut x_domain = [-10.0, 10.0];
                    let mut y_domain = [-10.0, 10.0];
                    let mut t_domain = [0.0, std::f64::consts::TAU];
                    let mut func = None;
                    let mut initial_size = [50.0, 50.0];
                    let mut tolerance = 0.5;
                    let mut max_depth = 10.0;
                    let mut resolution = 96.0;
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;
                    let initial_eval_env = self.build_eval_env(time_ms as u64);

                    for prop in props {
                        let prop_subject = format!("{}.{}", label, prop.name);
                        match prop.name.as_str() {
                            "size" => {
                                let size_val = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    initial_size[0] = w as f32 / 2.0;
                                    initial_size[1] = h as f32 / 2.0;
                                }
                            }
                            "radius" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                initial_size = [r, r];
                            }
                            "x_domain" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    x_domain = [min, max];
                                }
                            }
                            "y_domain" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    y_domain = [min, max];
                                }
                            }
                            "t_domain" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    t_domain = [min, max];
                                }
                            }
                            "func" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Closure(args, body) = v {
                                    func = Some((args, body));
                                }
                            }
                            "tolerance" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                tolerance = v.as_num();
                            }
                            "max_depth" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                max_depth = v.as_num();
                            }
                            "resolution" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(96.0));
                                resolution = v.as_num();
                            }
                            "at" => at_expr = Some(prop.value.clone()),
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    if ty == "Graph" {
                        self.env
                            .set(&format!("{}_x_domain", label), Value::Vec2(x_domain));
                        self.env
                            .set(&format!("{}_y_domain", label), Value::Vec2(y_domain));
                        self.env.set(
                            &format!("{}_size", label),
                            Value::Vec2([
                                initial_size[0] as f64 * 2.0,
                                initial_size[1] as f64 * 2.0,
                            ]),
                        );
                    }

                    self.process_inline_items(time_ms, children, label, diagnostics);
                    let eval_env = self.build_eval_env(time_ms as u64);
                    let existing_track = self
                        .tracks
                        .get(label)
                        .cloned()
                        .unwrap_or_else(|| AnimationTrack::new(label.clone()));

                    let mut position = existing_track.position.last_value();
                    let mut size = existing_track.size.last_value();
                    if ty == "Dot" && size == [50.0, 50.0] {
                        size = [6.0, 6.0];
                    } else if ty == "Arrow" && size == [50.0, 50.0] {
                        size = [24.0, 18.0];
                    }
                    let mut line_from = existing_track.line_from.last_value();
                    let mut line_to = existing_track.line_to.last_value();
                    let mut arc_angles = existing_track.arc_angles.last_value();
                    let mut color = existing_track.color.last_value();
                    let shape_type = shape_type_for_actor(ty);
                    let opacity = existing_track.opacity.last_value();
                    let mut stroke_width = existing_track.stroke_width.last_value();
                    let mut stroke_color = existing_track.stroke_color.last_value();
                    let mut stroke_progress = existing_track.stroke_progress.last_value();
                    let mut fill_opacity = existing_track.fill_opacity.last_value();
                    let mut gap = 0.0f32;
                    let mut align: Option<String> = None;
                    let mut cols: Option<usize> = None;
                    let mut custom_path: Option<kurbo::BezPath> = None;
                    let mut regular_polygon_sides: usize = 5;
                    let mut regular_polygon_radius = size[0];

                    let ParsedTimingModifiers {
                        duration_ms,
                        delay_ms,
                        easing,
                        morph_options,
                    } = parse_timing_modifiers(
                        modifiers,
                        ModifierHost::ActorDeclaration,
                        Some(label),
                        diagnostics,
                    );
                    let t_start_ms = (time_ms + delay_ms) as u64;
                    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
                    let supports_morph_options =
                        !existing_track.vector_paths.keyframes.is_empty() && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            "Morph-specific modifiers on actor declarations require a path-morphing re-declaration with non-zero duration; ignoring them for now.".to_string(),
                            Some(label),
                        );
                    }

                    for prop in props {
                        let prop_subject = format!("{}.{}", label, prop.name);
                        match prop.name.as_str() {
                            "at" | "anchor" | "offset" => {}
                            "radius" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                size = [r, r];
                                regular_polygon_radius = r;
                            }
                            "side" if ty == "Square" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(size[0] as f64 * 2.0));
                                let side = v.as_num() as f32;
                                size = [side / 2.0, side / 2.0];
                            }
                            "size" => {
                                let size_val = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    size[0] = w as f32 / 2.0;
                                    size[1] = h as f32 / 2.0;
                                }
                            }
                            "tip_length" if ty == "Arrow" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(size[0] as f64));
                                size[0] = v.as_num() as f32;
                            }
                            "tip_width" if ty == "Arrow" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(size[1] as f64));
                                size[1] = v.as_num() as f32;
                            }
                            "sides" if ty == "RegularPolygon" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(regular_polygon_sides as f64));
                                regular_polygon_sides = v.as_num().round().max(3.0) as usize;
                            }
                            "from" if ty == "Line" => {
                                if let Some(parsed) = parse_numeric_vec2_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                ) {
                                    line_from = parsed;
                                }
                            }
                            "to" if ty == "Line" => {
                                if let Some(parsed) = parse_numeric_vec2_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                ) {
                                    line_to = parsed;
                                }
                            }
                            "radius_x" if ty == "Ellipse" || ty == "Arc" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(size[0] as f64));
                                size[0] = v.as_num() as f32;
                            }
                            "radius_y" if ty == "Ellipse" || ty == "Arc" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(size[1] as f64));
                                size[1] = v.as_num() as f32;
                            }
                            "start_angle" if ty == "Arc" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(arc_angles[0] as f64));
                                arc_angles[0] = v.as_num() as f32;
                            }
                            "sweep_angle" if ty == "Arc" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(arc_angles[1] as f64));
                                arc_angles[1] = v.as_num() as f32;
                            }
                            "points" if ty == "Polygon" || ty == "RegularPolygon" => {
                                if let Some(points) = parse_point_list_expr(&prop.value, &eval_env)
                                {
                                    custom_path =
                                        Some(KurboShape_::Polygon { points }.to_path_default());
                                }
                            }
                            "commands" if ty == "Path" => {
                                custom_path = parse_path_commands_expr(&prop.value, &eval_env);
                            }
                            "color" => {
                                if matches!(&prop.value, Expr::Ident(name) if name == "auto") {
                                    if let Some(actor_color) = self.auto_color_for_label(label) {
                                        color = actor_color;
                                        if ty == "CartesianPlot"
                                            || ty == "PolarPlot"
                                            || ty == "ParametricPlot"
                                            || ty == "ImplicitPlot"
                                        {
                                            stroke_color = actor_color;
                                        }
                                    } else {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                DiagnosticCode::UnknownColorReference,
                                                DiagnosticPhase::Build,
                                                format!(
                                                    "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.",
                                                    label
                                                ),
                                            )
                                            .with_subject(&prop_subject),
                                        );
                                    }
                                } else if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label,
                                        "color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                {
                                    color = resolved_color;
                                    if ty == "CartesianPlot"
                                        || ty == "PolarPlot"
                                        || ty == "ParametricPlot"
                                        || ty == "ImplicitPlot"
                                    {
                                        stroke_color = resolved_color;
                                    }
                                }
                            }
                            "stroke_width" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "width" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "stroke_color" => {
                                if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label,
                                        "stroke_color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                {
                                    stroke_color = resolved_color;
                                }
                            }
                            "stroke" => {
                                if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label,
                                        "stroke",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                {
                                    stroke_color = resolved_color;
                                }
                            }
                            "stroke_progress" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                stroke_progress = v.as_num() as f32;
                            }
                            "fill_opacity" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                fill_opacity = v.as_num() as f32;
                            }
                            "gap" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                gap = v.as_num() as f32;
                            }
                            "align" => {
                                if let Expr::Str(s) = &prop.value {
                                    align = Some(s.clone());
                                } else if let Expr::Ident(s) = &prop.value {
                                    align = Some(s.clone());
                                }
                            }
                            "cols" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(1.0));
                                cols = Some(v.as_num().max(1.0) as usize);
                            }
                            _ => {}
                        }
                    }

                    if ty == "RegularPolygon" && custom_path.is_none() {
                        custom_path = Some(
                            KurboShape_::Polygon {
                                points: regular_polygon_points(
                                    regular_polygon_sides,
                                    regular_polygon_radius,
                                ),
                            }
                            .to_path_default(),
                        );
                    }

                    // For Graph types, make them invisible (container only)
                    if ty == "Graph" {
                        fill_opacity = 0.0;
                        stroke_width = 0.0;
                    }

                    let track = self
                        .tracks
                        .entry(label.clone())
                        .or_insert_with(|| AnimationTrack::new(label.clone()));

                    if let Some((binding, bound_position)) =
                        resolve_position_binding_with_lookup_diagnostic(
                            at_expr.as_ref(),
                            anchor_expr.as_ref(),
                            offset_expr.as_ref(),
                            &eval_env,
                            diagnostics,
                            label,
                        )
                    {
                        preserve_discrete_position_state_before(track, t_start_ms);
                        set_track_position_binding(track, t_start_ms, binding);
                        if let Some(bound_position) = bound_position {
                            position = bound_position;
                            mark_track_manual_position(track, t_start_ms);
                        } else {
                            mark_track_manual_position(track, t_start_ms);
                        }
                    } else if Timeline::is_layout_container_type(ty) && parent_label.is_none() {
                        preserve_discrete_position_state_before(track, t_start_ms);
                        set_track_position_binding(
                            track,
                            t_start_ms,
                            PositionBinding::ContainerDefault {
                                anchor: SceneAnchor::Center,
                            },
                        );
                    }

                    let mut vello_paths = vec![];

                    if ty == "Graph" {
                        let mut path = kurbo::BezPath::new();
                        // X axis
                        let x_axis_y = if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
                            size[1] as f64
                                * (1.0 - 2.0 * (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]))
                        } else {
                            size[1] as f64
                        };
                        path.move_to((-(size[0] as f64), x_axis_y));
                        path.line_to((size[0] as f64, x_axis_y));

                        // Y axis
                        let y_axis_x = if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
                            size[0] as f64
                                * (-1.0 + 2.0 * (0.0 - x_domain[0]) / (x_domain[1] - x_domain[0]))
                        } else {
                            -(size[0] as f64)
                        };
                        path.move_to((y_axis_x, -(size[1] as f64)));
                        path.line_to((y_axis_x, size[1] as f64));

                        vello_paths.push(crate::timeline::vello_path::VelloPath {
                            path,
                            fill: None,
                            stroke: Some((
                                vello::peniko::Color::from_rgba8(255, 255, 255, 255),
                                2.0,
                            )),
                        });
                    } else if ty == "CartesianPlot"
                        || ty == "PolarPlot"
                        || ty == "ParametricPlot"
                        || ty == "ImplicitPlot"
                    {
                        let p_label = parent_label.unwrap_or("").to_string();
                        let mut p_x_domain = [-10.0, 10.0];
                        let mut p_y_domain = [-10.0, 10.0];
                        let mut p_size = [500.0, 500.0];

                        if let Some(Value::Vec2(xd)) =
                            self.env.get(&format!("{}_x_domain", p_label))
                        {
                            p_x_domain = xd;
                        }
                        if let Some(Value::Vec2(yd)) =
                            self.env.get(&format!("{}_y_domain", p_label))
                        {
                            p_y_domain = yd;
                        }
                        if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                            p_size = sz;
                        }

                        if let Some((args, body)) = func {
                            let mut env_copy = eval_env.clone();
                            let arg_name = if !args.is_empty() {
                                args[0].clone()
                            } else {
                                "x".to_string()
                            };

                            let (min_t, max_t) = if ty == "CartesianPlot" {
                                (p_x_domain[0], p_x_domain[1])
                            } else if ty == "ImplicitPlot" {
                                (0.0, 0.0)
                            } else {
                                (t_domain[0], t_domain[1])
                            };

                            if ty == "ImplicitPlot" {
                                let path = build_implicit_plot_path(
                                    &mut env_copy,
                                    &args,
                                    &body,
                                    &p_x_domain,
                                    &p_y_domain,
                                    &p_size,
                                    resolution.round().max(8.0) as usize,
                                );
                                vello_paths.push(crate::timeline::vello_path::VelloPath {
                                    path,
                                    fill: None,
                                    stroke: if stroke_width > 0.0 {
                                        Some((
                                            vello::peniko::Color::from_rgba8(
                                                (stroke_color[0] * 255.0) as u8,
                                                (stroke_color[1] * 255.0) as u8,
                                                (stroke_color[2] * 255.0) as u8,
                                                (stroke_color[3] * 255.0) as u8,
                                            ),
                                            stroke_width,
                                        ))
                                    } else {
                                        None
                                    },
                                });
                            } else {
                                env_copy.set(&arg_name, Value::Num(min_t));
                                let start_eval =
                                    evaluate_expr(&body, &env_copy).unwrap_or(Value::Num(0.0));
                                let (start_math_x, start_math_y) = if ty == "CartesianPlot" {
                                    (min_t, start_eval.as_num())
                                } else if ty == "ParametricPlot" {
                                    match start_eval {
                                        Value::Vec2([x, y]) => (x, y),
                                        _ => (0.0, 0.0),
                                    }
                                } else {
                                    let start_val = start_eval.as_num();
                                    (start_val * min_t.cos(), start_val * min_t.sin())
                                };
                                let start_screen_x = -(p_size[0] / 2.0)
                                    + p_size[0]
                                        * ((start_math_x - p_x_domain[0])
                                            / (p_x_domain[1] - p_x_domain[0]));
                                let start_screen_y = (p_size[1] / 2.0)
                                    - p_size[1]
                                        * ((start_math_y - p_y_domain[0])
                                            / (p_y_domain[1] - p_y_domain[0]));

                                env_copy.set(&arg_name, Value::Num(max_t));
                                let end_eval =
                                    evaluate_expr(&body, &env_copy).unwrap_or(Value::Num(0.0));
                                let (end_math_x, end_math_y) = if ty == "CartesianPlot" {
                                    (max_t, end_eval.as_num())
                                } else if ty == "ParametricPlot" {
                                    match end_eval {
                                        Value::Vec2([x, y]) => (x, y),
                                        _ => (0.0, 0.0),
                                    }
                                } else {
                                    let end_val = end_eval.as_num();
                                    (end_val * max_t.cos(), end_val * max_t.sin())
                                };
                                let end_screen_x = -(p_size[0] / 2.0)
                                    + p_size[0]
                                        * ((end_math_x - p_x_domain[0])
                                            / (p_x_domain[1] - p_x_domain[0]));
                                let end_screen_y = (p_size[1] / 2.0)
                                    - p_size[1]
                                        * ((end_math_y - p_y_domain[0])
                                            / (p_y_domain[1] - p_y_domain[0]));

                                let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
                                let p1 = kurbo::Point::new(end_screen_x, end_screen_y);

                                let mut pts = vec![p0];

                                if ty == "CartesianPlot" {
                                    sample_recursive_cartesian(
                                        min_t,
                                        max_t,
                                        p0,
                                        p1,
                                        0,
                                        max_depth as usize,
                                        tolerance,
                                        &mut env_copy,
                                        &arg_name,
                                        &body,
                                        &p_x_domain,
                                        &p_y_domain,
                                        &p_size,
                                        &mut pts,
                                    );
                                } else if ty == "PolarPlot" {
                                    sample_recursive_polar(
                                        min_t,
                                        max_t,
                                        p0,
                                        p1,
                                        0,
                                        max_depth as usize,
                                        tolerance,
                                        &mut env_copy,
                                        &arg_name,
                                        &body,
                                        &p_x_domain,
                                        &p_y_domain,
                                        &p_size,
                                        &mut pts,
                                    );
                                } else {
                                    sample_recursive_parametric(
                                        min_t,
                                        max_t,
                                        p0,
                                        p1,
                                        0,
                                        max_depth as usize,
                                        tolerance,
                                        &mut env_copy,
                                        &arg_name,
                                        &body,
                                        &p_x_domain,
                                        &p_y_domain,
                                        &p_size,
                                        &mut pts,
                                    );
                                }

                                let mut path = kurbo::BezPath::new();
                                let mut first = true;
                                for pt in pts {
                                    if pt.x.is_nan() || pt.y.is_nan() {
                                        first = true;
                                    } else if first {
                                        path.move_to((pt.x, pt.y));
                                        first = false;
                                    } else {
                                        path.line_to((pt.x, pt.y));
                                    }
                                }
                                vello_paths.push(crate::timeline::vello_path::VelloPath {
                                    path,
                                    fill: None,
                                    stroke: if stroke_width > 0.0 {
                                        Some((
                                            vello::peniko::Color::from_rgba8(
                                                (stroke_color[0] * 255.0) as u8,
                                                (stroke_color[1] * 255.0) as u8,
                                                (stroke_color[2] * 255.0) as u8,
                                                (stroke_color[3] * 255.0) as u8,
                                            ),
                                            stroke_width,
                                        ))
                                    } else {
                                        None
                                    },
                                });
                            }
                        }
                    } else if ty != "Graph"
                        && ty != "CartesianPlot"
                        && ty != "PolarPlot"
                        && ty != "ParametricPlot"
                        && ty != "ImplicitPlot"
                    {
                        let vello_path = if matches!(shape_type, SHAPE_POLYGON | SHAPE_PATH) {
                            let path = custom_path.unwrap_or_else(kurbo::BezPath::new);
                            styled_vello_path(
                                path,
                                shape_type,
                                color,
                                stroke_width,
                                stroke_color,
                                fill_opacity,
                            )
                        } else {
                            build_shape_vello_path(
                                shape_type,
                                size,
                                line_from,
                                line_to,
                                arc_angles,
                                color,
                                stroke_width,
                                stroke_color,
                                fill_opacity,
                            )
                        };
                        vello_paths.push(vello_path);
                    }

                    if duration_ms > 0.0 {
                        let start_vector_paths = track.evaluate_vector_paths(t_start_ms);
                        let start_position = track.position.evaluate(t_start_ms);
                        let start_size = track.size.evaluate(t_start_ms);
                        let start_line_from = track.line_from.evaluate(t_start_ms);
                        let start_line_to = track.line_to.evaluate(t_start_ms);
                        let start_arc_angles = track.arc_angles.evaluate(t_start_ms);
                        let start_color = track.color.evaluate(t_start_ms);
                        let start_shape_type = track.shape_type.evaluate(t_start_ms);
                        let start_opacity = track.opacity.evaluate(t_start_ms);
                        let start_stroke_width = track.stroke_width.evaluate(t_start_ms);
                        let start_stroke_color = track.stroke_color.evaluate(t_start_ms);
                        let start_stroke_progress = track.stroke_progress.evaluate(t_start_ms);
                        let start_fill_opacity = track.fill_opacity.evaluate(t_start_ms);

                        track.vector_paths.add_keyframe(
                            t_start_ms,
                            start_vector_paths,
                            Easing::Linear,
                        );
                        track
                            .position
                            .add_keyframe(t_start_ms, start_position, Easing::Linear);
                        track
                            .size
                            .add_keyframe(t_start_ms, start_size, Easing::Linear);
                        track
                            .line_from
                            .add_keyframe(t_start_ms, start_line_from, Easing::Linear);
                        track
                            .line_to
                            .add_keyframe(t_start_ms, start_line_to, Easing::Linear);
                        track
                            .arc_angles
                            .add_keyframe(t_start_ms, start_arc_angles, Easing::Linear);
                        track
                            .color
                            .add_keyframe(t_start_ms, start_color, Easing::Linear);
                        track
                            .shape_type
                            .add_keyframe(t_start_ms, start_shape_type, Easing::Linear);
                        track
                            .opacity
                            .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
                        track.stroke_width.add_keyframe(
                            t_start_ms,
                            start_stroke_width,
                            Easing::Linear,
                        );
                        track.stroke_color.add_keyframe(
                            t_start_ms,
                            start_stroke_color,
                            Easing::Linear,
                        );
                        track.stroke_progress.add_keyframe(
                            t_start_ms,
                            start_stroke_progress,
                            Easing::Linear,
                        );
                        track.fill_opacity.add_keyframe(
                            t_start_ms,
                            start_fill_opacity,
                            Easing::Linear,
                        );
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.vector_paths, t_start_ms);
                        preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        preserve_instant_delayed_value(&mut track.size, t_start_ms);
                        preserve_instant_delayed_value(&mut track.line_from, t_start_ms);
                        preserve_instant_delayed_value(&mut track.line_to, t_start_ms);
                        preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                        preserve_instant_delayed_value(&mut track.color, t_start_ms);
                        preserve_instant_delayed_value(&mut track.shape_type, t_start_ms);
                        preserve_instant_delayed_value(&mut track.opacity, t_start_ms);
                        preserve_instant_delayed_value(&mut track.stroke_width, t_start_ms);
                        preserve_instant_delayed_value(&mut track.stroke_color, t_start_ms);
                        preserve_instant_delayed_value(&mut track.stroke_progress, t_start_ms);
                        preserve_instant_delayed_value(&mut track.fill_opacity, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }

                    track
                        .vector_paths
                        .add_keyframe(t_end_ms, vello_paths, easing);
                    track.position.add_keyframe(t_end_ms, position, easing);
                    track.size.add_keyframe(t_end_ms, size, easing);
                    track.line_from.add_keyframe(t_end_ms, line_from, easing);
                    track.line_to.add_keyframe(t_end_ms, line_to, easing);
                    track.arc_angles.add_keyframe(t_end_ms, arc_angles, easing);
                    track.color.add_keyframe(t_end_ms, color, easing);
                    track.shape_type.add_keyframe(t_end_ms, shape_type, easing);
                    track.opacity.add_keyframe(t_end_ms, opacity, easing);
                    track
                        .stroke_width
                        .add_keyframe(t_end_ms, stroke_width, easing);
                    track
                        .stroke_color
                        .add_keyframe(t_end_ms, stroke_color, easing);
                    track
                        .stroke_progress
                        .add_keyframe(t_end_ms, stroke_progress, easing);
                    track
                        .fill_opacity
                        .add_keyframe(t_end_ms, fill_opacity, easing);

                    if Timeline::is_layout_container_type(ty) {
                        self.apply_container_layout(
                            label,
                            ty,
                            t_start_ms as f64,
                            gap,
                            align.as_deref(),
                            cols,
                        );
                    }
                }
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                } => {
                    let eval_env = self.build_eval_env(time_ms as u64);
                    let assignment_subject = format!("{}.{}", target.join("."), property);
                    let ParsedTimingModifiers {
                        duration_ms,
                        delay_ms,
                        easing,
                        ..
                    } = parse_timing_modifiers(
                        modifiers,
                        ModifierHost::Assignment,
                        Some(&assignment_subject),
                        diagnostics,
                    );

                    let t_start_ms = (time_ms + delay_ms) as u64;
                    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
                    let instant_delayed = delay_ms > 0.0 && duration_ms == 0.0;

                    if target.len() == 1 && target[0] == "scene" {
                        if property == "background_color" {
                            let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                                "scene",
                                "background_color",
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            ) else {
                                continue;
                            };
                            if duration_ms > 0.0 {
                                let start_val = self.background_color.evaluate(t_start_ms);
                                self.background_color.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(
                                    &mut self.background_color,
                                    t_start_ms,
                                );
                            }
                            self.background_color
                                .add_keyframe(t_end_ms, target_color, easing);
                        }
                        continue;
                    }

                    let target_key = assignment_target_key(target);

                    if target.len() > 1 && !self.nodes.contains_key(&target_key) {
                        let suggestion = best_path_suggestion(
                            &target_key,
                            self.nodes.keys().map(String::as_str),
                        );
                        push_unknown_target_path_diagnostic(
                            diagnostics,
                            &assignment_subject,
                            &target_key,
                            suggestion,
                        );
                        continue;
                    }

                    let track = self
                        .tracks
                        .entry(target_key.clone())
                        .or_insert_with(|| AnimationTrack::new(target_key.clone()));

                    match property.as_str() {
                        "color" => {
                            let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                                &target_key,
                                "color",
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            ) else {
                                continue;
                            };
                            if duration_ms > 0.0 {
                                let start_val = track.color.evaluate(t_start_ms);
                                track
                                    .color
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.color, t_start_ms);
                            }
                            track.color.add_keyframe(t_end_ms, target_color, easing);
                        }
                        "stroke_width" => {
                            let target_width = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(0.0))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_width.evaluate(t_start_ms);
                                track.stroke_width.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.stroke_width, t_start_ms);
                            }
                            track
                                .stroke_width
                                .add_keyframe(t_end_ms, target_width, easing);
                        }
                        "stroke_color" => {
                            let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                                &target_key,
                                "stroke_color",
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            ) else {
                                continue;
                            };
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_color.evaluate(t_start_ms);
                                track.stroke_color.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.stroke_color, t_start_ms);
                            }
                            track
                                .stroke_color
                                .add_keyframe(t_end_ms, target_color, easing);
                        }
                        "stroke_progress" => {
                            let target_val = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(0.0))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_progress.evaluate(t_start_ms);
                                track.stroke_progress.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(
                                    &mut track.stroke_progress,
                                    t_start_ms,
                                );
                            }
                            track
                                .stroke_progress
                                .add_keyframe(t_end_ms, target_val, easing);
                        }
                        "fill_opacity" => {
                            let target_val = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(0.0))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.fill_opacity.evaluate(t_start_ms);
                                track.fill_opacity.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.fill_opacity, t_start_ms);
                            }
                            track
                                .fill_opacity
                                .add_keyframe(t_end_ms, target_val, easing);
                        }
                        "size" => {
                            let size_val = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(0.0));
                            let target_size = if let Value::Vec2([w, h]) = size_val {
                                [w as f32 / 2.0, h as f32 / 2.0]
                            } else {
                                track.size.last_value()
                            };
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.size, t_start_ms);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "tip_length" => {
                            let target_tip_length = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.size.last_value()[0] as f64))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.size, t_start_ms);
                            }
                            let mut target_size = track.size.evaluate(t_end_ms);
                            target_size[0] = target_tip_length;
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "tip_width" => {
                            let target_tip_width = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.size.last_value()[1] as f64))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.size, t_start_ms);
                            }
                            let mut target_size = track.size.evaluate(t_end_ms);
                            target_size[1] = target_tip_width;
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "url" => {
                            let target_url = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Str(String::new()))
                            .as_str();
                            if !target_url.is_empty() {
                                if let Some(target_image) =
                                    crate::timeline::image::load_image(&target_url)
                                {
                                    if duration_ms > 0.0 {
                                        let start_val = track.image.evaluate(t_start_ms);
                                        track.image.add_keyframe(
                                            t_start_ms,
                                            start_val,
                                            Easing::Linear,
                                        );
                                    } else if instant_delayed {
                                        preserve_instant_delayed_value(
                                            &mut track.image,
                                            t_start_ms,
                                        );
                                    }
                                    track
                                        .image
                                        .add_keyframe(t_end_ms, Some(target_image), easing);
                                } else {
                                    eprintln!("Failed to load image file {}", target_url);
                                }
                            }
                        }
                        "position" | "at" => {
                            let target_pos = if let Some((binding, position)) =
                                resolve_position_binding_with_lookup_diagnostic(
                                    Some(value),
                                    None,
                                    None,
                                    &eval_env,
                                    diagnostics,
                                    &assignment_subject,
                                ) {
                                preserve_discrete_position_state_before(track, t_start_ms);
                                if instant_delayed {
                                    preserve_instant_delayed_value(&mut track.position, t_start_ms);
                                }
                                mark_track_manual_position(track, t_start_ms);

                                if duration_ms > 0.0 {
                                    let start_binding = track.position_binding.evaluate(t_start_ms);
                                    track.position_binding.add_keyframe(
                                        t_start_ms,
                                        start_binding,
                                        Easing::Linear,
                                    );
                                    track
                                        .position_binding
                                        .add_keyframe(t_end_ms, binding, easing);
                                } else {
                                    set_track_position_binding(track, t_start_ms, binding);
                                }

                                position.unwrap_or_else(|| track.position.last_value())
                            } else {
                                track.position.last_value()
                            };
                            if duration_ms > 0.0 {
                                let start_val = track.position.evaluate(t_start_ms);
                                track
                                    .position
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.position, t_start_ms);
                            }
                            track.position.add_keyframe(t_end_ms, target_pos, easing);
                        }
                        "rotation" => {
                            let target_rotation = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.rotation.last_value() as f64))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.rotation.evaluate(t_start_ms);
                                track
                                    .rotation
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.rotation, t_start_ms);
                            }
                            track
                                .rotation
                                .add_keyframe(t_end_ms, target_rotation, easing);
                        }
                        "scale" => {
                            let target_scale = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.scale.last_value() as f64))
                            .as_num() as f32;
                            if duration_ms > 0.0 {
                                let start_val = track.scale.evaluate(t_start_ms);
                                track
                                    .scale
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.scale, t_start_ms);
                            }
                            track.scale.add_keyframe(t_end_ms, target_scale, easing);
                        }
                        "radius" => {
                            let r = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(0.0))
                            .as_num() as f32;
                            let target_size = [r, r];
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.size, t_start_ms);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "radius_x" => {
                            let target_radius = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.size.last_value()[0] as f64))
                            .as_num() as f32;
                            let mut target_size = track.size.last_value();
                            target_size[0] = target_radius;
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.size, t_start_ms);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "radius_y" => {
                            let target_radius = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.size.last_value()[1] as f64))
                            .as_num() as f32;
                            let mut target_size = track.size.last_value();
                            target_size[1] = target_radius;
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.size, t_start_ms);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "start_angle" => {
                            let target_angle = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.arc_angles.last_value()[0] as f64))
                            .as_num() as f32;
                            let mut target_angles = track.arc_angles.last_value();
                            target_angles[0] = target_angle;
                            if duration_ms > 0.0 {
                                let start_val = track.arc_angles.evaluate(t_start_ms);
                                track.arc_angles.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                            }
                            track
                                .arc_angles
                                .add_keyframe(t_end_ms, target_angles, easing);
                        }
                        "sweep_angle" => {
                            let target_angle = evaluate_expr_with_lookup_diagnostic(
                                value,
                                &eval_env,
                                diagnostics,
                                &assignment_subject,
                            )
                            .unwrap_or(Value::Num(track.arc_angles.last_value()[1] as f64))
                            .as_num() as f32;
                            let mut target_angles = track.arc_angles.last_value();
                            target_angles[1] = target_angle;
                            if duration_ms > 0.0 {
                                let start_val = track.arc_angles.evaluate(t_start_ms);
                                track.arc_angles.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                            }
                            track
                                .arc_angles
                                .add_keyframe(t_end_ms, target_angles, easing);
                        }
                        "from" => {
                            if let Some(target_from) = parse_numeric_vec2(value, &eval_env) {
                                if duration_ms > 0.0 {
                                    let start_val = track.line_from.evaluate(t_start_ms);
                                    track.line_from.add_keyframe(
                                        t_start_ms,
                                        start_val,
                                        Easing::Linear,
                                    );
                                } else if instant_delayed {
                                    preserve_instant_delayed_value(
                                        &mut track.line_from,
                                        t_start_ms,
                                    );
                                }
                                track.line_from.add_keyframe(t_end_ms, target_from, easing);
                            }
                        }
                        "to" => {
                            if let Some(target_to) = parse_numeric_vec2(value, &eval_env) {
                                if duration_ms > 0.0 {
                                    let start_val = track.line_to.evaluate(t_start_ms);
                                    track.line_to.add_keyframe(
                                        t_start_ms,
                                        start_val,
                                        Easing::Linear,
                                    );
                                } else if instant_delayed {
                                    preserve_instant_delayed_value(&mut track.line_to, t_start_ms);
                                }
                                track.line_to.add_keyframe(t_end_ms, target_to, easing);
                            }
                        }
                        _ => {}
                    }

                    if !track.vector_paths.default_value.is_empty()
                        || !track.vector_paths.keyframes.is_empty()
                    {
                        let shape_type = track.shape_type.last_value();
                        let size = track.size.last_value();
                        let line_from = track.line_from.last_value();
                        let line_to = track.line_to.last_value();
                        let arc_angles = track.arc_angles.last_value();
                        let color = track.color.last_value();
                        let stroke_width = track.stroke_width.last_value();
                        let stroke_color = track.stroke_color.last_value();
                        let fill_opacity = track.fill_opacity.last_value();

                        let target_vello_path = if matches!(shape_type, SHAPE_POLYGON | SHAPE_PATH)
                        {
                            let existing_path = track
                                .vector_paths
                                .last_value()
                                .first()
                                .map(|vp| vp.path.clone())
                                .unwrap_or_else(kurbo::BezPath::new);
                            styled_vello_path(
                                existing_path,
                                shape_type,
                                color,
                                stroke_width,
                                stroke_color,
                                fill_opacity,
                            )
                        } else {
                            build_shape_vello_path(
                                shape_type,
                                size,
                                line_from,
                                line_to,
                                arc_angles,
                                color,
                                stroke_width,
                                stroke_color,
                                fill_opacity,
                            )
                        };

                        if duration_ms > 0.0 {
                            let start_val = track.evaluate_vector_paths(t_start_ms);
                            track
                                .vector_paths
                                .add_keyframe(t_start_ms, start_val, Easing::Linear);
                        } else if instant_delayed {
                            preserve_instant_delayed_value(&mut track.vector_paths, t_start_ms);
                        }
                        track
                            .vector_paths
                            .add_keyframe(t_end_ms, vec![target_vello_path], easing);
                    }
                }
                Stmt::Always { body } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::LabeledAlways { label: _, body } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::ForLoop {
                    var,
                    iterable,
                    body,
                } => {
                    for value in for_iter_values(iterable, &self.env) {
                        self.env.set(var, value);
                        self.process_body(time_ms, body, parent_label, diagnostics);
                    }
                }
                Stmt::Sequence { body } => {
                    self.process_sequence(time_ms, body, parent_label, diagnostics);
                }
                Stmt::Stagger { modifiers, body } => {
                    self.process_stagger(time_ms, modifiers, body, parent_label, diagnostics);
                }
                Stmt::Action(action) => {
                    process_action(action, time_ms, self, diagnostics);
                }
                _ => {}
            }
        }
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
