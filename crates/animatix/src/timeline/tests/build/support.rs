//! Shared fixtures for the timeline build tests.
//!
//! Only helpers reused by more than one test in this submodule live here;
//! anything narrower stays next to its test.

use super::super::tests::without_content_lints;
use crate::ast::{Expr, Property, Stmt};
use crate::timeline::property_registry::lookup_property;
use crate::timeline::{
    BuildReport, DebugRenderOptions, PropertyValue, SceneDimensions, Timeline, VelloPath,
};

/// Parse a fixture, returning its statements.
pub(super) fn parse(source: &str) -> Vec<Stmt> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    ast.expect("parsed AST")
}

/// Parse and build a fixture, keeping every diagnostic.
pub(super) fn build(source: &str) -> BuildReport<Timeline> {
    build_ast(&parse(source))
}

/// Build statements that were assembled by hand.
pub(super) fn build_ast(ast: &[Stmt]) -> BuildReport<Timeline> {
    Timeline::build_with_diagnostics(ast, &std::collections::HashMap::new())
}

/// Assert the build produced nothing but `never-revealed` content hints.
pub(super) fn assert_clean(report: &BuildReport<Timeline>) {
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );
}

/// Build a fixture whose only acceptable diagnostics are content hints.
pub(super) fn build_clean(source: &str) -> Timeline {
    let report = build(source);
    assert_clean(&report);
    report.output
}

/// Build a fixture and require that no diagnostic is error-level.
pub(super) fn build_timeline(source: &str) -> Timeline {
    let report = build(source);
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "Unexpected build errors: {:?}",
        report.diagnostics
    );
    report.output
}

pub(super) fn prop(name: &str, value: Expr) -> Property {
    Property {
        name: name.to_string(),
        value,
        value_span: None,
        trailing_comment: None,
    }
}

pub(super) fn vec2(x: f64, y: f64) -> Expr {
    Expr::Tuple(vec![Expr::Num(x), Expr::Num(y)])
}

pub(super) fn actor(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
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

pub(super) fn keyframe(seconds: f64, body: Vec<Stmt>) -> Stmt {
    Stmt::Keyframe {
        time: crate::ast::Time::Seconds(seconds),
        body,
        span: None,
    }
}

/// The vector paths the frame at `time_s` actually draws.
pub(super) fn render_paths(timeline: &Timeline, time_s: f64) -> Vec<VelloPath> {
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
pub(super) fn assert_finite_samples(timeline: &Timeline, time_s: f64) -> usize {
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

/// The evaluated content of `label`'s text track at `ms`.
pub(super) fn text_at(timeline: &Timeline, label: &str, ms: u64) -> String {
    timeline
        .world
        .get_id(label)
        .and_then(|id| timeline.world.text_or_default(id).text_content.as_ref())
        .map(|track| track.evaluate(ms))
        .unwrap_or_default()
}

/// Read a registry-backed property, without substituting its default.
pub(super) fn property_value(
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

pub(super) fn f32_property(
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

pub(super) fn vec2_property(
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
pub(super) fn property_or_default(
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
