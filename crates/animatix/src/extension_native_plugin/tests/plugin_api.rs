//! Value, callable, plan, and property conversion tests for the native plugin ABI.

use super::*;
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
        Some(NATIVE_EASING_EXPO_IN_OUT),
        "scalar easing codes changed"
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

#[test]
fn native_values_round_trip_finite_runtime_values() {
    let cases = [
        Value::Null,
        Value::Num(3.5),
        Value::Duration(0.5),
        Value::Unit,
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

fn contract(result: CallableType) -> CallableDescriptor {
    CallableDescriptor {
        namespace: "demo".into(),
        name: "test".into(),
        params: vec![],
        return_type: result,
        phase: CallablePhase::Build,
        help: String::new(),
        category: "Test".into(),
    }
}

unsafe extern "C" fn read_context(
    ctx: *mut NativeCallableContext,
    _args: *const NativeValue,
    _len: usize,
    out: *mut NativeValue,
) -> i32 {
    let ctx = unsafe { &*ctx };
    let mut value = NativeValue::default();
    let mut service = 0;
    if unsafe { (ctx.get_env)(ctx.host, c"answer".as_ptr(), &mut value) } != NATIVE_STATUS_OK
        || unsafe { (ctx.get_service)(ctx.host, c"demo.pulse".as_ptr(), &mut service) }
            != NATIVE_STATUS_OK
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    value.num += service as f64;
    unsafe {
        *out = value;
    }
    NATIVE_STATUS_OK
}

#[test]
fn native_callable_reads_environment_and_service() {
    let mut env = Environment::new();
    env.set("answer", Value::Num(42.0));
    let result = super::super::callable::invoke_native_callable(
        &contract(CallableType::Num),
        read_context,
        &[],
        &env,
        &HashMap::new(),
        &HashMap::from([("demo.pulse".into(), 7)]),
    );
    assert_eq!(result.unwrap(), Value::Num(49.0));
}

unsafe extern "C" fn write_plan(
    ctx: *mut NativeCallableContext,
    args: *const NativeValue,
    len: usize,
    out: *mut NativeValue,
) -> i32 {
    if len != 1 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let ctx = unsafe { &*ctx };
    let actor = unsafe { *args };
    let value = NativeValue {
        tag: NATIVE_VALUE_NUM,
        num: 7.0,
        ..NativeValue::default()
    };
    let status = unsafe {
        (ctx.append_property_op)(
            ctx.host,
            actor.handle,
            1000000,
            value,
            0.0,
            1.0,
            NativeEasing::default(),
        )
    };
    unsafe {
        *out = NativeValue {
            tag: NATIVE_VALUE_TIMELINE,
            ..NativeValue::default()
        };
    }
    status
}

unsafe extern "C" fn ignore_invalid_write(
    ctx: *mut NativeCallableContext,
    _args: *const NativeValue,
    _len: usize,
    out: *mut NativeValue,
) -> i32 {
    let ctx = unsafe { &*ctx };
    unsafe {
        (ctx.append_property_op)(
            ctx.host,
            0,
            1000000,
            NativeValue::default(),
            0.0,
            1.0,
            NativeEasing::default(),
        );
        *out = NativeValue {
            tag: NATIVE_VALUE_TIMELINE,
            ..NativeValue::default()
        };
    }
    NATIVE_STATUS_OK
}

#[test]
fn native_plan_writer_is_inert_and_failed_writes_are_atomic() {
    use crate::timeline::{ActorRef, PlanOp};
    let properties = HashMap::from([(
        1000000,
        ("glow".into(), animatix_syntax::schema::PropertyValueKind::F32),
    )]);
    let args = [Value::ActorRef(ActorRef::caller("p"))];
    let value = super::super::callable::invoke_native_callable(
        &contract(CallableType::Timeline),
        write_plan,
        &args,
        &Environment::new(),
        &properties,
        &HashMap::new(),
    )
    .unwrap();
    let Value::TimelinePlan(plan) = value else {
        panic!("expected inert plan")
    };
    assert!(
        matches!(&plan.ops()[0], PlanOp::Property { actor, value: Value::Num(7.0), .. } if actor.label() == "p")
    );
    let failed = super::super::callable::invoke_native_callable(
        &contract(CallableType::Timeline),
        ignore_invalid_write,
        &args,
        &Environment::new(),
        &properties,
        &HashMap::new(),
    );
    assert!(
        failed.is_err(),
        "a plugin cannot ignore a failed append and return a partial plan"
    );
}

#[test]
fn parameterized_easing_payloads_round_trip() {
    use super::super::callable::{decode_native_easing, encode_native_easing};
    use crate::easing::Easing;
    for curve in [
        Easing::Spring {
            damping: 6.0,
            frequency: 9.0,
        },
        Easing::SpringV0 {
            damping: 6.0,
            frequency: 9.0,
            v0: 2.0,
        },
        Easing::CubicBezier([0.1, 0.2, 0.3, 0.4]),
    ] {
        assert_eq!(decode_native_easing(encode_native_easing(curve)), Some(curve));
    }
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

static PREVIOUS_ACTOR_HANDLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[test]
fn native_timeline_results_follow_the_result_contract_and_execution_phase() {
    let properties = HashMap::from([(
        1000000,
        ("glow".into(), animatix_syntax::schema::PropertyValueKind::F32),
    )]);
    let args = [Value::ActorRef(crate::timeline::ActorRef::caller("p"))];
    for result in [
        CallableType::Any,
        CallableType::Union(vec![CallableType::Timeline, CallableType::Unit]),
    ] {
        let descriptor = contract(result);
        let mut env = Environment::new();
        assert!(matches!(
            super::super::callable::invoke_native_callable(
                &descriptor,
                write_plan,
                &args,
                &env,
                &properties,
                &HashMap::new()
            )
            .unwrap(),
            Value::TimelinePlan(_)
        ));
        env.set_frame_phase();
        assert!(
            super::super::callable::invoke_native_callable(
                &descriptor,
                write_plan,
                &args,
                &env,
                &properties,
                &HashMap::new()
            )
            .is_err()
        );
    }
}

unsafe extern "C" fn return_previous_actor(
    _ctx: *mut NativeCallableContext,
    args: *const NativeValue,
    len: usize,
    out: *mut NativeValue,
) -> i32 {
    if len != 1 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let actor = unsafe { *args };
    let previous = PREVIOUS_ACTOR_HANDLE.swap(actor.handle, std::sync::atomic::Ordering::SeqCst);
    unsafe {
        *out = NativeValue {
            handle: if previous == 0 {
                actor.handle
            } else {
                previous
            },
            ..actor
        };
    }
    NATIVE_STATUS_OK
}

#[test]
fn actor_handles_cannot_escape_their_invocation() {
    use crate::timeline::ActorRef;
    PREVIOUS_ACTOR_HANDLE.store(0, std::sync::atomic::Ordering::SeqCst);
    let args = [Value::ActorRef(ActorRef::caller("p"))];
    let descriptor = contract(CallableType::Actor);
    let env = Environment::new();
    assert_eq!(
        super::super::callable::invoke_native_callable(
            &descriptor,
            return_previous_actor,
            &args,
            &env,
            &HashMap::new(),
            &HashMap::new()
        )
        .unwrap(),
        args[0]
    );
    assert!(
        super::super::callable::invoke_native_callable(
            &descriptor,
            return_previous_actor,
            &args,
            &env,
            &HashMap::new(),
            &HashMap::new()
        )
        .is_err()
    );
}
