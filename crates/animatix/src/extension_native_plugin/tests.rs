use std::ffi::c_void;

use animatix_plugin_api::NATIVE_RESIZE_MODE_SIZE;

use super::*;
use crate::extension_context::ExtensionContext;
use crate::timeline::{Environment, PropertyKind, Value};

/// The ABI easing table must not be able to lie. Every code the host
/// advertises decodes to a curve and re-encodes to that same code, codes it
/// does not know are rejected instead of reading as `Linear`, and the two
/// parameterized curves report `UNSUPPORTED` rather than borrowing a
/// neighbour's name. Before the table was exhaustive, `bounce`/`elastic`/
/// `back`/`expo*` all crossed the boundary as `Linear` in both directions.
#[test]
fn easing_codes_round_trip_and_reject_the_unknown() {
    use crate::easing::Easing;
    let table: &[(u32, Easing)] = &[
        (NATIVE_EASING_LINEAR, Easing::Linear),
        (NATIVE_EASING_IN, Easing::EaseIn),
        (NATIVE_EASING_OUT, Easing::EaseOut),
        (NATIVE_EASING_IN_OUT, Easing::EaseInOut),
        (NATIVE_EASING_BOUNCE_IN, Easing::Bounce),
        (NATIVE_EASING_ELASTIC, Easing::Elastic),
        (NATIVE_EASING_BACK, Easing::Back),
        (NATIVE_EASING_EXPO, Easing::Expo),
        (NATIVE_EASING_EXPO_OUT, Easing::ExpoOut),
        (NATIVE_EASING_EXPO_IN_OUT, Easing::ExpoInOut),
    ];
    for (code, easing) in table {
        assert_eq!(native_easing(*code), Some(*easing), "code {code} did not decode to {easing:?}");
        assert_eq!(easing_code(*easing), *code, "{easing:?} did not encode back to {code}");
    }
    let mut codes: Vec<u32> = table.iter().map(|(c, _)| *c).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), table.len(), "two easings share one code");
    assert_eq!(
        codes.last().copied(),
        Some(animatix_plugin_api::NATIVE_EASING_MAX),
        "NATIVE_EASING_MAX no longer names the highest tabled code"
    );

    // Unknown in both directions must be loud, never linear.
    assert_eq!(native_easing(animatix_plugin_api::NATIVE_EASING_MAX + 1), None);
    assert_eq!(native_easing(NATIVE_EASING_UNSUPPORTED), None);
    assert_eq!(native_easing(4242), None);
    assert_eq!(
        easing_code(Easing::Spring {
            damping: 6.0,
            frequency: 9.0
        }),
        NATIVE_EASING_UNSUPPORTED
    );
    assert_eq!(
        easing_code(Easing::CubicBezier([0.1, 0.2, 0.3, 0.4])),
        NATIVE_EASING_UNSUPPORTED
    );
    assert_ne!(
        easing_code(Easing::Spring {
            damping: 6.0,
            frequency: 9.0
        }),
        NATIVE_EASING_LINEAR,
        "a parameterized curve was reported as linear"
    );
}

static BUILD_CHILD_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

unsafe extern "C" fn action_write_test(ctx: *mut NativeActionContext) -> i32 {
    let ctx = unsafe { &*ctx };
    if unsafe { (ctx.get_target_count)(ctx.host) } != 1 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let mut target = std::ptr::null();
    if unsafe { (ctx.get_target)(ctx.host, 0, &mut target) } != NATIVE_STATUS_OK {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let value = NativeValue {
        tag: NATIVE_VALUE_NUM,
        num: 7.0,
        ..NativeValue::default()
    };
    unsafe { (ctx.write_keyframe)(ctx.host, target, c"glow".as_ptr(), value, 0, 1000, 0) }
}

unsafe extern "C" fn record_build_children(ctx: *mut NativePrimitiveBuildCtx) -> i32 {
    let ctx = unsafe { &*ctx };
    BUILD_CHILD_COUNT
        .store(unsafe { (ctx.get_child_count)(ctx.host) }, std::sync::atomic::Ordering::SeqCst);
    NATIVE_STATUS_OK
}

fn sample_evaluate_ctx<'a>(
    track: &'a crate::timeline::AnimationTrack,
    asset_cache: &'a crate::timeline::assets::AssetCache,
) -> EvaluateCtx<'a> {
    EvaluateCtx {
        track,
        time_ms: 0,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: crate::timeline::SceneDimensions {
            width: 640,
            height: 360,
        },
        background_color: [0.0; 4],
        overrides: None,
        vector_paths: &[],
        asset_cache,
        target_resolver: None,
    }
}

#[test]
fn native_values_round_trip_finite_runtime_values() {
    let cases = [
        Value::Num(3.5),
        Value::Bool(true),
        Value::Vec2([1.0, 2.0]),
        Value::Vec3([1.0, 2.0, 3.0]),
        Value::Vec4([1.0, 2.0, 3.0, 4.0]),
        Value::Color([0.1, 0.2, 0.3, 0.4]),
        Value::Str("hello".to_string()),
        Value::List(vec![Value::Num(1.0), Value::Str("two".to_string())].into()),
    ];
    for value in cases {
        let mut arena = NativeValueArena::default();
        assert_eq!(
            native_to_value(value_to_native(&value, &mut arena).expect("convert"))
                .expect("restore"),
            value
        );
    }
}

