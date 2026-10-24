//! Host-side adapter that turns a native primitive descriptor into a runtime
//! [`Primitive`].

use std::any::Any;
use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::Arc;

use animatix_plugin_api::{
    NATIVE_STATUS_OK, NATIVE_STATUS_UNSUPPORTED, NativeAssignmentContext, NativeAssignmentFn,
    NativeClipPathCtx, NativeClipPathFn, NativeDefaultColorKeyFn, NativeDefaultPropsCtx,
    NativeDefaultPropsFn, NativeFinalizeContext, NativeFinalizeFn, NativePrimitive,
    NativePrimitiveBuildCtx, NativePrimitiveBuildFn, NativePrimitiveEquationFragmentCtx,
    NativePrimitiveEquationFragmentFn, NativePrimitiveEvaluateCtx, NativePrimitiveEvaluateFn,
};

use super::{
    NativeAssignmentHost, NativeDefaultPropsHost, NativeEquationFragmentHost, NativeFinalizeHost,
    NativePrimitiveBuildHost, NativePrimitiveEvaluateHost, NativeValueArena, easing_code,
    native_append_highlight, native_append_image, native_append_path, native_append_text,
    native_assignment_get_value, native_assignment_write_keyframe, native_blend_from_code,
    native_build_get_child, native_build_get_child_count, native_build_get_modifier,
    native_build_get_modifier_count, native_build_get_property, native_build_get_property_count,
    native_build_report_diagnostic, native_capabilities, native_child_processing,
    native_default_props_append, native_get_property, native_get_service,
    native_primitive_category, native_resize_mode, native_write_content, read_c_string,
    value_to_native,
};
use crate::ast::{Expr, Property};
use crate::primitives::{
    AssignmentCtx, BuildCtx, ChildProcessing, EquationFragment, EvaluateCtx, Primitive,
};
use crate::timeline::{Environment, ResizeMode};

/// Host-side adapter that turns a native primitive descriptor into a runtime
/// [`Primitive`].
pub(crate) struct NativePrimitiveAdapter {
    pub(super) type_name: String,
    pub(super) info: animatix_std::PrimitiveInfo,
    child_processing: ChildProcessing,
    declared_properties: Vec<String>,
    resize_mode: ResizeMode,
    property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    service_values: HashMap<String, usize>,
    build: Option<NativePrimitiveBuildFn>,
    evaluate: Option<NativePrimitiveEvaluateFn>,
    handle_assignment: Option<NativeAssignmentFn>,
    finalize_container_build: Option<NativeFinalizeFn>,
    default_props: Option<NativeDefaultPropsFn>,
    default_color_key: Option<NativeDefaultColorKeyFn>,
    clip_path: Option<NativeClipPathFn>,
    equation_fragment: Option<NativePrimitiveEquationFragmentFn>,
    _library: Arc<dyn Any + Send + Sync>,
}

impl NativePrimitiveAdapter {
    pub(super) fn new(
        primitive: NativePrimitive,
        library: Arc<dyn Any + Send + Sync>,
        property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
        property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
        service_values: HashMap<String, usize>,
    ) -> Option<Self> {
        let type_name = unsafe { read_c_string(primitive.type_name)? };
        let display_name =
            unsafe { read_c_string(primitive.display_name) }.unwrap_or_else(|| type_name.clone());
        let icon_id =
            unsafe { read_c_string(primitive.icon_id) }.unwrap_or_else(|| "extension".to_string());
        let category = native_primitive_category(primitive.category)?;
        let declared_properties = if primitive.properties.is_null() || primitive.property_len == 0 {
            Vec::new()
        } else {
            let names =
                unsafe { std::slice::from_raw_parts(primitive.properties, primitive.property_len) };
            names.iter().filter_map(|ptr| unsafe { read_c_string(*ptr) }).collect()
        };
        let child_processing =
            native_child_processing(primitive.child_processing).unwrap_or_default();
        let capabilities = native_capabilities(primitive.capabilities);
        let info = animatix_std::PrimitiveInfo {
            type_name: type_name.clone().into(),
            display_name: display_name.into(),
            category,
            icon_id: icon_id.into(),
            advanced: primitive.advanced,
            capabilities,
            child_processing,
            shape: None,
            text: None,
            stroke_path: false,
            connector: false,
            plot: None,
            doc: "",
            default_stub: None,
        };
        Some(Self {
            type_name,
            info,
            child_processing,
            declared_properties,
            resize_mode: native_resize_mode(primitive.resize_mode),
            property_ids,
            property_kinds,
            service_values,
            build: primitive.build,
            evaluate: primitive.evaluate,
            handle_assignment: primitive.handle_assignment,
            finalize_container_build: primitive.finalize_container_build,
            default_props: primitive.default_props,
            default_color_key: primitive.default_color_key,
            clip_path: primitive.clip_path,
            equation_fragment: primitive.equation_fragment,
            _library: library,
        })
    }
}

