use std::sync::Arc;

use super::ExtensionContext;
use crate::composition::BuildTarget;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::{ActorCategory, TextKind, Timeline, TrackAccessor, Value};
use animatix_core::callable::{CallableDescriptor, CallableParam, CallablePhase, CallableType};

struct Marker;

impl Primitive for Marker {
    fn type_name(&self) -> &str {
        "Marker"
    }

    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }
}

struct Gauge;

impl Primitive for Gauge {
    fn type_name(&self) -> &str {
        "Gauge"
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        let track_id = ctx.timeline.world.spawn(label, "Gauge");
        // The fixture historically claimed the `Text` kind so its plan carries
        // text slots; override caps while keeping the extension's own actor type.
        ctx.timeline
            .world
            .set_caps(track_id, animatix_std::caps_for_type("Text").unwrap_or_default());
        ctx.timeline.world.rebuild_property_plan(track_id);
        Ok(())
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        Ok(Some(vec![crate::primitives::RenderCommand::Paths { paths: Vec::new() }]))
    }
}

fn descriptor(name: &str, result: CallableType) -> CallableDescriptor {
    CallableDescriptor {
        namespace: "test".into(),
        name: name.into(),
        params: vec![CallableParam::required("value", CallableType::Num)],
        return_type: result,
        phase: CallablePhase::BuildAndFrame,
        help: "Test callable".into(),
        category: "Test".into(),
    }
}

fn register_double(ctx: &mut ExtensionContext) {
    ctx.register_callable(descriptor("double", CallableType::Num), |args, _env| {
        let Some(Value::Num(n)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch(
                "double expects one number".into(),
            ));
        };
        Ok(Value::Num(*n * 2.0))
    })
    .expect("register callable");
}

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
fn without_content_lints(
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> impl Iterator<Item = &animatix_syntax::diagnostics::Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
}

#[test]
fn context_registers_capabilities() {
    let mut ctx = ExtensionContext::new();
    ctx.register_primitive(
        Arc::new(Marker),
        animatix_std::PrimitiveInfo {
            type_name: "Marker".into(),
            display_name: "Marker".into(),
            category: ActorCategory::Annotation,
            icon_id: "marker".into(),
            advanced: false,
            capabilities: Default::default(),
            required_features: animatix_core::features::FeatureMask::UNKNOWN,
            child_processing: Default::default(),
            shape: None,
            text: Some(TextKind::Text),
            stroke_path: false,
            connector: false,
            plot: None,
            doc: "",
            default_stub: None,
        },
    )
    .expect("register primitive");
    register_double(&mut ctx);
    ctx.provide("threshold", 42_u32).expect("provide service");

    assert!(ctx.primitive_registry().find("Marker").is_some());
    assert_eq!(ctx.get::<u32>("threshold"), Some(&42));

    let mut env = crate::timeline::Environment::new();
    ctx.install_functions(&mut env);
    assert!(matches!(env.get("test::double"), Some(Value::NativeFn(_))));
}

#[test]
fn context_records_tooling_descriptors() {
    let mut ctx = ExtensionContext::new();
    register_double(&mut ctx);
    ctx.provide_with_descriptor(
        "theme",
        "dark",
        animatix_syntax::schema::ServiceDescriptor {
            name: "theme".to_string(),
            type_info: Some("str".to_string()),
            help: None,
        },
    )
    .expect("provide service");

    assert_eq!(ctx.callable_descriptors()[0].params[0].name, "value");
    assert_eq!(ctx.service_descriptors()[0].type_info.as_deref(), Some("str"));
}