#[test]
fn native_function_context_reads_env_and_service() {
    let mut env = Environment::new();
    env.set("answer", Value::Num(42.0));
    let service_values = HashMap::from([("demo.pulse".to_string(), 7usize)]);
    let mut host = NativeFunctionHost {
        env: &env,
        service_values: &service_values,
        arena: NativeValueArena::default(),
    };
    let mut out = NativeValue::default();
    assert_eq!(
        unsafe {
            native_function_get_env(
                (&mut host as *mut NativeFunctionHost).cast::<c_void>(),
                c"answer".as_ptr(),
                &mut out,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(out.tag, NATIVE_VALUE_NUM);
    assert_eq!(out.num, 42.0);

    let mut service = 0;
    assert_eq!(
        unsafe {
            native_function_get_service(
                (&mut host as *mut NativeFunctionHost).cast::<c_void>(),
                c"demo.pulse".as_ptr(),
                &mut service,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(service, 7);
}

#[test]
fn native_action_context_writes_extension_keyframe() {
    use animatix_syntax::schema::PropertyValueKind;

    let mut ctx = ExtensionContext::new();
    let id = ctx
        .register_property("Pulse", "glow", PropertyValueKind::F32, true)
        .expect("register property");
    let mut timeline = crate::timeline::Timeline::new();
    let mut track = crate::timeline::AnimationTrack::placeholder("p".to_string());
    track.actor_type = "Pulse".to_string();
    timeline.world.tracks.insert("p".to_string(), track);

    let adapter = NativeActionAdapter {
        signature: ActionSignature {
            name: "pulse".to_string(),
            category: "Native".to_string(),
            description: "Test action".to_string(),
            params: vec![],
            modifiers: vec![],
        },
        callback: action_write_test,
        property_ids: HashMap::from([("glow".to_string(), id)]),
        property_kinds: HashMap::from([("glow".to_string(), PropertyValueKind::F32)]),
        service_values: HashMap::new(),
        _library: Arc::new(()) as Arc<dyn Any + Send + Sync>,
    };
    let action = crate::ast::Action {
        verb: "pulse".to_string(),
        targets: vec!["p".to_string()],
        args: vec![],
        modifiers: vec![],
        byte_span: None,
        target_index: vec![],
    };
    let mut diagnostics = Vec::new();
    adapter.execute(&action, 0.0, &mut timeline, &mut diagnostics);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
    assert_eq!(
        crate::timeline::property_engine::read_property_plan_slot(
            timeline.world.tracks.get("p").expect("track"),
            id,
            500,
        ),
        Some(crate::timeline::PropertyValue::F32(7.0))
    );
}

#[test]
fn property_kind_codes_cover_dynamic_track_kinds() {
    for kind in [
        PropertyKind::F32,
        PropertyKind::U32,
        PropertyKind::Bool,
        PropertyKind::Vec2,
        PropertyKind::Vec4,
        PropertyKind::String,
        PropertyKind::PointList,
        PropertyKind::Generic,
    ] {
        assert!(native_property_kind(property_kind_code(kind)).is_some());
    }
    assert!(native_property_kind(99).is_none());
}

#[test]
fn native_enum_type_and_property_kind_are_supported() {
    use animatix_plugin_api::NATIVE_PROPERTY_ENUM;
    use animatix_syntax::typing::Type;

    assert_eq!(
        parse_native_type("Enum(left, right, top)"),
        Some(Type::Enum(vec!["left".to_string(), "right".to_string(), "top".to_string()]))
    );
    assert_eq!(
        native_property_kind(NATIVE_PROPERTY_ENUM),
        Some(animatix_syntax::schema::PropertyValueKind::Generic)
    );
}

fn property_kind_code(kind: PropertyKind) -> u32 {
    match kind {
        PropertyKind::F32 => NATIVE_PROPERTY_F32,
        PropertyKind::U32 => NATIVE_PROPERTY_U32,
        PropertyKind::Bool => NATIVE_PROPERTY_BOOL,
        PropertyKind::Vec2 => NATIVE_PROPERTY_VEC2,
        PropertyKind::Vec4 => NATIVE_PROPERTY_VEC4,
        PropertyKind::String => NATIVE_PROPERTY_STRING,
        PropertyKind::PointList => NATIVE_PROPERTY_POINT_LIST,
        PropertyKind::Generic => NATIVE_PROPERTY_GENERIC,
    }
}

unsafe extern "C" fn double(
    _ctx: *mut NativeFunctionContext,
    args: *const NativeValue,
    arg_len: usize,
    out: *mut NativeValue,
) -> i32 {
    if arg_len != 1 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let arg = unsafe { &*args };
    if arg.tag != NATIVE_VALUE_NUM {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let result = NativeValue {
        tag: NATIVE_VALUE_NUM,
        num: arg.num * 2.0,
        ..NativeValue::default()
    };
    unsafe {
        *out = result;
    }
    NATIVE_STATUS_OK
}

unsafe extern "C" fn pulse_action(_ctx: *mut NativeActionContext) -> i32 {
    NATIVE_STATUS_OK
}

unsafe extern "C" fn pulse_evaluate(ctx: *mut NativePrimitiveEvaluateCtx) -> i32 {
    let ctx = unsafe { &*ctx };
    let Some(append_path) = ctx.append_path else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let command = NativePathCommand {
        kind: NATIVE_PATH_ELLIPSE,
        x: 0.0,
        y: 0.0,
        width: 60.0,
        height: 60.0,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 0.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [0.2, 0.8, 1.0, 1.0],
        stroke: [1.0, 1.0, 1.0, 1.0],
        stroke_width: 2.0,
        line_cap: 0,
        line_join: 0,
    };
    unsafe { (append_path)(ctx.host, command) }
}

unsafe extern "C" fn drop_service(_value: usize) {
    DROPPED_SERVICES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

static DROPPED_SERVICES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[test]
fn native_host_callbacks_install_and_dispose_capabilities() {
    use animatix_plugin_api::{NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC};

    let mut ctx = ExtensionContext::new();
    let mut host = NativeHost {
        ctx: &mut ctx,
        library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
        properties: Vec::new(),
        property_ids: HashMap::new(),
        property_kinds: HashMap::new(),
        service_values: HashMap::new(),
        functions: Vec::new(),
        primitives: Vec::new(),
        actions: Vec::new(),
        services: Vec::new(),
        effects: Vec::new(),
    };
    let mut property_id = 0;
    assert_eq!(
        unsafe {
            native_register_property(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativePropertyDescriptor {
                    actor_type: c"Rect".as_ptr(),
                    name: c"glow".as_ptr(),
                    display_name: std::ptr::null(),
                    kind: NATIVE_PROPERTY_F32,
                    type_info: std::ptr::null(),
                    injectable: true,
                    group: std::ptr::null(),
                    help: std::ptr::null(),
                },
                &mut property_id,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_ne!(property_id, 0);
    assert_eq!(
        unsafe {
            native_register_function(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativeFunctionDescriptor {
                    name: c"double".as_ptr(),
                    params: std::ptr::null(),
                    param_len: 0,
                    return_type: std::ptr::null(),
                    help: std::ptr::null(),
                    callback: double,
                },
            )
        },
        NATIVE_STATUS_OK
    );
    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: Some(pulse_evaluate),
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: None,
        equation_fragment: None,
    };
    assert_eq!(
        unsafe {
            native_register_primitive((&mut host as *mut NativeHost).cast::<c_void>(), primitive)
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(
        unsafe {
            native_register_action(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativeAction {
                    name: c"pulse".as_ptr(),
                    category: c"Native".as_ptr(),
                    description: c"Demo native pulse action".as_ptr(),
                    params: std::ptr::null(),
                    param_len: 0,
                    modifiers: std::ptr::null(),
                    modifier_len: 0,
                    execute: pulse_action,
                },
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(
        unsafe {
            native_provide_service(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativeService {
                    name: c"demo.pulse".as_ptr(),
                    type_info: std::ptr::null(),
                    help: std::ptr::null(),
                    value: 7,
                    drop: Some(drop_service),
                },
            )
        },
        NATIVE_STATUS_OK
    );
    let NativeHost {
        ctx,
        properties,
        functions,
        primitives,
        actions,
        services,
        ..
    } = host;
    assert_eq!(properties, vec![("Rect".to_string(), "glow".to_string())]);
    assert_eq!(functions, vec!["double".to_string()]);
    assert_eq!(primitives, vec!["Pulse".to_string()]);
    assert_eq!(actions, vec!["pulse".to_string()]);
    assert_eq!(services, vec!["demo.pulse".to_string()]);

    let mut env = Environment::new();
    ctx.install_functions(&mut env);
    let Some(Value::NativeFn(call)) = env.get("double") else {
        panic!("double was not installed");
    };
    assert_eq!(
        call(&[Value::Num(21.0)], &Environment::new()).expect("call native function"),
        Value::Num(42.0)
    );
    DROPPED_SERVICES.store(0, std::sync::atomic::Ordering::SeqCst);
    assert!(ctx.primitive_registry().find("Pulse").is_some());
    assert!(ctx.action("pulse").is_some());
    assert!(ctx.get::<NativeServiceHandle>("demo.pulse").is_some());

    assert!(ctx.remove_property("Rect", "glow"));
    assert!(ctx.remove_function("double"));
    assert!(ctx.remove_primitive("Pulse"));
    assert!(ctx.remove_action("pulse"));
    assert!(ctx.remove_service("demo.pulse"));
    assert_eq!(DROPPED_SERVICES.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(ctx.primitive_registry().find("Pulse").is_none());
    assert!(ctx.action("pulse").is_none());
    assert!(ctx.get::<NativeServiceHandle>("demo.pulse").is_none());
}

#[test]
fn native_host_rollback_removes_partial_registrations() {
    use animatix_plugin_api::{NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC};

    let mut ctx = ExtensionContext::new();
    let mut host = NativeHost {
        ctx: &mut ctx,
        library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
        properties: Vec::new(),
        property_ids: HashMap::new(),
        property_kinds: HashMap::new(),
        service_values: HashMap::new(),
        functions: Vec::new(),
        primitives: Vec::new(),
        actions: Vec::new(),
        services: Vec::new(),
        effects: Vec::new(),
    };
    let mut property_id = 0;
    assert_eq!(
        unsafe {
            native_register_property(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativePropertyDescriptor {
                    actor_type: c"Pulse".as_ptr(),
                    name: c"glow".as_ptr(),
                    display_name: std::ptr::null(),
                    kind: NATIVE_PROPERTY_F32,
                    type_info: std::ptr::null(),
                    injectable: true,
                    group: std::ptr::null(),
                    help: std::ptr::null(),
                },
                &mut property_id,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(
        unsafe {
            native_register_function(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativeFunctionDescriptor {
                    name: c"double".as_ptr(),
                    params: std::ptr::null(),
                    param_len: 0,
                    return_type: std::ptr::null(),
                    help: std::ptr::null(),
                    callback: double,
                },
            )
        },
        NATIVE_STATUS_OK
    );
    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: None,
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: None,
        equation_fragment: None,
    };
    assert_eq!(
        unsafe {
            native_register_primitive((&mut host as *mut NativeHost).cast::<c_void>(), primitive)
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(
        unsafe {
            native_register_action(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativeAction {
                    name: c"pulse".as_ptr(),
                    category: c"Native".as_ptr(),
                    description: c"Demo native pulse action".as_ptr(),
                    params: std::ptr::null(),
                    param_len: 0,
                    modifiers: std::ptr::null(),
                    modifier_len: 0,
                    execute: pulse_action,
                },
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(
        unsafe {
            native_provide_service(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativeService {
                    name: c"demo.pulse".as_ptr(),
                    type_info: std::ptr::null(),
                    help: std::ptr::null(),
                    value: 7,
                    drop: Some(drop_service),
                },
            )
        },
        NATIVE_STATUS_OK
    );

    host.rollback();

    assert!(ctx.property_spec("Pulse", "glow").is_none());
    assert!(ctx.action("pulse").is_none());
    assert!(ctx.get::<NativeServiceHandle>("demo.pulse").is_none());
    assert!(ctx.primitive_registry().find("Pulse").is_none());
    let mut env = Environment::new();
    ctx.install_functions(&mut env);
    assert!(env.get("double").is_none());
}

#[test]
fn native_register_property_rejects_same_name_across_actor_types() {
    use animatix_plugin_api::NATIVE_PROPERTY_F32;

    let mut ctx = ExtensionContext::new();
    let mut host = NativeHost {
        ctx: &mut ctx,
        library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
        properties: Vec::new(),
        property_ids: HashMap::new(),
        property_kinds: HashMap::new(),
        service_values: HashMap::new(),
        functions: Vec::new(),
        primitives: Vec::new(),
        actions: Vec::new(),
        services: Vec::new(),
        effects: Vec::new(),
    };
    let mut first_id = 0;
    assert_eq!(
        unsafe {
            native_register_property(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativePropertyDescriptor {
                    actor_type: c"Pulse".as_ptr(),
                    name: c"glow".as_ptr(),
                    display_name: std::ptr::null(),
                    kind: NATIVE_PROPERTY_F32,
                    type_info: std::ptr::null(),
                    injectable: true,
                    group: std::ptr::null(),
                    help: std::ptr::null(),
                },
                &mut first_id,
            )
        },
        NATIVE_STATUS_OK
    );
    // Reusing the same name on a different actor type must be rejected
    // (the internal name-keyed maps could not disambiguate it).
    let mut second_id = 0;
    assert_eq!(
        unsafe {
            native_register_property(
                (&mut host as *mut NativeHost).cast::<c_void>(),
                NativePropertyDescriptor {
                    actor_type: c"Rect".as_ptr(),
                    name: c"glow".as_ptr(),
                    display_name: std::ptr::null(),
                    kind: NATIVE_PROPERTY_F32,
                    type_info: std::ptr::null(),
                    injectable: true,
                    group: std::ptr::null(),
                    help: std::ptr::null(),
                },
                &mut second_id,
            )
        },
        NATIVE_STATUS_TYPE_ERROR
    );
    assert_eq!(host.property_ids.len(), 1);
    assert_eq!(host.property_ids["glow"], animatix_syntax::schema::PropertyId(first_id));
}

#[test]
fn native_path_commands_become_render_commands() {
    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::new();
    let service_values = HashMap::new();
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let command = NativePathCommand {
        kind: NATIVE_PATH_RECT,
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 0.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [0.0, 0.0, 0.0, 1.0],
        stroke: [1.0, 1.0, 1.0, 1.0],
        stroke_width: 1.0,
        line_cap: 0,
        line_join: 0,
    };
    assert_eq!(
        unsafe {
            native_append_path(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                command,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(host.commands.len(), 1);
}

#[test]
fn native_append_text_produces_text_command() {
    use animatix_plugin_api::NATIVE_TEXT_KIND_TEXT;

    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::new();
    let service_values = HashMap::new();
    let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
    let mut text_compiler = crate::renderer::text::TextCompiler::new();
    let text_ctx = crate::primitives::TextCompileCtx {
        text_compiler: &mut text_compiler,
        font_context: &font_context,
    };
    let (text_compiler, font_context) =
        (Some(&mut *text_ctx.text_compiler), Some(text_ctx.font_context));
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler,
        font_context,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let command = NativeTextCommand {
        content: c"Pulse".as_ptr(),
        content_len: 5,
        font_family: c"".as_ptr(),
        font_size: 28.0,
        font_weight: 600.0,
        font_style: c"normal".as_ptr(),
        line_height: 1.2,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        color: [1.0, 1.0, 1.0, 1.0],
        max_width: 0.0,
        text_align: c"center".as_ptr(),
        overflow: c"visible".as_ptr(),
        kind: NATIVE_TEXT_KIND_TEXT,
        x: 0.0,
        y: 0.0,
    };
    assert_eq!(
        unsafe {
            native_append_text(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                command,
            )
        },
        NATIVE_STATUS_OK
    );
    assert!(matches!(
        host.commands.as_slice(),
        [crate::primitives::RenderCommand::Text { .. }]
    ));
}

#[test]
fn native_append_highlight_produces_highlight_command() {
    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::new();
    let service_values = HashMap::new();
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let command = NativeHighlightCommand {
        rect: [-10.0, -10.0, 10.0, 10.0],
        color: [0.0, 0.0, 1.0, 1.0],
        alpha: 0.5,
        corner_radius: 2.0,
        blend: 2,
    };
    assert_eq!(
        unsafe {
            native_append_highlight(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                command,
            )
        },
        NATIVE_STATUS_OK
    );
    assert!(matches!(
        host.commands.as_slice(),
        [crate::primitives::RenderCommand::HighlightLayer { .. }]
    ));
}

#[test]
fn native_append_image_uses_actor_image() {
    let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let image = crate::timeline::image::SceneImage {
        data: vello::peniko::ImageData {
            data: vello::peniko::Blob::from(vec![0u8, 0, 0, 255]),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        },
        natural_size: [1.0, 1.0],
    };
    track.ensure_image_payload().image.ensure(None).add_keyframe(
        0,
        Some(image),
        crate::easing::Easing::Linear,
    );
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::new();
    let service_values = HashMap::new();
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let command = NativeImageCommand {
        url: std::ptr::null(),
        natural_size: [100.0, 50.0],
        x: 0.0,
        y: 0.0,
    };
    assert_eq!(
        unsafe {
            native_append_image(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                command,
            )
        },
        NATIVE_STATUS_OK
    );
    assert!(matches!(
        host.commands.as_slice(),
        [crate::primitives::RenderCommand::Image { .. }]
    ));
}

#[test]
fn native_append_image_rejects_uncached_explicit_url() {
    let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let image = crate::timeline::image::SceneImage {
        data: vello::peniko::ImageData {
            data: vello::peniko::Blob::from(vec![0u8, 0, 0, 255]),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        },
        natural_size: [1.0, 1.0],
    };
    track.ensure_image_payload().image.ensure(None).add_keyframe(
        0,
        Some(image),
        crate::easing::Easing::Linear,
    );
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::new();
    let service_values = HashMap::new();
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let url = std::ffi::CString::new("missing.png").expect("cstring");
    let command = NativeImageCommand {
        url: url.as_ptr(),
        natural_size: [100.0, 50.0],
        x: 0.0,
        y: 0.0,
    };
    assert_eq!(
        unsafe {
            native_append_image(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                command,
            )
        },
        NATIVE_STATUS_TYPE_ERROR
    );
    assert!(
        host.commands.is_empty(),
        "explicit uncached URL must not silently fall back to the actor image"
    );
}

#[test]
fn native_append_image_resolves_cached_url() {
    let dir = std::env::temp_dir().join("animatix_native_image_url_tests");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join(format!("native_image_url_{}.png", std::process::id()));
    let rgba = ::image::RgbaImage::from_raw(2, 2, vec![255; 16]).expect("rgba pixels");
    rgba.save(&path).expect("save image");

    let mut asset_cache = crate::timeline::assets::AssetCache::new();
    asset_cache.load_image_for(&path.to_string_lossy(), "p").expect("cache image");

    let track = crate::timeline::AnimationTrack::placeholder("p".to_string());
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::new();
    let service_values = HashMap::new();
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let url = std::ffi::CString::new(path.to_string_lossy().into_owned()).expect("cstring");
    let command = NativeImageCommand {
        url: url.as_ptr(),
        natural_size: [40.0, 20.0],
        x: 0.0,
        y: 0.0,
    };
    assert_eq!(
        unsafe {
            native_append_image(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                command,
            )
        },
        NATIVE_STATUS_OK
    );
    assert!(matches!(
        host.commands.as_slice(),
        [crate::primitives::RenderCommand::Image { natural_size, .. }]
            if *natural_size == [40.0, 20.0]
    ));
}

#[test]
fn native_path_commands_support_curve_and_rounded_rect() {
    let cubic = NativePathCommand {
        kind: NATIVE_PATH_CUBIC,
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
        x1: 2.0,
        y1: 2.0,
        x2: 8.0,
        y2: 8.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 0.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [1.0, 1.0, 1.0, 1.0],
        stroke: [0.0; 4],
        stroke_width: 0.0,
        line_cap: 0,
        line_join: 0,
    };
    assert!(native_path_to_vello(&cubic).is_some());

    let rounded = NativePathCommand {
        kind: NATIVE_PATH_ROUNDED_RECT,
        x: 0.0,
        y: 0.0,
        width: 20.0,
        height: 10.0,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 3.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [1.0, 1.0, 1.0, 1.0],
        stroke: [0.0; 4],
        stroke_width: 0.0,
        line_cap: 0,
        line_join: 0,
    };
    assert!(native_path_to_vello(&rounded).is_some());
}

#[test]
fn native_get_property_reads_extension_slot() {
    let mut ctx = ExtensionContext::new();
    let id = ctx
        .register_property("Pulse", "glow", animatix_syntax::schema::PropertyValueKind::F32, true)
        .expect("register property");
    let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    crate::timeline::property_engine::write_property_plan_slot(
        &mut track,
        id,
        PropertyKind::F32,
        crate::timeline::PropertyValue::F32(0.75),
        0,
        0,
        crate::easing::Easing::Linear,
    );
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
    let property_ids = HashMap::from([("glow".to_string(), id)]);
    let service_values = HashMap::new();
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let mut out = NativeValue::default();
    let status = unsafe {
        native_get_property(
            (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
            c"glow".as_ptr(),
            &mut out,
        )
    };
    assert_eq!(status, NATIVE_STATUS_OK);
    assert_eq!(out.tag, NATIVE_VALUE_NUM);
    assert!((out.num - 0.75).abs() < 1e-9);
}

#[test]
fn native_build_exposes_props_modifiers_and_children() {
    let mut timeline = crate::timeline::Timeline::new();
    let mut diagnostics = Vec::new();
    let props = vec![Property::new("level", Expr::Num(42.0))];
    let modifiers = vec![Modifier {
        name: Some("ease".to_string()),
        value: Expr::Ident("bounce".to_string()),
    }];
    let children = vec![InlineItem::Labeled {
        label: "kid".to_string(),
        array_index: None,
        ty: "Rect".to_string(),
        props: vec![Property::new(
            "size",
            Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(10.0)]),
        )],
        modifiers: Vec::new(),
        children: Vec::new(),
    }];
    let mut host = NativePrimitiveBuildHost::new(
        &mut timeline,
        0.0,
        None,
        "pulse",
        &props,
        &modifiers,
        &children,
        &mut diagnostics,
    );
    assert_eq!(
        unsafe {
            native_build_get_property_count(
                (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
            )
        },
        1
    );
    let mut property = NativePropertyValue {
        name: std::ptr::null(),
        value: NativeValue::default(),
    };
    assert_eq!(
        unsafe {
            native_build_get_property(
                (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                0,
                &mut property,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(unsafe { read_c_string(property.name) }.as_deref(), Some("level"));
    assert_eq!(property.value.tag, NATIVE_VALUE_NUM);

    assert_eq!(
        unsafe {
            native_build_get_modifier_count(
                (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
            )
        },
        1
    );
    let mut modifier = NativeModifierValue {
        name: std::ptr::null(),
        value: NativeValue::default(),
    };
    assert_eq!(
        unsafe {
            native_build_get_modifier(
                (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                0,
                &mut modifier,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(unsafe { read_c_string(modifier.name) }.as_deref(), Some("ease"));

    assert_eq!(
        unsafe {
            native_build_get_child_count(
                (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
            )
        },
        1
    );
    let mut child = NativeChild {
        label: std::ptr::null(),
        type_name: std::ptr::null(),
        properties: std::ptr::null(),
        property_len: 0,
    };
    assert_eq!(
        unsafe {
            native_build_get_child(
                (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                0,
                &mut child,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(unsafe { read_c_string(child.label) }.as_deref(), Some("kid"));
    assert_eq!(unsafe { read_c_string(child.type_name) }.as_deref(), Some("Rect"));
}

#[test]
fn native_assignment_write_keyframe_writes_extension_slot() {
    let mut ctx = ExtensionContext::new();
    let id = ctx
        .register_property("Pulse", "glow", animatix_syntax::schema::PropertyValueKind::F32, true)
        .expect("register property");
    let property_ids = HashMap::from([("glow".to_string(), id)]);
    let property_kinds =
        HashMap::from([("glow".to_string(), animatix_syntax::schema::PropertyValueKind::F32)]);
    let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let mut host = NativeAssignmentHost {
        track: &mut track,
        property_ids: &property_ids,
        property_kinds: &property_kinds,
        value: NativeValue {
            tag: NATIVE_VALUE_NUM,
            num: 0.5,
            ..NativeValue::default()
        },
    };
    assert_eq!(
        unsafe {
            native_assignment_write_keyframe(
                (&mut host as *mut NativeAssignmentHost).cast::<c_void>(),
                c"glow".as_ptr(),
                NativeValue {
                    tag: NATIVE_VALUE_NUM,
                    num: 0.75,
                    ..NativeValue::default()
                },
                0,
                1000,
                0,
            )
        },
        NATIVE_STATUS_OK
    );
    assert_eq!(
        crate::timeline::property_engine::read_property_plan_slot(&track, id, 500),
        Some(crate::timeline::PropertyValue::F32(0.75))
    );
}

#[test]
fn native_build_through_timeline_processes_children() {
    use animatix_plugin_api::{
        NATIVE_PRIMITIVE_CATEGORY_CONTAINER, NATIVE_PRIMITIVE_CHILD_GENERIC,
    };

    let (ast, errors) = animatix_syntax::parser::parse_source("#0s\np: Pulse { kid: Rect }");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_CONTAINER,
        capabilities: NATIVE_CAP_IS_CONTAINER,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: Some(record_build_children),
        evaluate: None,
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: None,
        equation_fragment: None,
    };
    let adapter = NativePrimitiveAdapter::new(
        primitive,
        Arc::new(()) as Arc<dyn Any + Send + Sync>,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )
    .expect("adapter");
    let mut ctx = ExtensionContext::new();
    let adapter_info =
        animatix_std::PrimitiveInfo::extension(adapter.type_name.clone(), adapter.info.category);
    ctx.register_primitive(Arc::new(adapter), adapter_info)
        .expect("register primitive");
    BUILD_CHILD_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);

    let report = crate::timeline::Timeline::build_with_context(
        &ast,
        &std::collections::HashMap::new(),
        Arc::new(ctx),
    );
    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let p_id = report.output.get_entity_id("p").expect("pulse track");
    let children: Vec<&str> = report.output.world.child_labels(p_id).collect();
    assert_eq!(children, vec!["kid"]);
    assert_eq!(BUILD_CHILD_COUNT.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn native_primitive_evaluate_emits_render_commands() {
    use animatix_plugin_api::{NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC};

    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: Some(pulse_evaluate),
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: None,
        equation_fragment: None,
    };
    let adapter = NativePrimitiveAdapter::new(
        primitive,
        Arc::new(()) as Arc<dyn Any + Send + Sync>,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )
    .expect("adapter");
    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let ctx = sample_evaluate_ctx(&track, &asset_cache);
    let commands = adapter.evaluate(&ctx, None).expect("native evaluate").expect("native commands");
    assert!(matches!(commands.as_slice(), [crate::primitives::RenderCommand::Paths { .. }]));
}

#[test]
fn native_primitive_clip_path_and_equation_fragment_flow_through_adapter() {
    use animatix_plugin_api::{
        NATIVE_PATH_RECT, NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        NativeClipPathCtx, NativePrimitiveEquationFragmentCtx,
    };

    use crate::primitives::Primitive;

    fn rect_command(x: f64) -> NativePathCommand {
        NativePathCommand {
            kind: NATIVE_PATH_RECT,
            x,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
            points: std::ptr::null(),
            point_len: 0,
            radius: 0.0,
            start_angle: 0.0,
            sweep_angle: 0.0,
            fill: [1.0, 1.0, 1.0, 1.0],
            stroke: [0.0; 4],
            stroke_width: 0.0,
            line_cap: 0,
            line_join: 0,
        }
    }

    unsafe extern "C" fn pulse_clip_path(ctx: *mut NativeClipPathCtx) -> i32 {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let Some(append) = ctx.append_path else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        // Two disjoint rects: both must survive into the clip geometry.
        for x in [0.0, 40.0] {
            let status = unsafe { append(ctx.host, rect_command(x)) };
            if status != NATIVE_STATUS_OK {
                return status;
            }
        }
        NATIVE_STATUS_OK
    }

    unsafe extern "C" fn pulse_equation_fragment(
        ctx: *mut NativePrimitiveEquationFragmentCtx,
    ) -> i32 {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let Some(write) = ctx.write_content else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let content = c"x^2";
        let status = unsafe { write(ctx.host, content.as_ptr(), content.to_bytes().len()) };
        if status != NATIVE_STATUS_OK {
            return status;
        }
        ctx.highlight_color = [1.0, 0.0, 0.0, 1.0];
        ctx.highlight_opacity = 0.5;
        NATIVE_STATUS_OK
    }

    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: None,
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: Some(pulse_clip_path),
        equation_fragment: Some(pulse_equation_fragment),
    };
    let adapter = NativePrimitiveAdapter::new(
        primitive,
        Arc::new(()) as Arc<dyn Any + Send + Sync>,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )
    .expect("adapter");

    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let ctx = sample_evaluate_ctx(&track, &asset_cache);

    let clip = adapter
        .clip_path(&ctx, &crate::timeline::ActorCaps::default())
        .expect("native clip path");
    // Two rects (5 path elements each) must both be present.
    assert!(
        clip.elements().len() >= 10,
        "clip must contain both rects, got {} elements",
        clip.elements().len()
    );

    let fragment = adapter.equation_fragment(&ctx).expect("native equation fragment");
    assert_eq!(fragment.content, "x^2");
    assert_eq!(fragment.highlight_color, [1.0, 0.0, 0.0, 1.0]);
    assert!((fragment.highlight_opacity - 0.5).abs() < 1e-6);
}

#[test]
fn native_primitive_default_props_and_color_key_flow_through_adapter() {
    use animatix_plugin_api::{
        NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC, NativeDefaultPropsCtx,
    };

    use crate::primitives::Primitive;

    unsafe extern "C" fn pulse_default_props(ctx: *mut NativeDefaultPropsCtx) -> i32 {
        let Some(ctx) = (unsafe { ctx.as_mut() }) else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let Some(append) = ctx.append_property else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let mut arena = NativeValueArena::default();
        let color_name = std::ffi::CString::new("color").unwrap();
        let color =
            value_to_native(&crate::timeline::Value::Color([1.0, 0.5, 0.25, 1.0]), &mut arena)
                .expect("color value");
        let status = unsafe { append(ctx.host, color_name.as_ptr(), color) };
        if status != NATIVE_STATUS_OK {
            return status;
        }
        let size_name = std::ffi::CString::new("size").unwrap();
        let size = value_to_native(&crate::timeline::Value::Vec2([120.0, 80.0]), &mut arena)
            .expect("size value");
        unsafe { append(ctx.host, size_name.as_ptr(), size) }
    }

    unsafe extern "C" fn pulse_default_color_key(
        property: *const c_char,
        out: *mut *const c_char,
    ) -> i32 {
        let Some(property) = (unsafe { read_c_string(property) }) else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        if property == "color" {
            // A `'static` pointer owned by the test binary — matches the
            // callback contract (plugin-lifetime-valid strings).
            unsafe { *out = c"accent.primary".as_ptr() };
            NATIVE_STATUS_OK
        } else {
            NATIVE_STATUS_UNSUPPORTED
        }
    }

    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: None,
        handle_assignment: None,
        finalize_container_build: None,
        default_props: Some(pulse_default_props),
        default_color_key: Some(pulse_default_color_key),
        clip_path: None,
        equation_fragment: None,
    };
    let adapter = NativePrimitiveAdapter::new(
        primitive,
        Arc::new(()) as Arc<dyn Any + Send + Sync>,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )
    .expect("adapter");

    let dims = crate::timeline::SceneDimensions {
        width: 640,
        height: 480,
    };
    let props = adapter.default_props(&dims);
    let color = props.iter().find(|p| p.name == "color").expect("color default");
    assert!(
        matches!(&color.value, crate::ast::Expr::Tuple(items) if items.len() == 4),
        "color default should be a 4-tuple"
    );
    let size = props.iter().find(|p| p.name == "size").expect("size default");
    assert!(
        matches!(&size.value, crate::ast::Expr::Tuple(items) if items.len() == 2),
        "size default should be a 2-tuple"
    );

    assert_eq!(adapter.default_color_key("color", &Default::default()), Some("accent.primary"));
    assert_eq!(adapter.default_color_key("stroke", &Default::default()), None);
}

#[test]
fn native_primitive_metadata_maps_all_abi_codes() {
    use animatix_plugin_api::{
        NATIVE_CAP_IMAGE_PAYLOAD, NATIVE_CAP_IS_CONTAINER, NATIVE_CAP_IS_SHAPE,
        NATIVE_CAP_LAYOUT_CONTAINER, NATIVE_CAP_MORPHABLE_PATHS, NATIVE_CAP_PLOT_GEOMETRY,
        NATIVE_CAP_PLOT_HOST, NATIVE_CAP_TEXT_PATHS, NATIVE_CAP_VECTOR_PATHS,
        NATIVE_CAP_VECTOR_REVEAL_TARGET, NATIVE_PRIMITIVE_CATEGORY_ANNOTATION,
        NATIVE_PRIMITIVE_CATEGORY_CONTAINER, NATIVE_PRIMITIVE_CATEGORY_MEDIA,
        NATIVE_PRIMITIVE_CATEGORY_PLOT, NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        NATIVE_PRIMITIVE_CATEGORY_TEXT, NATIVE_PRIMITIVE_CHILD_EQUATION,
        NATIVE_PRIMITIVE_CHILD_FILTER, NATIVE_PRIMITIVE_CHILD_GENERIC, NATIVE_PRIMITIVE_CHILD_MASK,
        NATIVE_RESIZE_MODE_SCALE,
    };

    assert_eq!(
        native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_SHAPE),
        Some(ActorCategory::Shape)
    );
    assert_eq!(
        native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_TEXT),
        Some(ActorCategory::Text)
    );
    assert_eq!(
        native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_MEDIA),
        Some(ActorCategory::Media)
    );
    assert_eq!(
        native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_PLOT),
        Some(ActorCategory::Plot)
    );
    assert_eq!(
        native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_CONTAINER),
        Some(ActorCategory::Container)
    );
    assert_eq!(
        native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_ANNOTATION),
        Some(ActorCategory::Annotation)
    );
    assert_eq!(native_primitive_category(99), None);

    assert_eq!(
        native_child_processing(NATIVE_PRIMITIVE_CHILD_GENERIC),
        Some(ChildProcessing::Generic)
    );
    assert_eq!(
        native_child_processing(NATIVE_PRIMITIVE_CHILD_FILTER),
        Some(ChildProcessing::Filter)
    );
    assert_eq!(
        native_child_processing(NATIVE_PRIMITIVE_CHILD_MASK),
        Some(ChildProcessing::Mask)
    );
    assert_eq!(
        native_child_processing(NATIVE_PRIMITIVE_CHILD_EQUATION),
        Some(ChildProcessing::Equation)
    );
    assert_eq!(native_child_processing(99), None);

    let capabilities = native_capabilities(
        NATIVE_CAP_TEXT_PATHS
            | NATIVE_CAP_VECTOR_PATHS
            | NATIVE_CAP_IMAGE_PAYLOAD
            | NATIVE_CAP_LAYOUT_CONTAINER
            | NATIVE_CAP_MORPHABLE_PATHS
            | NATIVE_CAP_VECTOR_REVEAL_TARGET
            | NATIVE_CAP_PLOT_GEOMETRY
            | NATIVE_CAP_PLOT_HOST
            | NATIVE_CAP_IS_CONTAINER
            | NATIVE_CAP_IS_SHAPE,
    );
    assert!(capabilities.text_paths);
    assert!(capabilities.vector_paths);
    assert!(capabilities.image_payload);
    assert!(capabilities.layout_container);
    assert!(capabilities.plot_geometry);
    assert!(capabilities.plot_host);
    assert!(capabilities.is_container);
    assert!(capabilities.is_shape);
    assert!(!native_capabilities(0).is_container);
    assert!(!native_capabilities(0).is_shape);

    assert_eq!(native_resize_mode(NATIVE_RESIZE_MODE_SCALE), ResizeMode::Scale);
    assert_eq!(native_resize_mode(NATIVE_RESIZE_MODE_SIZE), ResizeMode::Size);
    assert_eq!(native_resize_mode(99), ResizeMode::Size);
}

#[test]
fn native_declared_builtin_properties_use_generic_writer() {
    use animatix_plugin_api::{NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC};

    let declared = [c"position".as_ptr(), c"color".as_ptr()];
    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: declared.as_ptr(),
        property_len: declared.len(),
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: None,
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: None,
        equation_fragment: None,
    };
    let adapter = NativePrimitiveAdapter::new(
        primitive,
        Arc::new(()) as Arc<dyn Any + Send + Sync>,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )
    .expect("adapter");
    let mut ctx = ExtensionContext::new();
    let adapter_info =
        animatix_std::PrimitiveInfo::extension(adapter.type_name.clone(), adapter.info.category);
    ctx.register_primitive(Arc::new(adapter), adapter_info)
        .expect("register primitive");

    let (ast, errors) =
        animatix_syntax::parser::parse_source("p: Pulse, position: (10, 20), color: red");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");
    let report = crate::timeline::Timeline::build_with_context(
        &ast,
        &std::collections::HashMap::new(),
        Arc::new(ctx),
    );
    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let track = report.output.world.tracks.get("p").expect("pulse track");
    assert_eq!(
        track.geometry.position.get(0, [0.0, 0.0]),
        [10.0, 20.0],
        "declared position should be written by the generic property engine"
    );
    assert_eq!(
        track.style.color.get(0, [1.0, 1.0, 1.0, 1.0]),
        [1.0, 0.0, 0.0, 1.0],
        "declared color should be written by the generic property engine"
    );
}

#[test]
fn extension_actor_is_visible_from_declaration_time() {
    use animatix_plugin_api::{NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC};

    // Regression: the extension build path must set `first_seen_ms` like
    // the built-in path, otherwise render_actor_node skips the actor
    // forever (`time_ms < u64::MAX` is always true).
    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: 0,
        properties: std::ptr::null(),
        property_len: 0,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: None,
        handle_assignment: None,
        finalize_container_build: None,
        default_props: None,
        default_color_key: None,
        clip_path: None,
        equation_fragment: None,
    };
    let adapter = NativePrimitiveAdapter::new(
        primitive,
        Arc::new(()) as Arc<dyn Any + Send + Sync>,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    )
    .expect("adapter");
    let mut ctx = ExtensionContext::new();
    let adapter_info =
        animatix_std::PrimitiveInfo::extension(adapter.type_name.clone(), adapter.info.category);
    ctx.register_primitive(Arc::new(adapter), adapter_info)
        .expect("register primitive");

    let (ast, errors) = animatix_syntax::parser::parse_source("p: Pulse, opacity: 0.25");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");
    let report = crate::timeline::Timeline::build_with_context(
        &ast,
        &std::collections::HashMap::new(),
        Arc::new(ctx),
    );
    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let track = report.output.world.tracks.get("p").expect("pulse track");
    assert_eq!(
        track.first_seen_ms, 0,
        "extension actors must be visible from their declaration time"
    );
}

#[test]
fn enum_extension_property_bare_identifier_roundtrips() {
    use animatix_syntax::schema::{PropertyId, PropertyValueKind};
    use animatix_syntax::typing::Type;

    let (ast, errors) =
        animatix_syntax::parser::parse_source("p: Rect, mode: ring\n#1s\n p.mode = dot");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut ctx = ExtensionContext::new();
    let id = ctx
        .register_property_full_typed(
            "Rect",
            "mode",
            PropertyValueKind::Generic,
            Some(Type::Enum(vec!["ring".to_string(), "dot".to_string(), "cross".to_string()])),
            true,
            None,
            None,
            None,
        )
        .expect("register property");

    let report = crate::timeline::Timeline::build_with_context(
        &ast,
        &std::collections::HashMap::new(),
        Arc::new(ctx),
    );
    let track = report.output.world.tracks.get("p").expect("pulse track");
    let slot = track.property_plan.get(PropertyId(1_000_000));
    let t0 = slot.and_then(|s| s.track.sample(0));
    let t1500 = slot.and_then(|s| s.track.sample(1500));
    assert_eq!(t0, Some(crate::timeline::PropertyValue::Enum("ring".to_string())));
    assert_eq!(t1500, Some(crate::timeline::PropertyValue::Enum("dot".to_string())));

    // Read back through the native path -> must be NATIVE_VALUE_ENUM.
    let property_ids = HashMap::from([("mode".to_string(), id)]);
    let service_values = HashMap::new();
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx(track, &asset_cache);
    let mut host = NativePrimitiveEvaluateHost {
        ctx: &eval_ctx,
        property_ids: &property_ids,
        service_values: &service_values,
        text_compiler: None,
        font_context: None,
        commands: Vec::new(),
        arena: NativeValueArena::default(),
    };
    let mut out = NativeValue::default();
    let status = unsafe {
        native_get_property(
            (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
            c"mode".as_ptr(),
            &mut out,
        )
    };
    assert_eq!(status, NATIVE_STATUS_OK);
    assert_eq!(out.tag, NATIVE_VALUE_ENUM);
    assert_eq!(
        unsafe { read_c_string_len(out.string, out.string_len) }.as_deref(),
        Some("ring")
    );
}

#[test]
fn native_register_effect_registers_and_rejects_bad_layout() {
    use animatix_plugin_api::{
        NATIVE_EFFECT_PARAM_KIND_F32, NATIVE_EFFECT_PARAM_KIND_VEC2, NativeEffectDescriptor,
        NativeEffectParam, NativeEffectPass,
    };

    let mut ctx = ExtensionContext::new();
    let mut host = NativeHost {
        ctx: &mut ctx,
        library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
        properties: Vec::new(),
        property_ids: HashMap::new(),
        property_kinds: HashMap::new(),
        service_values: HashMap::new(),
        functions: Vec::new(),
        primitives: Vec::new(),
        actions: Vec::new(),
        services: Vec::new(),
        effects: Vec::new(),
    };

    let params = [
        NativeEffectParam {
            name: c"amount".as_ptr(),
            kind: NATIVE_EFFECT_PARAM_KIND_F32,
            offset: 0,
            identity: [0.0; 4],
        },
        NativeEffectParam {
            name: c"centre".as_ptr(),
            kind: NATIVE_EFFECT_PARAM_KIND_VEC2,
            offset: 8,
            identity: [0.5; 4],
        },
    ];
    let passes = [NativeEffectPass {
        wgsl: c"@compute fn main() {}".as_ptr(),
        entry: c"main".as_ptr(),
    }];
    let descriptor = NativeEffectDescriptor {
        name: c"MockGlow".as_ptr(),
        display_name: c"Mock Glow".as_ptr(),
        params: params.as_ptr(),
        param_len: params.len(),
        passes: passes.as_ptr(),
        pass_len: passes.len(),
        support_px: 3.0,
    };

    let status = unsafe {
        native_register_effect((&mut host as *mut NativeHost).cast::<c_void>(), descriptor)
    };
    assert_eq!(status, NATIVE_STATUS_OK);
    assert_eq!(host.effects, vec!["MockGlow".to_string()]);

    let registered =
        crate::timeline::effects::effect_for_type("MockGlow").expect("plugin effect registered");
    assert_eq!(registered.type_name(), "MockGlow");
    assert_eq!(registered.display_name(), "Mock Glow");
    assert_eq!(registered.author_uniform_size(), 16);
    assert_eq!(registered.params().len(), 2);
    assert_eq!(registered.passes().len(), 1);

    // A misaligned parameter offset is rejected.
    let bad_params = [NativeEffectParam {
        name: c"offset".as_ptr(),
        kind: NATIVE_EFFECT_PARAM_KIND_VEC2,
        offset: 3,
        identity: [0.0; 4],
    }];
    let bad = NativeEffectDescriptor {
        name: c"BadGlow".as_ptr(),
        display_name: c"Bad Glow".as_ptr(),
        params: bad_params.as_ptr(),
        param_len: bad_params.len(),
        passes: passes.as_ptr(),
        pass_len: passes.len(),
        support_px: 0.0,
    };
    let status =
        unsafe { native_register_effect((&mut host as *mut NativeHost).cast::<c_void>(), bad) };
    assert_eq!(status, NATIVE_STATUS_TYPE_ERROR);

    // Re-registering the same name is idempotent: same slot, no duplicate.
    let again = unsafe {
        native_register_effect((&mut host as *mut NativeHost).cast::<c_void>(), descriptor)
    };
    assert_eq!(again, NATIVE_STATUS_OK);
    assert_eq!(host.effects.len(), 1);
}
