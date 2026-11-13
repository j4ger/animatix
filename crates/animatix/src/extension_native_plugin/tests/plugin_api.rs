//! Value/function/action/kind conversion tests for the native plugin ABI.

use std::ffi::c_void;

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
        target_index_position: vec![],
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
