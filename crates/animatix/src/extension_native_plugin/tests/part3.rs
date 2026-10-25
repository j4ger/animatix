use super::*;

#[test]
fn native_assignment_write_keyframe_writes_extension_slot() {
    let mut ctx = ExtensionContext::new();
    let id = ctx
        .register_property("Pulse", "glow", animatix_syntax::schema::PropertyValueKind::F32, true)
        .expect("register property");
    let property_ids = HashMap::from([("glow".to_string(), id)]);
    let property_kinds =
        HashMap::from([("glow".to_string(), animatix_syntax::schema::PropertyValueKind::F32)]);
    let caps = animatix_std::caps_for_type("Rect").unwrap_or_default();
    let mut plan = crate::timeline::plan::PropertyPlan::for_actor(&caps, "Rect");
    let mut host = NativeAssignmentHost {
        plan: &mut plan,
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
        crate::timeline::property_engine::read_property_plan_slot(&plan, id, 500),
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
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let ctx = sample_evaluate_ctx(&asset_cache);
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

    let asset_cache = crate::timeline::assets::AssetCache::new();
    let ctx = sample_evaluate_ctx(&asset_cache);

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
    let id = report.output.world.get_id("p").expect("pulse track");
    let geometry = report.output.world.geometry(id);
    assert_eq!(
        geometry.position.get(0, [0.0, 0.0]),
        [10.0, 20.0],
        "declared position should be written by the generic property engine"
    );
    assert_eq!(
        report.output.world.style(id).color.get(0, [1.0, 1.0, 1.0, 1.0]),
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
    let id = report.output.world.get_id("p").expect("pulse track");
    assert_eq!(
        report.output.world.meta(id).first_seen_ms,
        0,
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
    let _id = ctx
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
    let id = report.output.world.get_id("p").expect("pulse track");
    let plan = report.output.world.property_plan(id);
    let slot = plan.get(PropertyId(1_000_000));
    let t0 = slot.and_then(|s| s.track.sample(0));
    let t1500 = slot.and_then(|s| s.track.sample(1500));
    assert_eq!(t0, Some(crate::timeline::PropertyValue::Enum("ring".to_string())));
    assert_eq!(t1500, Some(crate::timeline::PropertyValue::Enum("dot".to_string())));

    // Read back through the native path -> must be NATIVE_VALUE_ENUM.
    let property_ids = HashMap::from([("mode".to_string(), PropertyId(1_000_000))]);
    let service_values = HashMap::new();
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx = sample_evaluate_ctx_with_plan(&asset_cache, Some(plan));
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