impl Primitive for NativePrimitiveAdapter {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn declared_property_names(&self) -> Vec<&str> {
        self.declared_properties.iter().map(String::as_str).collect()
    }

    fn declares_property(&self, name: &str) -> bool {
        self.declared_properties.iter().any(|declared| declared == name)
    }

    fn resize_mode(&self, _caps: &crate::timeline::ActorCaps) -> ResizeMode {
        self.resize_mode
    }

    fn default_props(&self, scene_dimensions: &crate::timeline::SceneDimensions) -> Vec<Property> {
        let Some(callback) = self.default_props else {
            return Vec::new();
        };
        let mut host = NativeDefaultPropsHost { props: Vec::new() };
        let mut native_ctx = NativeDefaultPropsCtx {
            size: std::mem::size_of::<NativeDefaultPropsCtx>(),
            scene_width: scene_dimensions.width as f64,
            scene_height: scene_dimensions.height as f64,
            host: (&mut host as *mut NativeDefaultPropsHost).cast(),
            append_property: Some(native_default_props_append),
        };
        let status = unsafe { callback(&mut native_ctx) };
        let mut props = Vec::new();
        for (name, value) in host.props {
            match crate::timeline::value_parser::value_to_expr(&value) {
                Some(expr) => props.push(Property::new(name, expr)),
                None => {
                    tracing::warn!(
                        "native primitive '{}' default property '{name}' has a value that \
                         cannot be expressed as a source default; skipping",
                        self.type_name
                    );
                },
            }
        }
        if status != NATIVE_STATUS_OK {
            tracing::warn!(
                "native primitive '{}' default-props callback failed with status {status}",
                self.type_name
            );
        }
        props
    }