#[test]
fn extension_bool_property_roundtrips() {
    use animatix_syntax::schema::{PropertyId, PropertyValueKind};

    let (ast, errors) =
        animatix_syntax::parser::parse_source("r: Rect, visible: true\n#1s\n r.visible = false");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_property("Rect", "visible", PropertyValueKind::Bool, true)
        .expect("register property");

    let report =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let id = report.output.world.get_id("r").expect("rect track");
    let slot = report
        .output
        .world
        .property_plan(id)
        .get(PropertyId(1_000_000))
        .expect("bool slot");
    assert_eq!(slot.track.sample(0), Some(crate::timeline::PropertyValue::Bool(true)));
    assert_eq!(slot.track.sample(1000), Some(crate::timeline::PropertyValue::Bool(false)));

    let frame_env = report.output.build_frame_env(
        1000,
        crate::timeline::SceneDimensions::default(),
        &std::collections::HashMap::new(),
    );
    assert_eq!(frame_env.get("r.visible"), Some(Value::Bool(false)));
}

#[test]
fn typed_property_descriptor_preserves_precise_type() {
    use animatix_syntax::schema::PropertyValueKind;
    use animatix_syntax::typing::Type;

    let mut ctx = ExtensionContext::new();
    ctx.register_property_full_typed(
        "Pulse",
        "glow",
        PropertyValueKind::Vec4,
        Some(Type::Color),
        true,
        None,
        None,
        None,
    )
    .expect("register typed property");

    let descriptor = ctx.extension_property_descriptors().pop().expect("descriptor");
    assert_eq!(descriptor.ty, Type::Color);
    assert_eq!(descriptor.value_kind, PropertyValueKind::Vec4);
}

#[test]
fn context_can_dispose_registered_capabilities() {
    let mut ctx = ExtensionContext::new();
    ctx.register_primitive(
        Arc::new(Marker),
        animatix_std::PrimitiveInfo {
            type_name: "Marker".into(),
            display_name: "Marker".into(),
            category: ActorCategory::Annotation,
            icon_id: "marker".into(),
            advanced: false,
            capabilities: Default::default(),
            required_features: animatix_core::features::FeatureMask::UNKNOWN,
            child_processing: Default::default(),
            shape: None,
            text: Some(TextKind::Text),
            stroke_path: false,
            connector: false,
            plot: None,
            doc: "",
            default_stub: None,
        },
    )
    .expect("register primitive");
    register_double(&mut ctx);
    ctx.provide("threshold", 42_u32).expect("provide service");

    assert!(ctx.remove_primitive("Marker"));
    assert!(!ctx.remove_primitive("Marker"));
    assert!(ctx.remove_callable("test::double"));
    assert!(ctx.remove_service("threshold"));
    assert!(ctx.primitive_registry().find("Marker").is_none());
    assert!(ctx.get::<u32>("threshold").is_none());
}

#[test]
fn build_with_context_installs_extension_functions() {
    let (ast, errors) = animatix_syntax::parser::parse_source("let answer = test::double(21)");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    register_double(&mut ctx);

    let report =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    assert_eq!(report.output.env.get("answer"), Some(Value::Num(42.0)));
}

