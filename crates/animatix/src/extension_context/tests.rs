
use std::sync::Arc;

use super::{ExtensionContext, PropertyBinding, PropertyRegistry};
use crate::ast::Action;
use crate::composition::BuildTarget;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
use crate::timeline::{ActorCategory, TextKind, Timeline, TrackAccessor, Value};

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
        let track = ctx
            .timeline
            .world
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| crate::timeline::AnimationTrack::placeholder(label.to_string()));
        // The fixture historically claimed the `Text` kind so its plan
        // carries text slots; derive Text's caps while keeping the
        // extension's own actor type.
        track.caps = animatix_std::caps_for_type("Text").unwrap_or_default();
        track.rebuild_property_plan();
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

struct MarkAction;

impl BuiltinAction for MarkAction {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "mark".to_string(),
            category: "Custom".to_string(),
            description: "Extension action".to_string(),
            params: vec![],
            modifiers: vec![],
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        for target in &action.targets {
            if let Some(track) = timeline.world.tracks.get_mut(target) {
                track
                    .style
                    .opacity
                    .ensure(1.0)
                    .add_keyframe(time_ms as u64, 0.25, Easing::Linear);
            }
        }
    }
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
    ctx.register_action(Box::new(MarkAction)).expect("register action");
    ctx.register_function("double", |args, _env| {
        let Some(Value::Num(n)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch(
                "double expects one number".to_string(),
            ));
        };
        Ok(Value::Num(*n * 2.0))
    })
    .expect("register function");
    ctx.provide("threshold", 42_u32).expect("provide service");

    assert!(ctx.primitive_registry().find("Marker").is_some());
    assert!(ctx.action("mark").is_some());
    assert_eq!(ctx.get::<u32>("threshold"), Some(&42));

    let mut env = crate::timeline::Environment::new();
    ctx.install_functions(&mut env);
    assert!(matches!(env.get("double"), Some(Value::NativeFn(_))));
}

#[test]
fn context_records_tooling_descriptors() {
    let mut ctx = ExtensionContext::new();
    ctx.register_action(Box::new(MarkAction)).expect("register action");
    ctx.register_function_with_descriptor(
        "double",
        |_args, _env| Ok(Value::Num(42.0)),
        animatix_syntax::schema::FunctionDescriptor {
            name: "double".to_string(),
            params: vec![animatix_syntax::schema::ActionParam {
                name: "value".to_string(),
                description: "Input value".to_string(),
                type_info: "Num".to_string(),
            }],
            return_type: Some(animatix_syntax::typing::Type::Num),
            help: Some("Doubles a number".to_string()),
        },
    )
    .expect("register function");
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

    assert_eq!(ctx.action_signatures()[0].name, "mark");
    assert_eq!(ctx.function_descriptors()[0].params[0].name, "value");
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
    let track = report.output.world.tracks.get("r").expect("rect track");
    let slot = track.property_plan.get(PropertyId(1_000_000)).expect("bool slot");
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
    ctx.register_action(Box::new(MarkAction)).expect("register action");
    ctx.register_function("double", |args, _env| {
        let Some(Value::Num(n)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch(
                "double expects one number".to_string(),
            ));
        };
        Ok(Value::Num(*n * 2.0))
    })
    .expect("register function");
    ctx.provide("threshold", 42_u32).expect("provide service");

    assert!(ctx.remove_primitive("Marker"));
    assert!(!ctx.remove_primitive("Marker"));
    assert!(ctx.remove_action("mark"));
    assert!(ctx.remove_function("double"));
    assert!(ctx.remove_service("threshold"));
    assert!(ctx.primitive_registry().find("Marker").is_none());
    assert!(ctx.action("mark").is_none());
    assert!(ctx.get::<u32>("threshold").is_none());
}

