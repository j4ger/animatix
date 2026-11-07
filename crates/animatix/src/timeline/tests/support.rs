//! Shared test fixtures visible to every timeline test submodule.
//!
//! This is the single home for fixture construction, frame sampling and
//! registry reads. A test module that re-derives any of it locally is a bug:
//! the copies drift, and the drift is invisible until one of them asserts a
//! different thing.
//!
//! Only helpers reused by more than one sibling test module live here;
//! anything narrower stays next to its test.

use crate::ast::{Expr, Property, Stmt};
use crate::timeline::property_registry::lookup_property;
use crate::timeline::{
    BuildReport, DebugRenderOptions, PropertyValue, SceneDimensions, Timeline, VelloPath,
};
use animatix_syntax::diagnostics::Diagnostic;

use super::without_content_lints;

// ── Building fixtures ────────────────────────────────────────────────────────

/// Parse a fixture, returning its statements.
pub(crate) fn parse(source: &str) -> Vec<Stmt> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    ast.expect("parsed AST")
}

/// Parse and build a fixture, keeping every diagnostic.
pub(crate) fn build(source: &str) -> BuildReport<Timeline> {
    build_ast(&parse(source))
}

/// Build statements that were assembled by hand.
pub(crate) fn build_ast(ast: &[Stmt]) -> BuildReport<Timeline> {
    Timeline::build_with_diagnostics(ast, &std::collections::HashMap::new())
}

/// Build a fixture and return only the timeline, without judging diagnostics.
///
/// Use this when the test only reads timeline state; use [`build`] when it
/// inspects the diagnostics, and [`build_clean`]/[`build_timeline`] when it
/// asserts on them.
pub(crate) fn build_output(source: &str) -> Timeline {
    build(source).output
}

/// Build a fixture and require that no diagnostic is error-level.
pub(crate) fn build_timeline(source: &str) -> Timeline {
    let report = build(source);
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "Unexpected build errors: {:?}",
        report.diagnostics
    );
    report.output
}

/// Assert the build produced nothing but `never-revealed` content hints.
pub(crate) fn assert_clean(report: &BuildReport<Timeline>) {
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );
}

/// Build a fixture whose only acceptable diagnostics are content hints.
pub(crate) fn build_clean(source: &str) -> Timeline {
    let report = build(source);
    assert_clean(&report);
    report.output
}

/// Build a fixture and hand back both the timeline and every diagnostic.
///
/// For scenes whose point *is* the warning; prefer [`build_clean`] otherwise.
pub(crate) fn build_ignoring_warnings(source: &str) -> (Timeline, Vec<Diagnostic>) {
    let report = build(source);
    (report.output, report.diagnostics)
}

// ── Sampling the frame ───────────────────────────────────────────────────────

/// The vector paths the frame at `time_s` actually draws.
pub(crate) fn render_paths(timeline: &Timeline, time_s: f64) -> Vec<VelloPath> {
    let mut filter_backend = None;
    let program = timeline.evaluate_program_with_debug(
        time_s,
        SceneDimensions {
            width: 640,
            height: 360,
        },
        DebugRenderOptions::default(),
        &mut filter_backend,
    );
    let mut paths = Vec::new();
    for item in &program.items {
        for command in &item.commands {
            if let crate::primitives::RenderCommand::Paths { paths: found } = command {
                paths.extend(found.iter().cloned());
            }
        }
    }
    paths
}