#[test]
fn lexical_user_functions_retain_native_callables_during_frame_sampling() {
    let (ast, errors) = animatix_syntax::parser::parse_source(
        "fn amount(value: Num) { test::double(value) }\n\
         r: Rect, opacity: 1\n\
         always { r.opacity = amount(t) }",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let mut context = ExtensionContext::new();
    register_double(&mut context);
    let report = Timeline::build_with_context(
        &ast.unwrap(),
        &std::collections::HashMap::new(),
        Arc::new(context),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let timeline = report.output;
    let dimensions = crate::timeline::SceneDimensions::default();
    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env(250, dimensions, &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(program, 250, dimensions, &mut env, &mut overrides)
            .unwrap();
    }
    assert_eq!(overrides["r"]["opacity"], Value::Num(0.5));
}

#[test]
fn build_target_accepts_extension_context() {
    let (ast, errors) = animatix_syntax::parser::parse_source("let answer = test::double(21)");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    register_double(&mut ctx);

    let report = BuildTarget::build_with_options(
        &ast,
        &std::collections::HashMap::new(),
        None,
        crate::timeline::BuildOptions::default().with_context(Arc::new(ctx)),
    );
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let BuildTarget::SingleScene(timeline) = &report.output else {
        panic!("expected single-scene build target");
    };
    assert_eq!(timeline.env.get("answer"), Some(Value::Num(42.0)));
}

#[test]
fn build_target_propagates_context_to_composition() {
    let (ast, errors) = animatix_syntax::parser::parse_source("# A\nlet answer = test::double(21)");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    register_double(&mut ctx);

    let report = BuildTarget::build_with_options(
        &ast,
        &std::collections::HashMap::new(),
        None,
        crate::timeline::BuildOptions::default().with_context(Arc::new(ctx)),
    );
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let BuildTarget::MultiScene(composition) = &report.output else {
        panic!("expected multi-scene build target");
    };
    let scene = composition.scenes.get("A").expect("scene A");
    assert_eq!(scene.timeline.env.get("answer"), Some(Value::Num(42.0)));
}

#[test]
fn context_registers_and_disposes_properties() {
    use animatix_syntax::schema::PropertyValueKind;

    let mut ctx = ExtensionContext::new();
    let id = ctx
        .register_property("Gauge", "level", PropertyValueKind::F32, true)
        .expect("register property");
    assert_eq!(ctx.property_spec("Gauge", "level").map(|spec| spec.id), Some(id));
    assert!(ctx.property_spec("Gauge", "missing").is_none());
    assert!(ctx.remove_property("Gauge", "level"));
    assert!(!ctx.remove_property("Gauge", "level"));
    assert!(ctx.property_spec("Gauge", "level").is_none());
}

#[test]
fn build_with_context_writes_extension_properties() {
    use animatix_syntax::schema::PropertyValueKind;

    let (ast, errors) = animatix_syntax::parser::parse_source(
        "g: Gauge, level: 42\n#1s\n g.level = 80\nalways { g.level = g.level + 1 }",
    );
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_primitive(
        Arc::new(Gauge),
        animatix_std::PrimitiveInfo {
            type_name: "Gauge".into(),
            display_name: "Gauge".into(),
            category: ActorCategory::Plot,
            icon_id: "gauge".into(),
            advanced: false,
            capabilities: Default::default(),
            required_features: animatix_core::features::FeatureMask::UNKNOWN,
            child_processing: Default::default(),
            shape: None,
            text: Some(TextKind::Text),
            stroke_path: false,
            connector: false,
            plot: None,
            doc: "",
            default_stub: None,
        },
    )
    .expect("register Gauge");
    ctx.register_property("Gauge", "level", PropertyValueKind::F32, true)
        .expect("register level");

    let report =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let id = report.output.world.get_id("g").expect("gauge track");
    assert_eq!(report.output.world.actor_type(id), "Gauge");
    let plan = report.output.world.property_plan(id);
    let level = animatix_syntax::schema::PropertyId(1_000_000);
    assert_eq!(
        plan.get(level).and_then(|slot| slot.track.sample(0)),
        Some(crate::timeline::PropertyValue::F32(42.0))
    );
    // `#1s g.level = 80` carries no `[duration]`, so it is an instant change
    // (`docs/spec.md`, "Instant Change"): the declared 42 holds until the
    // stamp rather than easing toward 80 across the preceding second.
    assert_eq!(
        plan.get(level).and_then(|slot| slot.track.sample(500)),
        Some(crate::timeline::PropertyValue::F32(42.0)),
        "an undated extension assignment eased from the previous keyframe"
    );
    assert_eq!(
        plan.get(level).and_then(|slot| slot.track.sample(1000)),
        Some(crate::timeline::PropertyValue::F32(80.0))
    );

    let frame_env = report.output.build_frame_env(
        500,
        crate::timeline::SceneDimensions::default(),
        &std::collections::HashMap::new(),
    );
    assert_eq!(frame_env.get("g.level"), Some(Value::Num(42.0)));
}

#[test]
fn build_with_context_writes_extension_property_on_builtin_actor() {
    use animatix_syntax::schema::PropertyValueKind;

    let (ast, errors) =
        animatix_syntax::parser::parse_source("r: Rect, intensity: 5\n#1s\n r.intensity = 10");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_property("Rect", "intensity", PropertyValueKind::F32, true)
        .expect("register intensity");

    let report =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let id = report.output.world.get_id("r").expect("rect track");
    let plan = report.output.world.property_plan(id);
    assert_eq!(
        plan.get(animatix_syntax::schema::PropertyId(1_000_000))
            .and_then(|slot| slot.track.sample(500)),
        // Undated assignment at `#1s`, so the declared 5 holds through the
        // gap; see `build_with_context_writes_extension_properties`.
        Some(crate::timeline::PropertyValue::F32(5.0)),
        "an undated assignment on a built-in actor eased from its declaration"
    );
    assert_eq!(
        plan.get(animatix_syntax::schema::PropertyId(1_000_000))
            .and_then(|slot| slot.track.sample(1000)),
        Some(crate::timeline::PropertyValue::F32(10.0)),
        "the step did not take effect at its own stamp"
    );
}

#[cfg(feature = "serde")]
#[test]
fn extension_property_survives_carry_bag_serde() {
    use animatix_syntax::schema::PropertyValueKind;

    // The assignment needs a duration to interpolate: `#1s g.level = 80`
    // without one is an *instant* change (see the sibling
    // `extension_property_*` tests), so a carry at 500 ms would still read
    // the declared 42. A ramp from the declaration at 0 to 80 over 1 s
    // samples 61 at the halfway carry point, which is what makes this a
    // round-trip check rather than a "still 42" check.
    let (ast, errors) =
        animatix_syntax::parser::parse_source("g: Gauge, level: 42\n#0s\n g.level = 80 [1s]");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_primitive(
        Arc::new(Gauge),
        animatix_std::PrimitiveInfo {
            type_name: "Gauge".into(),
            display_name: "Gauge".into(),
            category: ActorCategory::Plot,
            icon_id: "gauge".into(),
            advanced: false,
            capabilities: Default::default(),
            required_features: animatix_core::features::FeatureMask::UNKNOWN,
            child_processing: Default::default(),
            shape: None,
            text: Some(TextKind::Text),
            stroke_path: false,
            connector: false,
            plot: None,
            doc: "",
            default_stub: None,
        },
    )
    .expect("register Gauge");
    ctx.register_property("Gauge", "level", PropertyValueKind::F32, true)
        .expect("register level");

    let mut timeline =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx)).output;
    timeline.set_persistence_flag("g", true);
    let bag = timeline.compute_carry_bag(500, true);

    let json = serde_json::to_string(&bag).expect("serialize carry bag");
    let restored: crate::timeline::persistence::CarryBag =
        serde_json::from_str(&json).expect("deserialize carry bag");
    let entry = restored.entries.get("g").expect("carried gauge");
    assert_eq!(
        entry
            .property_plan
            .get(animatix_syntax::schema::PropertyId(1_000_000))
            .and_then(|slot| slot.track.sample(0)),
        Some(crate::timeline::PropertyValue::F32(61.0))
    );
}

