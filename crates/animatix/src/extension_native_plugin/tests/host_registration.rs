//! Native host registration and rollback tests.

use std::ffi::c_void;

use animatix_plugin_api::NATIVE_RESIZE_MODE_SIZE;

use super::*;
use crate::extension_context::ExtensionContext;
use crate::timeline::Environment;

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