/// Assert every sampled vertex is finite and return how many were checked.
pub(crate) fn assert_finite_samples(timeline: &Timeline, time_s: f64) -> usize {
    let mut checked = 0;
    for vp in render_paths(timeline, time_s) {
        for el in vp.path.elements() {
            if let kurbo::PathEl::LineTo(p) | kurbo::PathEl::MoveTo(p) = el {
                assert!(p.y.is_finite(), "sampled y must be finite, got NaN at x={}", p.x);
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "expected sampled points");
    checked
}

/// The vector paths an entity has baked into its shape tier at `t = 0`.
pub(crate) fn baked_paths(timeline: &Timeline, label: &str) -> Vec<VelloPath> {
    let id = timeline.world.get_id(label).unwrap_or_else(|| panic!("no {label} track"));
    timeline
        .world
        .shape_or_default(id)
        .evaluate_vector_paths(timeline.world.style(id), 0)
        .to_vec()
}

/// Overall bounds of a set of baked paths.
pub(crate) fn path_bounds(paths: &[VelloPath]) -> ([f64; 2], [f64; 2]) {
    use kurbo::Shape as _;
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for path in paths {
        let bounds = path.path.bounding_box();
        min[0] = min[0].min(bounds.min_x());
        min[1] = min[1].min(bounds.min_y());
        max[0] = max[0].max(bounds.max_x());
        max[1] = max[1].max(bounds.max_y());
    }
    (min, max)
}

// ── Reading tracks ───────────────────────────────────────────────────────────

/// The evaluated content of `label`'s text track at `ms`.
pub(crate) fn text_at(timeline: &Timeline, label: &str, ms: u64) -> String {
    timeline
        .world
        .get_id(label)
        .and_then(|id| timeline.world.text_or_default(id).text_content.as_ref())
        .map(|track| track.evaluate(ms))
        .unwrap_or_default()
}

/// Read a registry-backed property, without substituting its default.
pub(crate) fn property_value(
    timeline: &Timeline,
    label: &str,
    property: &str,
    ms: u64,
) -> Option<PropertyValue> {
    let schema = lookup_property(property).expect("registered property");
    let track = timeline
        .world
        .tracks
        .get_id(label)
        .unwrap_or_else(|| panic!("no track {label}"));
    timeline.world.read_property_value(track, schema.field, ms)
}

pub(crate) fn f32_property(
    timeline: &Timeline,
    label: &str,
    property: &str,
    ms: u64,
) -> Option<f32> {
    match property_value(timeline, label, property, ms) {
        Some(PropertyValue::F32(value)) => Some(value),
        None => None,
        other => panic!("expected a numeric {property} for {label} at {ms}ms, got {other:?}"),
    }
}

pub(crate) fn vec2_property(
    timeline: &Timeline,
    label: &str,
    property: &str,
    ms: u64,
) -> Option<[f32; 2]> {
    match property_value(timeline, label, property, ms) {
        Some(PropertyValue::Vec2(value)) => Some(value),
        None => None,
        other => panic!("expected a Vec2 {property} for {label} at {ms}ms, got {other:?}"),
    }
}

/// Read a registry-backed property, substituting its registered default when
/// no track wrote one.
pub(crate) fn property_or_default(
    timeline: &Timeline,
    label: &str,
    property: &str,
    ms: u64,
) -> PropertyValue {
    let schema = lookup_property(property).expect("registered property");
    let track = timeline
        .world
        .tracks
        .get_id(label)
        .unwrap_or_else(|| panic!("no track {label}"));
    timeline
        .world
        .read_property_value(track, schema.field, ms)
        .unwrap_or_else(|| (schema.default_value)(&timeline.world.caps(track)))
}

// ── Assembling statements by hand ────────────────────────────────────────────

pub(crate) fn actor(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}

pub(crate) fn keyframe(seconds: f64, body: Vec<Stmt>) -> Stmt {
    Stmt::Keyframe {
        time: crate::ast::Time::Seconds(seconds),
        body,
        span: None,
    }
}

pub(crate) fn prop(name: &str, value: Expr) -> Property {
    Property {
        name: name.to_string(),
        value,
        value_span: None,
        trailing_comment: None,
    }
}

pub(crate) fn vec2(x: f64, y: f64) -> Expr {
    Expr::Tuple(vec![Expr::Num(x), Expr::Num(y)])
}
