use std::ffi::c_void;

use animatix_plugin_api::NATIVE_RESIZE_MODE_SIZE;

use super::*;
use crate::extension_context::ExtensionContext;
use crate::timeline::{Environment, PropertyKind, Value};

/// The ABI easing table must not be able to lie. Every code the host
/// advertises decodes to a curve and re-encodes to that same code, codes it
/// does not know are rejected instead of reading as `Linear`, and the two
/// parameterized curves report `UNSUPPORTED` rather than borrowing a
/// neighbour's name. Before the table was exhaustive, `bounce`/`elastic`/`back`/`expo*` all crossed the boundary as `Linear` in both directions.
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
    asset_cache: &'a crate::timeline::assets::AssetCache,
) -> EvaluateCtx<'a> {
    sample_evaluate_ctx_with_payload(asset_cache, None)
}

/// Like [`sample_evaluate_ctx`], but with the entity's payload component so
/// native commands that read primitive state (e.g. the actor image) can see it.
fn sample_evaluate_ctx_with_payload<'a>(
    asset_cache: &'a crate::timeline::assets::AssetCache,
    payload: Option<&'a crate::primitives::PrimitivePayload>,
) -> EvaluateCtx<'a> {
    EvaluateCtx {
        id: crate::timeline::EntityId(0),
        label: "pulse",
        // These helper contexts carry no World identity; the implied type is
        // Rect.
        caps: animatix_std::caps_for_type("Rect").unwrap_or_default(),
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
        highlight: Box::leak(
            Box::new(crate::timeline::animation_track::HighlightTracks::default()),
        ),
        property_plan: Box::leak(Box::new(crate::timeline::plan::PropertyPlan::default())),
        world: None,
        // The shape, text, geometry and style tiers live in World; these helper
        // contexts build no World, so default tiers are attached.
        style: Box::leak(Box::new(crate::timeline::animation_track::StyleTracks::default())),
        shape: Box::leak(Box::new(crate::timeline::animation_track::ShapeTracks::default())),
        text: Box::leak(Box::new(crate::timeline::animation_track::TextTracks::default())),
        geometry: Box::leak(Box::new(crate::timeline::animation_track::GeometryTracks::default())),
        payload,
        target_resolver: None,
        legend: None,
    }
}

/// Like [`sample_evaluate_ctx`], but with the entity's property plan so native
/// property reads can resolve registry-driven slots.
fn sample_evaluate_ctx_with_plan<'a>(
    asset_cache: &'a crate::timeline::assets::AssetCache,
    property_plan: &'a crate::timeline::plan::PropertyPlan,
) -> EvaluateCtx<'a> {
    let mut ctx = sample_evaluate_ctx(asset_cache);
    ctx.property_plan = property_plan;
    ctx
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
    let entity = timeline.world.spawn("p", "Pulse");

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
            timeline.world.property_plan(entity),
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

mod part2;
mod part3;