#[test]
fn build_with_context_mounts_inert_extension_plans() {
    use crate::timeline::{PlanDuration, TimelinePlanBuilder};
    let (ast, errors) = animatix_syntax::parser::parse_source(
        "target: Rect, size: (10, 10)\n#0s\ntest::mark(target)",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let mut ctx = ExtensionContext::new();
    let mut contract = descriptor("mark", CallableType::Timeline);
    contract.params = vec![CallableParam::required("target", CallableType::Actor)];
    contract.phase = CallablePhase::Build;
    ctx.register_callable(contract, |args, _| {
        let Some(Value::ActorRef(actor)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch("expected actor".into()));
        };
        let mut builder = TimelinePlanBuilder::default();
        builder.append_property_op(
            actor.clone(),
            "opacity",
            Value::Num(0.25),
            PlanDuration::default(),
            PlanDuration::default(),
            Some(Easing::Linear),
        );
        Ok(Value::TimelinePlan(Arc::new(builder.finish())))
    })
    .expect("register plan function");
    let report =
        Timeline::build_with_context(&ast.expect("AST"), &Default::default(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "{:?}",
        report.diagnostics
    );
    let id = report.output.world.get_id("target").expect("actor");
    assert_eq!(report.output.world.style(id).opacity.get(0, 1.0), 0.25);
}

#[test]
fn build_with_context_registers_extension_behavior_for_render_dispatch() {
    let (ast, errors) = animatix_syntax::parser::parse_source("g: Gauge");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_primitive(
        Arc::new(Gauge),
        animatix_std::PrimitiveInfo::extension("Gauge", ActorCategory::Plot),
    )
    .expect("register Gauge");

    let report =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let id = timeline.world.get_id("g").expect("gauge track");
    let def = timeline.behavior_for_node(id, 0).expect("extension behavior");
    assert_eq!(def.name.as_ref(), "Gauge");
    assert!(
        def.extension_primitive().is_some(),
        "the synced behavior must dispatch to the extension primitive"
    );
    assert_eq!(
        timeline.world.behavior(id),
        def.id,
        "entity behavior id must match the registry"
    );
    let _ = timeline.evaluate(0.0, crate::timeline::SceneDimensions::default());
}

#[test]
fn invalid_callable_contracts_do_not_partially_register() {
    use animatix_core::callable::DefaultValue;
    let mut ctx = ExtensionContext::new();
    let mut invalid_default = descriptor("wrong_default", CallableType::Num);
    invalid_default.params[0].default = Some(DefaultValue::Duration(0.5));
    assert!(ctx.register_callable(invalid_default, |_, _| Ok(Value::Num(0.0))).is_err());
    let mut invalid_phase = descriptor("frame_plan", CallableType::Timeline);
    invalid_phase.phase = CallablePhase::BuildAndFrame;
    assert!(ctx.register_callable(invalid_phase, |_, _| Ok(Value::Unit)).is_err());
    let mut reserved = descriptor("sin", CallableType::Num);
    reserved.namespace = "std".into();
    assert!(ctx.register_callable(reserved, |_, _| Ok(Value::Num(0.0))).is_err());
    assert!(ctx.callable_descriptors().is_empty());
    let mut env = crate::timeline::Environment::new();
    ctx.install_functions(&mut env);
    assert!(env.get("test::wrong_default").is_none());
}

#[test]
fn namespaced_native_functions_do_not_replace_standard_prelude_contracts() {
    let mut context = ExtensionContext::new();
    let mut native = descriptor("sin", CallableType::Num);
    native.params.clear();
    context.register_callable(native, |_, _| Ok(Value::Num(42.0))).unwrap();
    let mut env = crate::timeline::Environment::new();
    crate::timeline::load_standard_library(&mut env);
    context.install_functions(&mut env);
    for (source, expected) in [("sin(0)", 0.0), ("test::sin()", 42.0)] {
        let (ast, errors) =
            animatix_syntax::parser::parse_source(&format!("let answer = {source}"));
        assert!(errors.is_empty(), "{errors:?}");
        let crate::ast::Stmt::LetDecl { value, .. } = &ast.unwrap()[0] else {
            panic!("let")
        };
        assert_eq!(crate::timeline::evaluate_expr(value, &env).unwrap(), Value::Num(expected));
    }
    assert_eq!(env.callable_descriptor("sin").unwrap().namespace, "std");
}