    fn default_color_key(
        &self,
        property: &str,
        _caps: &crate::timeline::ActorCaps,
    ) -> Option<&'static str> {
        let callback = self.default_color_key?;
        let property_c = std::ffi::CString::new(property).ok()?;
        let mut out: *const c_char = std::ptr::null();
        let status = unsafe { callback(property_c.as_ptr(), &mut out) };
        if status != NATIVE_STATUS_OK || out.is_null() {
            return None;
        }
        unsafe {
            let text = std::ffi::CStr::from_ptr(out).to_str().ok()?;
            Some(std::mem::transmute::<&str, &'static str>(text))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[crate::ast::Property],
        modifiers: &[crate::ast::Modifier],
        children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let BuildCtx {
            timeline,
            time_ms,
            parent_label,
            diagnostics,
            coordinate_context: _,
        } = ctx;
        let time_ms = *time_ms;
        let parent_label = *parent_label;
        let track_id = {
            let track = timeline.world.tracks.entry(label.to_string()).or_insert_with(|| {
                crate::timeline::AnimationTrack::new(label.to_string(), self.type_name.clone())
            });
            let type_name = self.type_name.clone();
            track.actor_type = type_name;
            track.set_caps(animatix_std::caps_from_info(&self.info));
            track.id
        };
        timeline.world.ensure_len();
        timeline.world.rebuild_property_plan(track_id);
        let Some(build) = self.build else {
            return Ok(());
        };
        let status = {
            let mut host = NativePrimitiveBuildHost::new(
                timeline,
                time_ms,
                parent_label,
                label,
                props,
                modifiers,
                children,
                diagnostics,
            );
            let parent_label_ptr = match parent_label {
                Some(parent_label) => host.arena.string(parent_label).0,
                None => std::ptr::null(),
            };
            let mut native_ctx = NativePrimitiveBuildCtx {
                size: std::mem::size_of::<NativePrimitiveBuildCtx>(),
                time_ms,
                host: (&mut host as *mut NativePrimitiveBuildHost).cast(),
                parent_label: parent_label_ptr,
                get_property_count: native_build_get_property_count,
                get_property: native_build_get_property,
                get_modifier_count: native_build_get_modifier_count,
                get_modifier: native_build_get_modifier,
                get_child_count: native_build_get_child_count,
                get_child: native_build_get_child,
                report_diagnostic: native_build_report_diagnostic,
            };
            unsafe { build(&mut native_ctx) }
        };
        if status != NATIVE_STATUS_OK {
            diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                crate::diagnostics::DiagnosticPhase::Build,
                format!("native primitive '{}' build failed with status {status}", self.type_name),
            ));
        }
        Ok(())
    }

    fn handle_assignment(
        &self,
        _track: &mut crate::timeline::AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
        subject: &str,
    ) -> bool {
        let Some(callback) = self.handle_assignment else {
            return false;
        };
        let Some(value) = crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
            value,
            env,
            diagnostics,
            subject,
        ) else {
            return false;
        };
        let mut arena = NativeValueArena::default();
        let Ok(native_value) = value_to_native(&value, &mut arena) else {
            return false;
        };
        let property_ptr = arena.string(property).0;
        let t_start_ms = ctx.t_start_ms;
        let t_end_ms = ctx.t_end_ms;
        let easing = easing_code(ctx.easing);
        let Some(plan) = ctx.property_plan.as_deref_mut() else {
            return false;
        };
        let status = {
            let mut host = NativeAssignmentHost {
                plan,
                property_ids: &self.property_ids,
                property_kinds: &self.property_kinds,
                value: native_value,
            };
            let mut native_ctx = NativeAssignmentContext {
                size: std::mem::size_of::<NativeAssignmentContext>(),
                host: (&mut host as *mut NativeAssignmentHost).cast(),
                property: property_ptr,
                t_start_ms,
                t_end_ms,
                easing,
                get_value: native_assignment_get_value,
                write_keyframe: native_assignment_write_keyframe,
            };
            unsafe { callback(&mut native_ctx) }
        };
        match status {
            NATIVE_STATUS_OK => true,
            NATIVE_STATUS_UNSUPPORTED => false,
            _ => {
                diagnostics.push(crate::diagnostics::Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::InvalidAssignmentTarget,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!(
                        "native primitive '{}' failed assignment {property} with status {status}",
                        self.type_name
                    ),
                ));
                true
            },
        }
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[Property],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let Some(callback) = self.finalize_container_build else {
            return Ok(());
        };
        let child_count = ctx
            .timeline
            .world
            .get_id(label)
            .map(|id| ctx.timeline.world.hierarchy(id).children.len())
            .unwrap_or(0);
        let label_c = std::ffi::CString::new(label).expect("actor label has no nul");
        let mut host = NativeFinalizeHost;
        let mut native_ctx = NativeFinalizeContext {
            size: std::mem::size_of::<NativeFinalizeContext>(),
            host: (&mut host as *mut NativeFinalizeHost).cast(),
            label: label_c.as_ptr(),
            child_count,
        };
        let status = unsafe { callback(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            ctx.diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                crate::diagnostics::DiagnosticPhase::Build,
                format!(
                    "native primitive '{}' finalize failed with status {status}",
                    self.type_name
                ),
            ));
        }
        Ok(())
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        let Some(evaluate) = self.evaluate else {
            return Ok(None);
        };
        let (text_compiler, font_context) = match text_ctx {
            Some(text_ctx) => (Some(&mut *text_ctx.text_compiler), Some(text_ctx.font_context)),
            None => (None, None),
        };
        let mut host = NativePrimitiveEvaluateHost {
            ctx,
            property_ids: &self.property_ids,
            service_values: &self.service_values,
            text_compiler,
            font_context,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut native_ctx = NativePrimitiveEvaluateCtx {
            size: std::mem::size_of::<NativePrimitiveEvaluateCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativePrimitiveEvaluateHost).cast(),
            get_property: Some(native_get_property),
            get_service: Some(native_get_service),
            append_path: Some(native_append_path),
            append_text: Some(native_append_text),
            append_image: Some(native_append_image),
            append_highlight: Some(native_append_highlight),
        };
        let status = unsafe { evaluate(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            return Err(crate::renderer::error::RenderError::NativePrimitive(format!(
                "native primitive '{}' failed with status {status}",
                self.type_name
            )));
        }
        Ok(Some(host.commands))
    }

    fn clip_path(
        &self,
        ctx: &EvaluateCtx,
        _caps: &crate::timeline::ActorCaps,
    ) -> Option<kurbo::BezPath> {
        let clip_path = self.clip_path?;
        let mut host = NativePrimitiveEvaluateHost {
            ctx,
            property_ids: &self.property_ids,
            service_values: &self.service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut native_ctx = NativeClipPathCtx {
            size: std::mem::size_of::<NativeClipPathCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativePrimitiveEvaluateHost).cast(),
            append_path: Some(native_append_path),
        };
        let status = unsafe { clip_path(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            return None;
        }
        crate::primitives::clip_bezpath_from_commands(&host.commands)
    }

    fn equation_fragment(&self, ctx: &EvaluateCtx) -> Option<EquationFragment> {
        let equation_fragment = self.equation_fragment?;
        let mut host = NativeEquationFragmentHost::default();
        let mut native_ctx = NativePrimitiveEquationFragmentCtx {
            size: std::mem::size_of::<NativePrimitiveEquationFragmentCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativeEquationFragmentHost).cast(),
            write_content: Some(native_write_content),
            highlight_color: [0.0; 4],
            highlight_opacity: 0.0,
            highlight_padding: 4.0,
            highlight_radius: 3.0,
            highlight_blend: 0,
        };
        let status = unsafe { equation_fragment(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            return None;
        }
        let color = native_ctx.highlight_color;
        Some(EquationFragment {
            content: host.content,
            highlight_color: [
                color[0].clamp(0.0, 1.0) as f32,
                color[1].clamp(0.0, 1.0) as f32,
                color[2].clamp(0.0, 1.0) as f32,
                color[3].clamp(0.0, 1.0) as f32,
            ],
            highlight_opacity: native_ctx.highlight_opacity.clamp(0.0, 1.0) as f32,
            highlight_padding: native_ctx.highlight_padding.max(0.0) as f32,
            highlight_radius: native_ctx.highlight_radius.max(0.0) as f32,
            highlight_blend: native_blend_from_code(native_ctx.highlight_blend),
        })
    }

    fn render_children(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        children: &[&str],
    ) -> Result<(), crate::renderer::error::RenderError> {
        let timeline = ctx.timeline;
        match self.child_processing {
            ChildProcessing::Filter => timeline.render_filter_children_ctx(ctx),
            ChildProcessing::Glass => timeline.render_glass_children_ctx(ctx),
            ChildProcessing::Mask => timeline.render_mask_children_ctx(ctx),
            ChildProcessing::Equation => timeline.render_equation_children_ctx(ctx),
            ChildProcessing::Captions => timeline.render_captions_children_ctx(ctx),
            ChildProcessing::Generic => ctx.render_children_default(children),
        }
        Ok(())
    }

    fn render_children_by_id(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), crate::renderer::error::RenderError> {
        let timeline = ctx.timeline;
        match self.child_processing {
            ChildProcessing::Filter => timeline.render_filter_children_ctx(ctx),
            ChildProcessing::Glass => timeline.render_glass_children_ctx(ctx),
            ChildProcessing::Mask => timeline.render_mask_children_ctx(ctx),
            ChildProcessing::Equation => timeline.render_equation_children_ctx(ctx),
            ChildProcessing::Captions => timeline.render_captions_children_ctx(ctx),
            ChildProcessing::Generic => ctx.render_children_ids(children_ids),
        }
        Ok(())
    }
}
