pub mod actions;
pub mod env;
pub mod image;
pub mod kurbo_shapes;
pub mod morph;
pub mod svg;
pub mod track;
pub mod utils;
pub mod vello_path;

use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use actions::process_action;
pub use env::{load_standard_library, Environment, EvalError, Value};
pub use image::load_image;
pub use kurbo_shapes::{morph_kurbo_shapes, morph_kurbo_shapes_default, KurboShape_};
pub use morph::{MorphOptions, MorphStrategy};
pub use svg::parse_svg;
pub use track::{
    AnimationTrack, Interpolate, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor,
};
pub use utils::{evaluate_expr, parse_color, parse_color_in_env, time_to_ms};
pub use vello_path::VelloPath;

use crate::ast::{Expr, Modifier, Stmt};
use crate::easing::*;
use std::collections::BTreeMap;

fn sequence_stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Action(_) => "action",
        Stmt::Assignment { .. } => "assignment",
        Stmt::Sequence { .. } => "sequence",
        Stmt::LetDecl { .. } => "let declaration",
        Stmt::Text { .. } => "text declaration",
        Stmt::Math { .. } => "math declaration",
        Stmt::Code { .. } => "code declaration",
        Stmt::Svg { .. } => "svg declaration",
        Stmt::Image { .. } => "image declaration",
        Stmt::ActorDecl { .. } => "actor declaration",
        Stmt::Import { .. } => "import",
        Stmt::Use { .. } => "use",
        Stmt::Keyframe { .. } => "keyframe",
        Stmt::RelativeKeyframe { .. } => "relative keyframe",
        Stmt::Always { .. } => "always block",
        Stmt::LabeledAlways { .. } => "labeled always block",
        Stmt::Conditional { .. } => "conditional",
        Stmt::ForLoop { .. } => "for loop",
        Stmt::ComponentDef(_) => "component definition",
        Stmt::ComponentAction { .. } => "component action",
        Stmt::Config { .. } => "config block",
        Stmt::Comment(_) => "comment",
    }
}

fn push_unknown_target_path_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    target_key: &str,
) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::UnknownTargetPath,
            DiagnosticPhase::Build,
            format!(
                "Assignment target '{target_key}' does not resolve to a declared actor or nested label; ignoring this assignment."
            ),
        )
        .with_subject(subject),
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ParsedTimingModifiers {
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub easing: Easing,
    pub morph_options: MorphOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModifierHost {
    Action,
    Assignment,
    Text,
    Math,
    Code,
    ActorDeclaration,
}

impl ModifierHost {
    fn display_name(self) -> &'static str {
        match self {
            ModifierHost::Action => "action",
            ModifierHost::Assignment => "assignment",
            ModifierHost::Text => "text declaration",
            ModifierHost::Math => "math declaration",
            ModifierHost::Code => "code declaration",
            ModifierHost::ActorDeclaration => "actor declaration",
        }
    }

    fn supports_morph_modifiers(self) -> bool {
        matches!(
            self,
            ModifierHost::Text
                | ModifierHost::Math
                | ModifierHost::Code
                | ModifierHost::ActorDeclaration
        )
    }
}

fn parse_easing_name(raw: &str) -> Option<Easing> {
    match raw {
        "ease-in" => Some(Easing::EaseIn),
        "ease-out" => Some(Easing::EaseOut),
        "ease-in-out" => Some(Easing::EaseInOut),
        "bounce" => Some(Easing::Bounce),
        "linear" => Some(Easing::Linear),
        _ => None,
    }
}

fn parse_duration_literal(raw: &str) -> Option<f64> {
    if let Some(ms) = raw.strip_suffix("ms") {
        ms.parse::<f64>().ok()
    } else if let Some(seconds) = raw.strip_suffix('s') {
        seconds.parse::<f64>().ok().map(|seconds| seconds * 1000.0)
    } else {
        None
    }
}

fn push_conflicting_modifier_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    logical_name: &str,
    host: ModifierHost,
    subject: Option<&str>,
) {
    push_modifier_diagnostic(
        diagnostics,
        DiagnosticCode::ConflictingModifierKey,
        format!(
            "Conflicting '{logical_name}' modifiers on {}; using the last value provided.",
            host.display_name()
        ),
        subject,
    );
}

fn has_non_default_morph_options(options: MorphOptions) -> bool {
    options != MorphOptions::default()
}

fn push_modifier_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: String,
    subject: Option<&str>,
) {
    let diagnostic = Diagnostic::warning(code, DiagnosticPhase::Build, message);
    diagnostics.push(match subject {
        Some(subject) => diagnostic.with_subject(subject),
        None => diagnostic,
    });
}

pub(crate) fn parse_timing_modifiers(
    modifiers: &[Modifier],
    host: ModifierHost,
    subject: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> ParsedTimingModifiers {
    let mut parsed = ParsedTimingModifiers {
        duration_ms: 0.0,
        delay_ms: 0.0,
        easing: Easing::Linear,
        morph_options: MorphOptions::default(),
    };
    let mut saw_duration = false;
    let mut saw_delay = false;
    let mut saw_ease = false;
    let mut saw_strategy = false;
    let mut saw_path_arc = false;
    let mut saw_stretch = false;

    for modifier in modifiers {
        match modifier.name.as_deref() {
            Some("delay") => match &modifier.value {
                Expr::Ident(raw) => {
                    if let Some(delay_ms) = parse_duration_literal(raw) {
                        if saw_delay {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "delay",
                                host,
                                subject,
                            );
                        }
                        parsed.delay_ms = delay_ms;
                        saw_delay = true;
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported delay value '{raw}' on {}; expected a time literal such as 120ms or 1s.",
                                host.display_name()
                            ),
                            subject,
                        );
                    }
                }
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported delay modifier value {:?} on {}; expected a time literal such as 120ms or 1s.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
            },
            Some("ease") => match &modifier.value {
                Expr::Ident(raw) => {
                    if let Some(easing) = parse_easing_name(raw) {
                        if saw_ease {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "ease",
                                host,
                                subject,
                            );
                        }
                        parsed.easing = easing;
                        saw_ease = true;
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported ease value '{raw}' on {}; supported values are linear, ease-in, ease-out, ease-in-out, and bounce.",
                                host.display_name()
                            ),
                            subject,
                        );
                    }
                }
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported ease modifier value {:?} on {}; expected an easing identifier.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
            },
            Some("strategy") => {
                if !host.supports_morph_modifiers() {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::UnsupportedModifierKey,
                        format!(
                            "Unsupported modifier key 'strategy' on {}; morph-only keys are limited to path-morphing declarations.",
                            host.display_name()
                        ),
                        subject,
                    );
                    continue;
                }

                match &modifier.value {
                    Expr::Ident(raw) => {
                        if saw_strategy {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "strategy",
                                host,
                                subject,
                            );
                        }
                        match raw.as_str() {
                            "auto" => parsed.morph_options.strategy = MorphStrategy::Auto,
                            "match" => parsed.morph_options.strategy = MorphStrategy::Match,
                            "fade" => push_modifier_diagnostic(
                                diagnostics,
                                DiagnosticCode::InvalidModifierValue,
                                format!(
                                    "Unsupported strategy value 'fade' on {}; cross-fade remains deferred until the runtime has a richer transition/compositing model.",
                                    host.display_name()
                                ),
                                subject,
                            ),
                            other => push_modifier_diagnostic(
                                diagnostics,
                                DiagnosticCode::InvalidModifierValue,
                                format!(
                                    "Unsupported strategy value '{other}' on {}; supported values are auto and match.",
                                    host.display_name()
                                ),
                                subject,
                            ),
                        }
                        saw_strategy = true;
                    }
                    other => push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported strategy modifier value {:?} on {}; expected an identifier such as auto or match.",
                            other,
                            host.display_name()
                        ),
                        subject,
                    ),
                }
            }
            Some("path_arc") => {
                if !host.supports_morph_modifiers() {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::UnsupportedModifierKey,
                        format!(
                            "Unsupported modifier key 'path_arc' on {}; morph-only keys are limited to path-morphing declarations.",
                            host.display_name()
                        ),
                        subject,
                    );
                    continue;
                }

                let parsed_arc = match &modifier.value {
                    Expr::Num(value) => Some(*value),
                    Expr::Ident(raw) => raw.parse::<f64>().ok(),
                    _ => None,
                };

                if let Some(path_arc) = parsed_arc {
                    if saw_path_arc {
                        push_conflicting_modifier_diagnostic(
                            diagnostics,
                            "path_arc",
                            host,
                            subject,
                        );
                    }
                    parsed.morph_options.path_arc = path_arc;
                    saw_path_arc = true;
                } else {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported path_arc value on {}; expected a numeric radians hint.",
                            host.display_name()
                        ),
                        subject,
                    );
                }
            }
            Some("stretch") => {
                if !host.supports_morph_modifiers() {
                    push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::UnsupportedModifierKey,
                        format!(
                            "Unsupported modifier key 'stretch' on {}; morph-only keys are limited to path-morphing declarations.",
                            host.display_name()
                        ),
                        subject,
                    );
                    continue;
                }

                match &modifier.value {
                    Expr::Bool(value) => {
                        if saw_stretch {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "stretch",
                                host,
                                subject,
                            );
                        }
                        parsed.morph_options.stretch = *value;
                        saw_stretch = true;
                    }
                    Expr::Ident(raw) if raw == "true" || raw == "false" => {
                        if saw_stretch {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "stretch",
                                host,
                                subject,
                            );
                        }
                        parsed.morph_options.stretch = raw == "true";
                        saw_stretch = true;
                    }
                    other => push_modifier_diagnostic(
                        diagnostics,
                        DiagnosticCode::InvalidModifierValue,
                        format!(
                            "Unsupported stretch modifier value {:?} on {}; expected true or false.",
                            other,
                            host.display_name()
                        ),
                        subject,
                    ),
                }
            }
            Some(name) => push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::UnsupportedModifierKey,
                format!(
                    "Unsupported modifier key '{name}' on {}; this host currently supports positional duration shorthand, named delay, and named ease.",
                    host.display_name()
                ),
                subject,
            ),
            None => match &modifier.value {
                Expr::Ident(raw) => {
                    if let Some(duration_ms) = parse_duration_literal(raw) {
                        if saw_duration {
                            push_conflicting_modifier_diagnostic(
                                diagnostics,
                                "duration",
                                host,
                                subject,
                            );
                        }
                        parsed.duration_ms = duration_ms;
                        saw_duration = true;
                    } else if parse_easing_name(raw).is_some() {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Use named syntax like [ease: {raw}] on {}; bare modifiers are reserved for duration values such as 2s or 500ms.",
                                host.display_name()
                            ),
                            subject,
                        );
                    } else {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            format!(
                                "Unsupported duration shorthand '{raw}' on {}; expected a bare time literal such as 2s or 500ms.",
                                host.display_name()
                            ),
                            subject,
                        );
                    }
                }
                other => push_modifier_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    format!(
                        "Unsupported positional modifier value {:?} on {}; expected a bare duration like 2s or 500ms.",
                        other,
                        host.display_name()
                    ),
                    subject,
                ),
            },
        }
    }

    parsed
}