#[test]
fn build_with_context_installs_extension_functions() {
    let (ast, errors) = animatix_syntax::parser::parse_source("let answer = double(21)");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_function("double", |args, _env| {
        let Some(Value::Num(n)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch(
                "double expects one number".to_string(),
            ));
        };
        Ok(Value::Num(*n * 2.0))
    })
    .expect("register function");

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
fn build_target_accepts_extension_context() {
    let (ast, errors) = animatix_syntax::parser::parse_source("let answer = double(21)");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_function("double", |args, _env| {
        let Some(Value::Num(n)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch(
                "double expects one number".to_string(),
            ));
        };
        Ok(Value::Num(*n * 2.0))
    })
    .expect("register function");

    let report = BuildTarget::from_ast_with_context(
        &ast,
        &std::collections::HashMap::new(),
        None,
        Arc::new(ctx),
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
    let (ast, errors) = animatix_syntax::parser::parse_source("# A\nlet answer = double(21)");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_function("double", |args, _env| {
        let Some(Value::Num(n)) = args.first() else {
            return Err(crate::timeline::EvalError::TypeMismatch(
                "double expects one number".to_string(),
            ));
        };
        Ok(Value::Num(*n * 2.0))
    })
    .expect("register function");

    let report = BuildTarget::from_ast_with_context(
        &ast,
        &std::collections::HashMap::new(),
        None,
        Arc::new(ctx),
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
fn builtin_property_entries_cover_runtime_registry() {
    let registry = PropertyRegistry::new();
    for schema in crate::timeline::PROPERTY_REGISTRY.iter() {
        let entry = registry
            .builtins
            .iter()
            .find(|entry| entry.descriptor.name == schema.name)
            .unwrap_or_else(|| panic!("missing property entry for {}", schema.name));
        assert!(
            matches!(entry.binding, PropertyBinding::Direct { .. }),
            "runtime property {} must have a direct binding",
            schema.name
        );
    }
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

    let track = report.output.world.tracks.get("g").expect("gauge track");
    assert_eq!(track.actor_type, "Gauge");
    let level = animatix_syntax::schema::PropertyId(1_000_000);
    assert_eq!(
        track.property_plan.get(level).and_then(|slot| slot.track.sample(0)),
        Some(crate::timeline::PropertyValue::F32(42.0))
    );
    // `#1s g.level = 80` carries no `[duration]`, so it is an instant change
    // (`docs/spec.md`, "Instant Change"): the declared 42 holds until the
    // stamp rather than easing toward 80 across the preceding second.
    assert_eq!(
        track.property_plan.get(level).and_then(|slot| slot.track.sample(500)),
        Some(crate::timeline::PropertyValue::F32(42.0)),
        "an undated extension assignment eased from the previous keyframe"
    );
    assert_eq!(
        track.property_plan.get(level).and_then(|slot| slot.track.sample(1000)),
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
    let track = report.output.world.tracks.get("r").expect("rect track");
    assert_eq!(
        track
            .property_plan
            .get(animatix_syntax::schema::PropertyId(1_000_000))
            .and_then(|slot| slot.track.sample(500)),
        // Undated assignment at `#1s`, so the declared 5 holds through the
        // gap; see `build_with_context_writes_extension_properties`.
        Some(crate::timeline::PropertyValue::F32(5.0)),
        "an undated assignment on a built-in actor eased from its declaration"
    );
    assert_eq!(
        track
            .property_plan
            .get(animatix_syntax::schema::PropertyId(1_000_000))
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
            .track
            .property_plan
            .get(animatix_syntax::schema::PropertyId(1_000_000))
            .and_then(|slot| slot.track.sample(0)),
        Some(crate::timeline::PropertyValue::F32(61.0))
    );
}

#[test]
fn build_with_context_dispatches_custom_actions() {
    let (ast, errors) =
        animatix_syntax::parser::parse_source("target: Rect, size: (10, 10)\n#0s\nmark target");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    ctx.register_action(Box::new(MarkAction)).expect("register action");
    let report =
        Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let track = report.output.world.tracks.get("target").expect("actor track");
    assert_eq!(track.style.opacity.get(0, 1.0), 0.25);
}
