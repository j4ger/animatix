use super::*;

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
    let mut timeline = crate::timeline::Timeline::new();
    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
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
    let id = timeline.world.spawn_with_track("pulse", track);
    timeline.world.ensure_image_payload(id).image.ensure(None).add_keyframe(
        0,
        Some(image),
        crate::easing::Easing::Linear,
    );
    let track = timeline.world.track(id).expect("spawned pulse track");
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx =
        sample_evaluate_ctx_with_payload(track, &asset_cache, timeline.world.payload(id));
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
    let mut timeline = crate::timeline::Timeline::new();
    let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
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
    let id = timeline.world.spawn_with_track("pulse", track);
    timeline.world.ensure_image_payload(id).image.ensure(None).add_keyframe(
        0,
        Some(image),
        crate::easing::Easing::Linear,
    );
    let track = timeline.world.track(id).expect("spawned pulse track");
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let eval_ctx =
        sample_evaluate_ctx_with_payload(track, &asset_cache, timeline.world.payload(id));
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
        &mut track.property_plan,
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