fn sample_recursive_cartesian(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    pts: &mut Vec<kurbo::Point>,
) {
    let screen_height = p_size[1];

    let margin_y = screen_height * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    if (p0.y < min_screen_y && p1.y < min_screen_y) || (p0.y > max_screen_y && p1.y > max_screen_y)
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dx = (p1.x - p0.x).abs();
    let dy = (p1.y - p0.y).abs();
    if dx > 0.0 && (dy / dx) > 1000.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    // Discontinuity detection (steep slope)
    let dx = (p1.x - p0.x).abs();
    let dy = (p1.y - p0.y).abs();
    if dx > 0.0 && (dy / dx) > 1000.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    env.set(arg_name, Value::Num(mid_t));
    let val = evaluate_expr(body, env).unwrap_or(Value::Num(0.0)).as_num();

    let math_x = mid_t;
    let math_y = val;

    let screen_x = -(p_size[0] / 2.0)
        + p_size[0] * ((math_x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y = (p_size[1] / 2.0)
        - p_size[1] * ((math_y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_cartesian(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            pts,
        );
        sample_recursive_cartesian(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

fn sample_recursive_polar(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    pts: &mut Vec<kurbo::Point>,
) {
    let margin_y = p_size[1] * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > (p_size[0].max(p_size[1])).powi(2) * 4.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    env.set(arg_name, Value::Num(mid_t));
    let val = evaluate_expr(body, env).unwrap_or(Value::Num(0.0)).as_num();

    let math_x = val * mid_t.cos();
    let math_y = val * mid_t.sin();

    let screen_x = -(p_size[0] / 2.0)
        + p_size[0] * ((math_x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y = (p_size[1] / 2.0)
        - p_size[1] * ((math_y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_polar(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            pts,
        );
        sample_recursive_polar(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

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

impl Default for SceneDimensions {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

fn is_layout_container(container_ty: &str) -> bool {
    matches!(container_ty, "Row" | "Col" | "Stack" | "Grid")
}

fn parse_scene_anchor(expr: &Expr) -> Option<SceneAnchor> {
    match expr {
        Expr::Path(parts) if parts.len() == 2 && parts[0] == "scene" => match parts[1].as_str() {
            "top_left" => Some(SceneAnchor::TopLeft),
            "top" => Some(SceneAnchor::Top),
            "top_right" => Some(SceneAnchor::TopRight),
            "left" => Some(SceneAnchor::Left),
            "center" => Some(SceneAnchor::Center),
            "right" => Some(SceneAnchor::Right),
            "bottom_left" => Some(SceneAnchor::BottomLeft),
            "bottom" => Some(SceneAnchor::Bottom),
            "bottom_right" => Some(SceneAnchor::BottomRight),
            _ => None,
        },
        _ => None,
    }
}

fn parse_numeric_vec2(expr: &Expr, env: &Environment) -> Option<[f32; 2]> {
    match evaluate_expr(expr, env).ok()? {
        Value::Vec2([x, y]) => Some([x as f32, y as f32]),
        _ => None,
    }
}

fn parse_percent_vec2(expr: &Expr) -> Option<[f32; 2]> {
    match expr {
        Expr::Tuple(items) if items.len() == 2 => match (&items[0], &items[1]) {
            (Expr::Percent(x), Expr::Percent(y)) => {
                Some([(*x as f32) / 100.0, (*y as f32) / 100.0])
            }
            _ => None,
        },
        _ => None,
    }
}

fn resolve_position_binding(
    at_expr: Option<&Expr>,
    anchor_expr: Option<&Expr>,
    offset_expr: Option<&Expr>,
    env: &Environment,
) -> Option<(PositionBinding, Option<[f32; 2]>)> {
    let offset = offset_expr
        .and_then(|expr| parse_numeric_vec2(expr, env))
        .unwrap_or([0.0, 0.0]);

    if let Some(anchor_expr) = anchor_expr {
        if let Some(anchor) = parse_scene_anchor(anchor_expr) {
            return Some((PositionBinding::SceneAnchor { anchor, offset }, None));
        }
    }

    if let Some(at_expr) = at_expr {
        if let Some(anchor) = parse_scene_anchor(at_expr) {
            return Some((PositionBinding::SceneAnchor { anchor, offset }, None));
        }

        if let Some([x, y]) = parse_percent_vec2(at_expr) {
            return Some((PositionBinding::ScenePercent { x, y, offset }, None));
        }

        if let Some(position) = parse_numeric_vec2(at_expr, env) {
            return Some((PositionBinding::Absolute, Some(position)));
        }
    }

    None
}

fn assignment_target_key(target: &[String]) -> String {
    target.join(".")
}

fn for_iter_values(iterable: &Expr, env: &Environment) -> Vec<Value> {
    match iterable {
        Expr::Tuple(items) => items
            .iter()
            .filter_map(|item| evaluate_expr(item, env).ok())
            .collect(),
        _ => match evaluate_expr(iterable, env) {
            Ok(Value::Vec2([start, end])) => {
                let start = start as i64;
                let end = end as i64;
                (start..end).map(|i| Value::Num(i as f64)).collect()
            }
            Ok(Value::Vec3(values)) => values.into_iter().map(Value::Num).collect(),
            Ok(Value::Vec4(values)) => values.into_iter().map(Value::Num).collect(),
            Ok(value) => vec![value],
            Err(_) => Vec::new(),
        },
    }
}

fn set_lookup_scalar(env: &mut Environment, key: &str, value: f64) {
    env.set(key, Value::Num(value));
}

fn set_lookup_vec2(env: &mut Environment, key: &str, value: [f64; 2]) {
    env.set(key, Value::Vec2(value));
    env.set(&format!("{}.x", key), Value::Num(value[0]));
    env.set(&format!("{}.y", key), Value::Num(value[1]));
}

fn set_lookup_color(env: &mut Environment, key: &str, value: [f64; 4]) {
    env.set(key, Value::Color(value));
    env.set(&format!("{}.r", key), Value::Num(value[0]));
    env.set(&format!("{}.g", key), Value::Num(value[1]));
    env.set(&format!("{}.b", key), Value::Num(value[2]));
    env.set(&format!("{}.a", key), Value::Num(value[3]));
}

fn scene_anchor_point(anchor: SceneAnchor, scene_dimensions: SceneDimensions) -> kurbo::Point {
    let width = scene_dimensions.width as f64;
    let height = scene_dimensions.height as f64;
    match anchor {
        SceneAnchor::TopLeft => kurbo::Point::new(0.0, 0.0),
        SceneAnchor::Top => kurbo::Point::new(width / 2.0, 0.0),
        SceneAnchor::TopRight => kurbo::Point::new(width, 0.0),
        SceneAnchor::Left => kurbo::Point::new(0.0, height / 2.0),
        SceneAnchor::Center => kurbo::Point::new(width / 2.0, height / 2.0),
        SceneAnchor::Right => kurbo::Point::new(width, height / 2.0),
        SceneAnchor::BottomLeft => kurbo::Point::new(0.0, height),
        SceneAnchor::Bottom => kurbo::Point::new(width / 2.0, height),
        SceneAnchor::BottomRight => kurbo::Point::new(width, height),
    }
}

fn resolve_bound_position(
    binding: PositionBinding,
    base_position: [f32; 2],
    parent_transform: kurbo::Affine,
    scene_dimensions: SceneDimensions,
) -> [f32; 2] {
    let scene_point = match binding {
        PositionBinding::Absolute => return base_position,
        PositionBinding::SceneAnchor { anchor, offset } => {
            let point = scene_anchor_point(anchor, scene_dimensions);
            kurbo::Point::new(point.x + offset[0] as f64, point.y + offset[1] as f64)
        }
        PositionBinding::ScenePercent { x, y, offset } => kurbo::Point::new(
            scene_dimensions.width as f64 * x as f64 + offset[0] as f64,
            scene_dimensions.height as f64 * y as f64 + offset[1] as f64,
        ),
        PositionBinding::ContainerDefault { anchor } => {
            scene_anchor_point(anchor, scene_dimensions)
        }
    };

    let local_point = parent_transform.inverse() * scene_point;
    [local_point.x as f32, local_point.y as f32]
}

const SHAPE_RECT: u32 = 0;
const SHAPE_CIRCLE: u32 = 1;
const SHAPE_LINE: u32 = 2;
const SHAPE_ELLIPSE: u32 = 3;
const SHAPE_ARC: u32 = 4;
const SHAPE_POLYGON: u32 = 5;
const SHAPE_PATH: u32 = 6;

fn shape_type_for_actor(ty: &str) -> u32 {
    match ty {
        "Circle" => SHAPE_CIRCLE,
        "Line" => SHAPE_LINE,
        "Ellipse" => SHAPE_ELLIPSE,
        "Arc" => SHAPE_ARC,
        "Polygon" => SHAPE_POLYGON,
        "Path" => SHAPE_PATH,
        _ => SHAPE_RECT,
    }
}

fn parse_point_list_expr(expr: &Expr, env: &Environment) -> Option<Vec<kurbo::Point>> {
    match expr {
        Expr::Tuple(items) => {
            let mut points = Vec::with_capacity(items.len());
            for item in items {
                let [x, y] = parse_numeric_vec2(item, env)?;
                points.push(kurbo::Point::new(x as f64, y as f64));
            }
            Some(points)
        }
        _ => None,
    }
}

fn parse_path_commands_expr(expr: &Expr, env: &Environment) -> Option<kurbo::BezPath> {
    let Expr::Tuple(items) = expr else {
        return None;
    };

    let mut path = kurbo::BezPath::new();

    for item in items {
        let Expr::Call(name, args) = item else {
            return None;
        };

        match name.as_str() {
            "move_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.move_to((x, y));
            }
            "line_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.line_to((x, y));
            }
            "quad_to" => {
                if args.len() != 4 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                path.quad_to((x1, y1), (x2, y2));
            }
            "curve_to" => {
                if args.len() != 6 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                let x3 = evaluate_expr(&args[4], env).ok()?.as_num();
                let y3 = evaluate_expr(&args[5], env).ok()?.as_num();
                path.curve_to((x1, y1), (x2, y2), (x3, y3));
            }
            "close" => {
                if !args.is_empty() {
                    return None;
                }
                path.close_path();
            }
            _ => return None,
        }
    }

    Some(path)
}

fn build_shape(
    shape_type: u32,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
) -> KurboShape_ {
    match shape_type {
        SHAPE_CIRCLE => KurboShape_::Circle {
            center: kurbo::Point::new(0.0, 0.0),
            radius: size[0] as f64,
        },
        SHAPE_LINE => KurboShape_::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        SHAPE_ELLIPSE => KurboShape_::Ellipse {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
            rotation: 0.0,
        },
        SHAPE_ARC => KurboShape_::Arc {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
            start_angle: arc_angles[0] as f64,
            sweep_angle: arc_angles[1] as f64,
            rotation: 0.0,
        },
        _ => KurboShape_::Rect {
            x0: -(size[0] as f64),
            y0: -(size[1] as f64),
            x1: size[0] as f64,
            y1: size[1] as f64,
        },
    }
}

fn shape_fill_color(
    shape_type: u32,
    color: [f32; 4],
    fill_opacity: f32,
) -> Option<vello::peniko::Color> {
    if matches!(shape_type, SHAPE_LINE | SHAPE_ARC) || fill_opacity <= 0.0 {
        return None;
    }

    Some(vello::peniko::Color::from_rgba8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0 * fill_opacity) as u8,
    ))
}

fn shape_stroke(stroke_color: [f32; 4], stroke_width: f32) -> Option<(vello::peniko::Color, f32)> {
    if stroke_width <= 0.0 {
        return None;
    }

    Some((
        vello::peniko::Color::from_rgba8(
            (stroke_color[0] * 255.0) as u8,
            (stroke_color[1] * 255.0) as u8,
            (stroke_color[2] * 255.0) as u8,
            (stroke_color[3] * 255.0) as u8,
        ),
        stroke_width,
    ))
}

fn build_shape_vello_path(
    shape_type: u32,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    fill_opacity: f32,
) -> VelloPath {
    let shape = build_shape(shape_type, size, line_from, line_to, arc_angles);

    VelloPath {
        path: shape.to_path_default(),
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
    }
}

fn styled_vello_path(
    path: kurbo::BezPath,
    shape_type: u32,
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    fill_opacity: f32,
) -> VelloPath {
    VelloPath {
        path,
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
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
        }
    }

    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_diagnostics(ast).output
    }

    pub fn build_with_diagnostics(ast: &[Stmt]) -> BuildReport<Self> {
        let mut timeline = Self::new();
        load_standard_library(&mut timeline.env);
        let mut current_time_ms = 0.0;
        let mut diagnostics = Vec::new();

        for stmt in ast {
            match stmt {
                Stmt::Keyframe { time, body } => {
                    current_time_ms = time_to_ms(time);
                    timeline.process_body(current_time_ms, body, None, &mut diagnostics);
                }
                Stmt::RelativeKeyframe { offset, body } => {
                    current_time_ms += time_to_ms(offset);
                    timeline.process_body(current_time_ms, body, None, &mut diagnostics);
                }
                Stmt::ActorDecl { .. } | Stmt::Assignment { .. } | Stmt::Sequence { .. } => {
                    timeline.process_body(current_time_ms, &[stmt.clone()], None, &mut diagnostics);
                }
                _ => {}
            }
        }
        BuildReport::new(timeline, diagnostics)
    }

    fn sequence_statement_span_ms(&self, stmt: &Stmt) -> Option<f64> {
        let mut ignored_diagnostics = Vec::new();
        match stmt {
            Stmt::Action(action) => {
                let parsed = parse_timing_modifiers(
                    &action.modifiers,
                    ModifierHost::Action,
                    Some(&action.verb),
                    &mut ignored_diagnostics,
                );
                Some(parsed.delay_ms + parsed.duration_ms)
            }
            Stmt::Assignment {
                target,
                property,
                modifiers,
                ..
            } => {
                let subject = format!("{}.{}", target.join("."), property);
                let parsed = parse_timing_modifiers(
                    modifiers,
                    ModifierHost::Assignment,
                    Some(&subject),
                    &mut ignored_diagnostics,
                );
                Some(parsed.delay_ms + parsed.duration_ms)
            }
            _ => None,
        }
    }

    fn process_sequence(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut cursor_time_ms = time_ms;

        for stmt in body {
            let Some(span_ms) = self.sequence_statement_span_ms(stmt) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnsupportedSequenceStatement,
                        DiagnosticPhase::Build,
                        format!(
                            "Sequence blocks currently support only actions and assignments; '{}' is not supported in sequence v1a.",
                            sequence_stmt_kind(stmt)
                        ),
                    )
                    .with_subject("sequence"),
                );
                continue;
            };

            self.process_body(cursor_time_ms, &[stmt.clone()], parent_label, diagnostics);
            cursor_time_ms += span_ms;
        }
    }

    fn build_eval_env(&self, time_ms: u64) -> Environment {
        let mut env = self.env.clone();
        self.inject_runtime_lookup_values(&mut env, time_ms, None, None);
        env
    }

    fn frame_eval_env(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Environment {
        let mut env = self.env.clone();
        env.set("t", Value::Num(time_ms as f64 / 1000.0));
        env.set("scene_width", Value::Num(scene_dimensions.width as f64));
        env.set("scene_height", Value::Num(scene_dimensions.height as f64));
        self.inject_runtime_lookup_values(
            &mut env,
            time_ms,
            Some(scene_dimensions),
            Some(overrides),
        );
        env
    }

    pub fn frame(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Environment {
        self.frame_eval_env(time_ms, scene_dimensions, overrides)
    }

    fn inject_runtime_lookup_values(
        &self,
        env: &mut Environment,
        time_ms: u64,
        scene_dimensions: Option<SceneDimensions>,
        overrides: Option<
            &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        >,
    ) {
        let background_color = overrides
            .and_then(|map| map.get("scene"))
            .and_then(|props| props.get("background_color"))
            .and_then(|value| match value {
                Value::Color(c) => Some(*c),
                Value::Vec4(c) => Some(*c),
                _ => None,
            })
            .unwrap_or_else(|| {
                let [r, g, b, a] = self.background_color.evaluate(time_ms);
                [r as f64, g as f64, b as f64, a as f64]
            });
        set_lookup_color(env, "scene.background_color", background_color);

        if let Some(dimensions) = scene_dimensions {
            for (suffix, anchor) in [
                ("top_left", SceneAnchor::TopLeft),
                ("top", SceneAnchor::Top),
                ("top_right", SceneAnchor::TopRight),
                ("left", SceneAnchor::Left),
                ("center", SceneAnchor::Center),
                ("right", SceneAnchor::Right),
                ("bottom_left", SceneAnchor::BottomLeft),
                ("bottom", SceneAnchor::Bottom),
                ("bottom_right", SceneAnchor::BottomRight),
            ] {
                let point = scene_anchor_point(anchor, dimensions);
                set_lookup_vec2(env, &format!("scene.{}", suffix), [point.x, point.y]);
            }
        }

        for (label, track) in &self.tracks {
            let node_overrides = overrides.and_then(|map| map.get(label));
            let motion_offset = track.motion_offset.evaluate(time_ms);
            let rotation = track.rotation.evaluate(time_ms) as f64;
            let scale = track.scale.evaluate(time_ms) as f64;
            let base_position = node_overrides
                .and_then(|props| props.get("at").or_else(|| props.get("position")))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [x, y] = track.position.evaluate(time_ms);
                    [x as f64, y as f64]
                });
            let position = [
                base_position[0] + motion_offset[0] as f64,
                base_position[1] + motion_offset[1] as f64,
            ];
            set_lookup_vec2(env, &format!("{}.at", label), position);
            env.set(&format!("{}.position", label), Value::Vec2(position));
            set_lookup_vec2(
                env,
                &format!("{}.shift", label),
                [motion_offset[0] as f64, motion_offset[1] as f64],
            );
            set_lookup_scalar(env, &format!("{}.rotation", label), rotation);
            set_lookup_scalar(env, &format!("{}.scale", label), scale);

            let size = node_overrides
                .and_then(|props| props.get("size"))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [w, h] = track.size.evaluate(time_ms);
                    [w as f64 * 2.0, h as f64 * 2.0]
                });
            set_lookup_vec2(env, &format!("{}.size", label), size);
            set_lookup_scalar(env, &format!("{}.width", label), size[0]);
            set_lookup_scalar(env, &format!("{}.height", label), size[1]);

            let radius_x = node_overrides
                .and_then(|props| props.get("radius_x"))
                .map(Value::as_num)
                .unwrap_or(size[0] / 2.0);
            let radius_y = node_overrides
                .and_then(|props| props.get("radius_y"))
                .map(Value::as_num)
                .unwrap_or(size[1] / 2.0);
            let radius = node_overrides
                .and_then(|props| props.get("radius"))
                .map(Value::as_num)
                .unwrap_or(radius_x);
            set_lookup_scalar(env, &format!("{}.radius", label), radius);
            set_lookup_scalar(env, &format!("{}.radius_x", label), radius_x);
            set_lookup_scalar(env, &format!("{}.radius_y", label), radius_y);

            let color = node_overrides
                .and_then(|props| props.get("color"))
                .and_then(|value| match value {
                    Value::Color(c) => Some(*c),
                    Value::Vec4(c) => Some(*c),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [r, g, b, a] = track.color.evaluate(time_ms);
                    [r as f64, g as f64, b as f64, a as f64]
                });
            set_lookup_color(env, &format!("{}.color", label), color);

            let stroke_color = node_overrides
                .and_then(|props| props.get("stroke_color").or_else(|| props.get("stroke")))
                .and_then(|value| match value {
                    Value::Color(c) => Some(*c),
                    Value::Vec4(c) => Some(*c),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [r, g, b, a] = track.stroke_color.evaluate(time_ms);
                    [r as f64, g as f64, b as f64, a as f64]
                });
            set_lookup_color(env, &format!("{}.stroke_color", label), stroke_color);

            let opacity = node_overrides
                .and_then(|props| props.get("opacity"))
                .map(Value::as_num)
                .unwrap_or(track.opacity.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.opacity", label), opacity);

            let fill_opacity = node_overrides
                .and_then(|props| props.get("fill_opacity"))
                .map(Value::as_num)
                .unwrap_or(track.fill_opacity.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.fill_opacity", label), fill_opacity);

            let stroke_width = node_overrides
                .and_then(|props| props.get("stroke_width").or_else(|| props.get("width")))
                .map(Value::as_num)
                .unwrap_or(track.stroke_width.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.stroke_width", label), stroke_width);

            let stroke_progress = node_overrides
                .and_then(|props| props.get("stroke_progress"))
                .map(Value::as_num)
                .unwrap_or(track.stroke_progress.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.stroke_progress", label), stroke_progress);

            let from = node_overrides
                .and_then(|props| props.get("from"))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [x, y] = track.line_from.evaluate(time_ms);
                    [x as f64, y as f64]
                });
            set_lookup_vec2(env, &format!("{}.from", label), from);

            let to = node_overrides
                .and_then(|props| props.get("to"))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [x, y] = track.line_to.evaluate(time_ms);
                    [x as f64, y as f64]
                });
            set_lookup_vec2(env, &format!("{}.to", label), to);

            let start_angle = node_overrides
                .and_then(|props| props.get("start_angle"))
                .map(Value::as_num)
                .unwrap_or(track.arc_angles.evaluate(time_ms)[0] as f64);
            let sweep_angle = node_overrides
                .and_then(|props| props.get("sweep_angle"))
                .map(Value::as_num)
                .unwrap_or(track.arc_angles.evaluate(time_ms)[1] as f64);
            set_lookup_scalar(env, &format!("{}.start_angle", label), start_angle);
            set_lookup_scalar(env, &format!("{}.sweep_angle", label), sweep_angle);
        }
    }

    fn apply_modifier_stmt(
        &self,
        stmt: &Stmt,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) {
        match stmt {
            Stmt::Assignment {
                target,
                property,
                value,
                ..
            } => {
                if let Ok(val) = evaluate_expr(value, frame_env) {
                    overrides
                        .entry(assignment_target_key(target))
                        .or_default()
                        .insert(property.clone(), val);
                    *frame_env = self.frame_eval_env(time_ms, scene_dimensions, overrides);
                }
            }
            Stmt::LetDecl { name, value } => {
                if let Ok(val) = evaluate_expr(value, frame_env) {
                    frame_env.set(name, val);
                }
            }
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                if evaluate_expr(condition, frame_env)
                    .map(|value| value.as_num() != 0.0)
                    .unwrap_or(false)
                {
                    for stmt in then_branch {
                        self.apply_modifier_stmt(
                            stmt,
                            time_ms,
                            scene_dimensions,
                            frame_env,
                            overrides,
                        );
                    }
                } else if let Some(else_branch) = else_branch {
                    for stmt in else_branch {
                        self.apply_modifier_stmt(
                            stmt,
                            time_ms,
                            scene_dimensions,
                            frame_env,
                            overrides,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    pub fn apply_modifier_stmt_for_test(
        &self,
        stmt: &Stmt,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) {
        self.apply_modifier_stmt(stmt, time_ms, scene_dimensions, frame_env, overrides)
    }

    pub fn apply_modifier_ir_program(
        &self,
        program: &crate::ir::ModifierIrProgram,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        crate::ir::execute_modifier_ir(program, frame_env, overrides, |frame_env, overrides| {
            *frame_env = self.frame_eval_env(time_ms, scene_dimensions, overrides);
        })
    }

    pub fn apply_modifier_bytecode_program(
        &self,
        program: &crate::vm::ModifierBytecodeProgram,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        crate::vm::execute_modifier_bytecode(
            program,
            frame_env,
            overrides,
            |frame_env, overrides| {
                *frame_env = self.frame_eval_env(time_ms, scene_dimensions, overrides);
            },
        )
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
    fn apply_container_layout(
        &mut self,
        container_label: &str,
        container_ty: &str,
        time_ms: f64,
        gap: f32,
        align: Option<&str>,
        cols: Option<usize>,
    ) {
        let children = if let Some(node) = self.nodes.get(container_label) {
            node.children.clone()
        } else {
            return;
        };

        let is_row = container_ty == "Row";
        let is_col = container_ty == "Col";
        let is_stack = container_ty == "Stack";
        let is_grid = container_ty == "Grid";

        if !is_row && !is_col && !is_stack && !is_grid {
            return;
        }

        let child_extents: Vec<(f32, f32)> = children
            .iter()
            .filter_map(|cl| {
                self.tracks.get(cl).map(|t| {
                    let s = t.size.last_value();
                    (s[0] * 2.0, s[1] * 2.0)
                })
            })
            .collect();

        let t_ms = time_ms as u64;

        if is_stack {
            for child_label in &children {
                if let Some(track) = self.tracks.get_mut(child_label) {
                    if track.placement_mode.last_value() == PlacementMode::LayoutManaged {
                        track
                            .position
                            .add_keyframe(t_ms, [0.0, 0.0], Easing::Linear);
                    }
                }
            }
            return;
        }

        if is_grid {
            let cols = cols.unwrap_or(1).max(1);
            let rows = children.len().div_ceil(cols);
            let mut col_widths = vec![0.0f32; cols];
            let mut row_heights = vec![0.0f32; rows.max(1)];

            for (index, (child_w, child_h)) in child_extents.iter().copied().enumerate() {
                let row = index / cols;
                let col = index % cols;
                col_widths[col] = col_widths[col].max(child_w);
                row_heights[row] = row_heights[row].max(child_h);
            }

            let total_width =
                col_widths.iter().sum::<f32>() + gap * (col_widths.len().saturating_sub(1) as f32);
            let total_height = row_heights.iter().sum::<f32>()
                + gap * (row_heights.len().saturating_sub(1) as f32);

            let mut row_starts = Vec::with_capacity(row_heights.len());
            let mut current_y = -total_height / 2.0;
            for row_height in &row_heights {
                row_starts.push(current_y);
                current_y += *row_height + gap;
            }

            let mut col_starts = Vec::with_capacity(col_widths.len());
            let mut current_x = -total_width / 2.0;
            for col_width in &col_widths {
                col_starts.push(current_x);
                current_x += *col_width + gap;
            }

            for (index, child_label) in children.iter().enumerate() {
                if let Some(track) = self.tracks.get_mut(child_label) {
                    if track.placement_mode.last_value() != PlacementMode::LayoutManaged {
                        continue;
                    }

                    let row = index / cols;
                    let col = index % cols;
                    if row >= row_heights.len() || col >= col_widths.len() {
                        continue;
                    }

                    let x = col_starts[col] + col_widths[col] / 2.0;
                    let y = row_starts[row] + row_heights[row] / 2.0;
                    track.position.add_keyframe(t_ms, [x, y], Easing::Linear);
                }
            }
            return;
        }

        // Pre-compute total content extent to support alignment.
        // For Row: total width; for Col: total height.
        let mut total_extent = 0.0f32;
        let mut max_cross_extent = 0.0f32; // max height for Row, max width for Col
        for (w, h) in child_extents.iter().copied() {
            if is_row {
                total_extent += w;
                if max_cross_extent < h {
                    max_cross_extent = h;
                }
            } else {
                total_extent += h;
                if max_cross_extent < w {
                    max_cross_extent = w;
                }
            }
        }

        // Add gaps between children
        if !children.is_empty() && children.len() > 1 {
            total_extent += gap * (children.len() as f32 - 1.0);
        }

        // Determine the offset for the perpendicular axis alignment
        let cross_offset = match align.unwrap_or("center") {
            "start" => {
                if is_row {
                    -max_cross_extent / 2.0
                } else {
                    -max_cross_extent / 2.0
                }
            }
            "end" => {
                if is_row {
                    max_cross_extent / 2.0
                } else {
                    max_cross_extent / 2.0
                }
            }
            _ => 0.0,
        };

        // Compute the starting offset along the main axis (centered within container)
        let main_start = -total_extent / 2.0;

        let mut offset = 0.0f32;

        for (i, child_label) in children.iter().enumerate() {
            if let Some(track) = self.tracks.get_mut(child_label) {
                let (child_w, child_h) = child_extents[i];

                let (x, y) = if is_row {
                    // Main axis: X; cross axis: Y
                    let cx = main_start + offset + child_w / 2.0;
                    offset += child_w;
                    if i < children.len() - 1 {
                        offset += gap;
                    }
                    let cy = match align.unwrap_or("center") {
                        "start" => cross_offset + child_h / 2.0, // top
                        "end" => cross_offset - child_h / 2.0,   // bottom
                        _ => cross_offset,                       // center
                    };
                    (cx, cy)
                } else {
                    // Col: main axis: Y; cross axis: X
                    let cy = main_start + offset + child_h / 2.0;
                    offset += child_h;
                    if i < children.len() - 1 {
                        offset += gap;
                    }
                    let cx = match align.unwrap_or("center") {
                        "start" => cross_offset + child_w / 2.0, // left
                        "end" => cross_offset - child_w / 2.0,   // right
                        _ => cross_offset,                       // center
                    };
                    (cx, cy)
                };

                let placement_mode = track.placement_mode.last_value();
                if placement_mode == PlacementMode::LayoutManaged {
                    track.position.add_keyframe(t_ms, [x, y], Easing::Linear);
                }
            }
        }
    }

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
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str.clone()));
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
                    let supports_morph_options =
                        !track.text_paths.keyframes.is_empty() && duration_ms > 0.0;

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
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;

                    for prop in props {
                        match prop.name.as_str() {
                            "text" => {
                                text_content = evaluate_expr(&prop.value, &eval_env)
                                    .map(|v| v.as_str())
                                    .unwrap_or_default();
                            }
                            "font_size" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let c = parse_color_in_env(&prop.value, &eval_env);
                                color = typst::visualize::Color::from_u8(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    (c[3] * 255.0) as u8,
                                );
                                if delay_ms > 0.0 && duration_ms == 0.0 {
                                    preserve_instant_delayed_value(&mut track.color, t_start_ms);
                                }
                                track.color.add_keyframe(t_start_ms, c, Easing::Linear);
                            }
                            "at" => {
                                at_expr = Some(prop.value.clone());
                            }
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    if let Some((binding, position)) = resolve_position_binding(
                        at_expr.as_ref(),
                        anchor_expr.as_ref(),
                        offset_expr.as_ref(),
                        &eval_env,
                    ) {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_discrete_position_state_before(track, t_start_ms);
                            preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        }
                        apply_explicit_position_binding(track, t_start_ms, binding, position);
                    }

                    let frame =
                        crate::renderer::text::compile_text(&text_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);

                    if duration_ms > 0.0 {
                        let start_val = track.evaluate_text_paths(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                }
                Stmt::Math {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_math".to_string());
                    let eval_env = self.build_eval_env(time_ms as u64);
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str.clone()));
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
                    let supports_morph_options =
                        !track.text_paths.keyframes.is_empty() && duration_ms > 0.0;

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
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;

                    for prop in props {
                        match prop.name.as_str() {
                            "latex" | "math" => {
                                latex_content = evaluate_expr(&prop.value, &eval_env)
                                    .map(|v| v.as_str())
                                    .unwrap_or_default();
                            }
                            "font_size" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let c = parse_color_in_env(&prop.value, &eval_env);
                                color = typst::visualize::Color::from_u8(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    (c[3] * 255.0) as u8,
                                );
                                if delay_ms > 0.0 && duration_ms == 0.0 {
                                    preserve_instant_delayed_value(&mut track.color, t_start_ms);
                                }
                                track.color.add_keyframe(t_start_ms, c, Easing::Linear);
                            }
                            "at" => {
                                at_expr = Some(prop.value.clone());
                            }
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    if let Some((binding, position)) = resolve_position_binding(
                        at_expr.as_ref(),
                        anchor_expr.as_ref(),
                        offset_expr.as_ref(),
                        &eval_env,
                    ) {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_discrete_position_state_before(track, t_start_ms);
                            preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        }
                        apply_explicit_position_binding(track, t_start_ms, binding, position);
                    }

                    let frame =
                        crate::renderer::text::compile_math(&latex_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);

                    if duration_ms > 0.0 {
                        let start_val = track.evaluate_text_paths(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
                }
                Stmt::Code {
                    label,
                    props,
                    modifiers,
                } => {
                    let label_str = label.clone().unwrap_or_else(|| "unnamed_code".to_string());
                    let eval_env = self.build_eval_env(time_ms as u64);
                    self.add_node(label_str.clone(), parent_label);
                    let track = self
                        .tracks
                        .entry(label_str.clone())
                        .or_insert_with(|| AnimationTrack::new(label_str.clone()));
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
                    let supports_morph_options =
                        !track.text_paths.keyframes.is_empty() && duration_ms > 0.0;

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
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;

                    for prop in props {
                        match prop.name.as_str() {
                            "code" => {
                                code_content = evaluate_expr(&prop.value, &eval_env)
                                    .map(|v| v.as_str())
                                    .unwrap_or_default();
                            }
                            "font_size" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                font_size = v.as_num() as f32;
                            }
                            "color" => {
                                let c = parse_color_in_env(&prop.value, &eval_env);
                                color = typst::visualize::Color::from_u8(
                                    (c[0] * 255.0) as u8,
                                    (c[1] * 255.0) as u8,
                                    (c[2] * 255.0) as u8,
                                    (c[3] * 255.0) as u8,
                                );
                                if delay_ms > 0.0 && duration_ms == 0.0 {
                                    preserve_instant_delayed_value(&mut track.color, t_start_ms);
                                }
                                track.color.add_keyframe(t_start_ms, c, Easing::Linear);
                            }
                            "at" => {
                                at_expr = Some(prop.value.clone());
                            }
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    if let Some((binding, position)) = resolve_position_binding(
                        at_expr.as_ref(),
                        anchor_expr.as_ref(),
                        offset_expr.as_ref(),
                        &eval_env,
                    ) {
                        if delay_ms > 0.0 && duration_ms == 0.0 {
                            preserve_discrete_position_state_before(track, t_start_ms);
                            preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        }
                        apply_explicit_position_binding(track, t_start_ms, binding, position);
                    }

                    let frame =
                        crate::renderer::text::compile_code(&code_content, font_size, color);
                    let new_paths = crate::renderer::text::extract_glyphs(&frame);

                    if duration_ms > 0.0 {
                        let start_val = track.evaluate_text_paths(t_start_ms);
                        track
                            .text_paths
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }
                    track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
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
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;
                    let initial_eval_env = self.build_eval_env(time_ms as u64);

                    for prop in props {
                        match prop.name.as_str() {
                            "size" => {
                                let size_val = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    initial_size[0] = w as f32 / 2.0;
                                    initial_size[1] = h as f32 / 2.0;
                                }
                            }
                            "radius" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                initial_size = [r, r];
                            }
                            "x_domain" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    x_domain = [min, max];
                                }
                            }
                            "y_domain" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    y_domain = [min, max];
                                }
                            }
                            "t_domain" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    t_domain = [min, max];
                                }
                            }
                            "func" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Closure(args, body) = v {
                                    func = Some((args, body));
                                }
                            }
                            "tolerance" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                tolerance = v.as_num();
                            }
                            "max_depth" => {
                                let v = evaluate_expr(&prop.value, &initial_eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                max_depth = v.as_num();
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
                    let track = self
                        .tracks
                        .entry(label.clone())
                        .or_insert_with(|| AnimationTrack::new(label.clone()));

                    let mut position = track.position.last_value();
                    let mut size = track.size.last_value();
                    let mut line_from = track.line_from.last_value();
                    let mut line_to = track.line_to.last_value();
                    let mut arc_angles = track.arc_angles.last_value();
                    let mut color = track.color.last_value();
                    let shape_type = shape_type_for_actor(ty);
                    let opacity = track.opacity.last_value();
                    let mut stroke_width = track.stroke_width.last_value();
                    let mut stroke_color = track.stroke_color.last_value();
                    let mut stroke_progress = track.stroke_progress.last_value();
                    let mut fill_opacity = track.fill_opacity.last_value();
                    let mut gap = 0.0f32;
                    let mut align: Option<String> = None;
                    let mut cols: Option<usize> = None;
                    let mut custom_path: Option<kurbo::BezPath> = None;

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
                        !track.vector_paths.keyframes.is_empty() && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            "Morph-specific modifiers on actor declarations require a path-morphing re-declaration with non-zero duration; ignoring them for now.".to_string(),
                            Some(label),
                        );
                    }

                    for prop in props {
                        match prop.name.as_str() {
                            "at" | "anchor" | "offset" => {}
                            "radius" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                size = [r, r];
                            }
                            "size" => {
                                let size_val = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    size[0] = w as f32 / 2.0;
                                    size[1] = h as f32 / 2.0;
                                }
                            }
                            "from" if ty == "Line" => {
                                if let Some(parsed) = parse_numeric_vec2(&prop.value, &eval_env) {
                                    line_from = parsed;
                                }
                            }
                            "to" if ty == "Line" => {
                                if let Some(parsed) = parse_numeric_vec2(&prop.value, &eval_env) {
                                    line_to = parsed;
                                }
                            }
                            "radius_x" if ty == "Ellipse" || ty == "Arc" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(size[0] as f64));
                                size[0] = v.as_num() as f32;
                            }
                            "radius_y" if ty == "Ellipse" || ty == "Arc" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(size[1] as f64));
                                size[1] = v.as_num() as f32;
                            }
                            "start_angle" if ty == "Arc" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(arc_angles[0] as f64));
                                arc_angles[0] = v.as_num() as f32;
                            }
                            "sweep_angle" if ty == "Arc" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(arc_angles[1] as f64));
                                arc_angles[1] = v.as_num() as f32;
                            }
                            "points" if ty == "Polygon" => {
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
                                color = parse_color_in_env(&prop.value, &eval_env);
                                // For plot types, also set stroke_color
                                if ty == "CartesianPlot" || ty == "PolarPlot" {
                                    stroke_color = parse_color_in_env(&prop.value, &eval_env);
                                }
                            }
                            "stroke_width" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "width" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "stroke_color" => {
                                stroke_color = parse_color_in_env(&prop.value, &eval_env);
                            }
                            "stroke" => {
                                stroke_color = parse_color_in_env(&prop.value, &eval_env);
                            }
                            "stroke_progress" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                stroke_progress = v.as_num() as f32;
                            }
                            "fill_opacity" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(0.0));
                                fill_opacity = v.as_num() as f32;
                            }
                            "gap" => {
                                let v = evaluate_expr(&prop.value, &eval_env)
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
                                let v = evaluate_expr(&prop.value, &eval_env)
                                    .unwrap_or(Value::Num(1.0));
                                cols = Some(v.as_num().max(1.0) as usize);
                            }
                            _ => {}
                        }
                    }

                    // For Graph types, make them invisible (container only)
                    if ty == "Graph" {
                        fill_opacity = 0.0;
                        stroke_width = 0.0;
                    }

                    if let Some((binding, bound_position)) = resolve_position_binding(
                        at_expr.as_ref(),
                        anchor_expr.as_ref(),
                        offset_expr.as_ref(),
                        &eval_env,
                    ) {
                        preserve_discrete_position_state_before(track, t_start_ms);
                        set_track_position_binding(track, t_start_ms, binding);
                        if let Some(bound_position) = bound_position {
                            position = bound_position;
                            mark_track_manual_position(track, t_start_ms);
                        } else {
                            mark_track_manual_position(track, t_start_ms);
                        }
                    } else if is_layout_container(ty) && parent_label.is_none() {
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
                    } else if ty == "CartesianPlot" || ty == "PolarPlot" {
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
                            let mut path = kurbo::BezPath::new();

                            let mut env_copy = eval_env.clone();
                            let arg_name = if !args.is_empty() {
                                args[0].clone()
                            } else {
                                "x".to_string()
                            };

                            let (min_t, max_t) = if ty == "CartesianPlot" {
                                (p_x_domain[0], p_x_domain[1])
                            } else {
                                (t_domain[0], t_domain[1])
                            };

                            env_copy.set(&arg_name, Value::Num(min_t));
                            let start_val = evaluate_expr(&body, &env_copy)
                                .unwrap_or(Value::Num(0.0))
                                .as_num();
                            let (start_math_x, start_math_y) = if ty == "CartesianPlot" {
                                (min_t, start_val)
                            } else {
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
                            let end_val = evaluate_expr(&body, &env_copy)
                                .unwrap_or(Value::Num(0.0))
                                .as_num();
                            let (end_math_x, end_math_y) = if ty == "CartesianPlot" {
                                (max_t, end_val)
                            } else {
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
                            } else {
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
                            }

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
                    } else if ty != "Graph" && ty != "CartesianPlot" && ty != "PolarPlot" {
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

                    if is_layout_container(ty) {
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
                            let target_color = parse_color_in_env(value, &eval_env);
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
                        push_unknown_target_path_diagnostic(
                            diagnostics,
                            &assignment_subject,
                            &target_key,
                        );
                        continue;
                    }

                    let track = self
                        .tracks
                        .entry(target_key.clone())
                        .or_insert_with(|| AnimationTrack::new(target_key.clone()));

                    match property.as_str() {
                        "color" => {
                            let target_color = parse_color_in_env(value, &eval_env);
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
                            let target_width = evaluate_expr(value, &eval_env)
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
                            let target_color = parse_color_in_env(value, &eval_env);
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
                            let target_val = evaluate_expr(value, &eval_env)
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
                            let target_val = evaluate_expr(value, &eval_env)
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
                            let size_val =
                                evaluate_expr(value, &eval_env).unwrap_or(Value::Num(0.0));
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
                        "url" => {
                            let target_url = evaluate_expr(value, &eval_env)
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
                                resolve_position_binding(Some(value), None, None, &eval_env)
                            {
                                preserve_discrete_position_state_before(track, t_start_ms);
                                if instant_delayed {
                                    preserve_instant_delayed_value(&mut track.position, t_start_ms);
                                }
                                mark_track_manual_position(track, t_start_ms);
                                set_track_position_binding(track, t_start_ms, binding);
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
                            let target_rotation = evaluate_expr(value, &eval_env)
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
                            let target_scale = evaluate_expr(value, &eval_env)
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
                            let r = evaluate_expr(value, &eval_env)
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
                            let target_radius = evaluate_expr(value, &eval_env)
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
                            let target_radius = evaluate_expr(value, &eval_env)
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
                            let target_angle = evaluate_expr(value, &eval_env)
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
                            let target_angle = evaluate_expr(value, &eval_env)
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
                Stmt::Action(action) => {
                    process_action(action, time_ms, self, diagnostics);
                }
                _ => {}
            }
        }
    }

    pub fn extract_all_glyphs(&self) -> Vec<crate::renderer::text::TextPath> {
        let mut glyphs = Vec::new();
        for track in self.tracks.values() {
            for (_, (paths, _)) in &track.text_paths.keyframes {
                for glyph in paths {
                    glyphs.push(glyph.clone());
                }
            }
            for glyph in &track.text_paths.default_value {
                glyphs.push(glyph.clone());
            }
        }
        glyphs
    }

    fn evaluate_node(
        &self,
        node_label: &str,
        time_ms: u64,
        parent_transform: kurbo::Affine,
        parent_opacity: f32,
        scene_dimensions: SceneDimensions,
        scene: &mut vello::Scene,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) {
        let (global_transform, global_opacity) = if let Some(track) = self.tracks.get(node_label) {
            let base_position = track.position.evaluate(time_ms);
            let binding = track.position_binding.evaluate(time_ms);
            let mut position =
                resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
            let motion_offset = track.motion_offset.evaluate(time_ms);
            let mut rotation = track.rotation.evaluate(time_ms) as f64;
            let mut scale = track.scale.evaluate(time_ms) as f64;
            let mut opacity = track.opacity.evaluate(time_ms);
            let text_paths = track.evaluate_text_paths(time_ms);
            let shape_type = track.shape_type.evaluate(time_ms);
            let mut vector_paths = track.evaluate_vector_paths(time_ms);
            let mut half_size = track.size.evaluate(time_ms);
            let mut line_from = track.line_from.evaluate(time_ms);
            let mut line_to = track.line_to.evaluate(time_ms);
            let mut arc_angles = track.arc_angles.evaluate(time_ms);
            let mut color = track.color.evaluate(time_ms);
            let mut stroke_width = track.stroke_width.evaluate(time_ms);
            let mut stroke_color = track.stroke_color.evaluate(time_ms);
            let mut fill_opacity = track.fill_opacity.evaluate(time_ms);

            if let Some(node_overrides) = overrides.get(node_label) {
                if let Some(Value::Vec2(pos)) = node_overrides.get("at") {
                    position = [pos[0] as f32, pos[1] as f32];
                }
                if let Some(Value::Num(op)) = node_overrides.get("opacity") {
                    opacity = *op as f32;
                }
                if let Some(Value::Vec2(size)) = node_overrides.get("size") {
                    half_size = [size[0] as f32 / 2.0, size[1] as f32 / 2.0];
                }
                if let Some(Value::Num(radius)) = node_overrides.get("radius") {
                    half_size = [*radius as f32, *radius as f32];
                }
                if let Some(Value::Num(radius_x)) = node_overrides.get("radius_x") {
                    half_size[0] = *radius_x as f32;
                }
                if let Some(Value::Num(radius_y)) = node_overrides.get("radius_y") {
                    half_size[1] = *radius_y as f32;
                }
                if let Some(Value::Vec2(from)) = node_overrides.get("from") {
                    line_from = [from[0] as f32, from[1] as f32];
                }
                if let Some(Value::Vec2(to)) = node_overrides.get("to") {
                    line_to = [to[0] as f32, to[1] as f32];
                }
                if let Some(Value::Num(start_angle)) = node_overrides.get("start_angle") {
                    arc_angles[0] = *start_angle as f32;
                }
                if let Some(Value::Num(sweep_angle)) = node_overrides.get("sweep_angle") {
                    arc_angles[1] = *sweep_angle as f32;
                }
                if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides.get("color") {
                    color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                }
                if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides
                    .get("stroke_color")
                    .or_else(|| node_overrides.get("stroke"))
                {
                    stroke_color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                }
                if let Some(Value::Num(width)) = node_overrides
                    .get("stroke_width")
                    .or_else(|| node_overrides.get("width"))
                {
                    stroke_width = *width as f32;
                }
                if let Some(Value::Num(opacity)) = node_overrides.get("fill_opacity") {
                    fill_opacity = *opacity as f32;
                }
                if let Some(Value::Num(angle)) = node_overrides.get("rotation") {
                    rotation = *angle;
                }
                if let Some(Value::Num(factor)) = node_overrides.get("scale") {
                    scale = *factor;
                }
            }

            if !vector_paths.is_empty() {
                vector_paths = if matches!(shape_type, SHAPE_POLYGON | SHAPE_PATH) {
                    let existing_path = vector_paths
                        .first()
                        .map(|vp| vp.path.clone())
                        .unwrap_or_else(kurbo::BezPath::new);
                    vec![styled_vello_path(
                        existing_path,
                        shape_type,
                        color,
                        stroke_width,
                        stroke_color,
                        fill_opacity,
                    )]
                } else {
                    vec![build_shape_vello_path(
                        shape_type,
                        half_size,
                        line_from,
                        line_to,
                        arc_angles,
                        color,
                        stroke_width,
                        stroke_color,
                        fill_opacity,
                    )]
                };
            }

            let local_opacity = opacity * parent_opacity;
            let local_transform = parent_transform
                * kurbo::Affine::translate((
                    position[0] as f64 + motion_offset[0] as f64,
                    position[1] as f64 + motion_offset[1] as f64,
                ))
                * kurbo::Affine::rotate(rotation)
                * kurbo::Affine::scale(scale);
            let image = track.image.evaluate(time_ms);

            for vector_path in &vector_paths {
                let transform = local_transform;
                if let Some(mut fill_color) = vector_path.fill {
                    if local_opacity < 1.0 {
                        fill_color =
                            fill_color.with_alpha(fill_color.components[3] * local_opacity);
                    }
                    scene.fill(
                        vello::peniko::Fill::NonZero,
                        transform,
                        fill_color,
                        None,
                        &vector_path.path,
                    );
                }

                if let Some((mut stroke_color, stroke_width)) = vector_path.stroke {
                    if local_opacity < 1.0 {
                        stroke_color =
                            stroke_color.with_alpha(stroke_color.components[3] * local_opacity);
                    }
                    let stroke = vello::kurbo::Stroke::new(stroke_width as f64);
                    scene.stroke(&stroke, transform, stroke_color, None, &vector_path.path);
                }
            }

            for text_path in &text_paths {
                let color = match &text_path.color {
                    typst::visualize::Paint::Solid(color) => {
                        let rgba = color.to_vec4_u8();
                        vello::peniko::Color::from_rgba8(
                            rgba[0],
                            rgba[1],
                            rgba[2],
                            (rgba[3] as f32 * local_opacity) as u8,
                        )
                    }
                    _ => vello::peniko::Color::WHITE,
                };

                scene.fill(
                    vello::peniko::Fill::NonZero,
                    local_transform,
                    color,
                    None,
                    &text_path.path,
                );
            }

            for svg_path in &track.svg_paths {
                let transform = local_transform;
                if let Some(mut fill_color) = svg_path.fill {
                    if local_opacity < 1.0 {
                        fill_color =
                            fill_color.with_alpha(fill_color.components[3] * local_opacity);
                    }
                    scene.fill(
                        vello::peniko::Fill::NonZero,
                        transform,
                        fill_color,
                        None,
                        &svg_path.path,
                    );
                }

                if let Some((mut stroke_color, stroke_width)) = svg_path.stroke {
                    if local_opacity < 1.0 {
                        stroke_color =
                            stroke_color.with_alpha(stroke_color.components[3] * local_opacity);
                    }
                    let stroke = vello::kurbo::Stroke::new(stroke_width as f64);
                    scene.stroke(&stroke, transform, stroke_color, None, &svg_path.path);
                }
            }

            if let Some(image) = image {
                let [natural_width, natural_height] = image.natural_size;
                let display_width = half_size[0] * 2.0;
                let display_height = half_size[1] * 2.0;
                let image_transform = local_transform
                    * kurbo::Affine::scale_non_uniform(
                        (display_width / natural_width) as f64,
                        (display_height / natural_height) as f64,
                    );

                let brush = vello::peniko::ImageBrush::new(image.data.clone())
                    .with_extend(vello::peniko::Extend::Pad)
                    .with_quality(vello::peniko::ImageQuality::Medium)
                    .with_alpha(local_opacity);

                scene.draw_image(&brush, image_transform);
            }

            (local_transform, local_opacity)
        } else {
            (parent_transform, parent_opacity)
        };

        if let Some(node) = self.nodes.get(node_label) {
            for child in &node.children {
                self.evaluate_node(
                    child,
                    time_ms,
                    global_transform,
                    global_opacity,
                    scene_dimensions,
                    scene,
                    overrides,
                );
            }
        }
    }

    pub fn evaluate(&self, time_s: f64, scene_dimensions: SceneDimensions) -> vello::Scene {
        let time_ms = (time_s * 1000.0) as u64;
        let mut scene = vello::Scene::new();
        let bg_color = self.background_color.evaluate(time_ms);

        let mut overrides: std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        > = std::collections::HashMap::new();
        let mut frame_env = self.frame_eval_env(time_ms, scene_dimensions, &overrides);

        for modifier in &self.modifiers {
            self.apply_modifier_stmt(
                modifier,
                time_ms,
                scene_dimensions,
                &mut frame_env,
                &mut overrides,
            );
        }

        let bg = vello::peniko::Color::new([
            bg_color[0] as f32,
            bg_color[1] as f32,
            bg_color[2] as f32,
            bg_color[3] as f32,
        ]);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            bg,
            None,
            &kurbo::Rect::new(
                0.0,
                0.0,
                scene_dimensions.width as f64,
                scene_dimensions.height as f64,
            ),
        );

        for root in &self.root_nodes {
            self.evaluate_node(
                root,
                time_ms,
                kurbo::Affine::IDENTITY,
                1.0,
                scene_dimensions,
                &mut scene,
                &overrides,
            );
        }

        scene
    }
}
impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

fn mark_track_manual_position(track: &mut AnimationTrack, time_ms: u64) {
    track
        .placement_mode
        .add_keyframe(time_ms, PlacementMode::Manual, Easing::Linear);
}

fn preserve_discrete_position_state_before(track: &mut AnimationTrack, time_ms: u64) {
    if time_ms == 0 {
        return;
    }

    let previous_time = time_ms - 1;

    if !track.placement_mode.keyframes.contains_key(&previous_time) {
        let previous_mode = track.placement_mode.evaluate(previous_time);
        track
            .placement_mode
            .add_keyframe(previous_time, previous_mode, Easing::Linear);
    }

    if !track
        .position_binding
        .keyframes
        .contains_key(&previous_time)
    {
        let previous_binding = track.position_binding.evaluate(previous_time);
        track
            .position_binding
            .add_keyframe(previous_time, previous_binding, Easing::Linear);
    }
}

fn preserve_instant_delayed_value<T: Interpolate + Clone>(
    track: &mut PropertyTrack<T>,
    t_start_ms: u64,
) {
    if t_start_ms == 0 || track.keyframes.contains_key(&t_start_ms.saturating_sub(1)) {
        return;
    }

    let previous_time = t_start_ms.saturating_sub(1);
    let previous_value = track.evaluate(previous_time);
    track.add_keyframe(previous_time, previous_value, Easing::Linear);
}

fn set_track_position_binding(track: &mut AnimationTrack, time_ms: u64, binding: PositionBinding) {
    track
        .position_binding
        .add_keyframe(time_ms, binding, Easing::Linear);
}

fn apply_explicit_position_binding(
    track: &mut AnimationTrack,
    time_ms: u64,
    binding: PositionBinding,
    position: Option<[f32; 2]>,
) {
    mark_track_manual_position(track, time_ms);
    set_track_position_binding(track, time_ms, binding);
    if let Some(position) = position {
        track
            .position
            .add_keyframe(time_ms, position, Easing::Linear);
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
