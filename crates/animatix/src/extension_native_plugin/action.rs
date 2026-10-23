//! Host-side adapter that turns a native action callback into a runtime action.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use animatix_plugin_api::{
    NATIVE_STATUS_OK, NATIVE_STATUS_UNSUPPORTED, NativeActionContext, NativeActionExecuteFn,
    NativeModifierValue,
};

use super::{
    NativeActionHost, NativeValueArena, native_action_get_arg, native_action_get_arg_count,
    native_action_get_modifier, native_action_get_modifier_count, native_action_get_service,
    native_action_get_target, native_action_get_target_count, native_action_write_keyframe,
    value_to_native,
};
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};

/// Host-side adapter that turns a native action callback into a runtime action.
pub(crate) struct NativeActionAdapter {
    pub(super) signature: ActionSignature,
    pub(super) callback: NativeActionExecuteFn,
    pub(super) property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    pub(super) property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    pub(super) service_values: HashMap<String, usize>,
    pub(super) _library: Arc<dyn Any + Send + Sync>,
}

impl BuiltinAction for NativeActionAdapter {
    fn signature(&self) -> ActionSignature {
        self.signature.clone()
    }

    fn execute(
        &self,
        action: &crate::ast::Action,
        time_ms: f64,
        timeline: &mut crate::timeline::Timeline,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    ) {
        let mut arena = NativeValueArena::default();
        let args = action
            .args
            .iter()
            .map(|arg| {
                crate::timeline::utils::evaluate_expr(arg, &timeline.env)
                    .ok()
                    .and_then(|value| value_to_native(&value, &mut arena).ok())
                    .unwrap_or_default()
            })
            .collect();
        let modifiers = action
            .modifiers
            .iter()
            .map(|modifier| {
                let name_ptr = modifier
                    .name
                    .as_deref()
                    .map(|name| arena.string(name).0)
                    .unwrap_or(std::ptr::null());
                let value = crate::timeline::utils::evaluate_expr(&modifier.value, &timeline.env)
                    .ok()
                    .and_then(|value| value_to_native(&value, &mut arena).ok())
                    .unwrap_or_default();
                NativeModifierValue {
                    name: name_ptr,
                    value,
                }
            })
            .collect();
        let targets = action.targets.iter().map(|target| arena.string(target).0).collect();
        let mut host = NativeActionHost {
            timeline,
            targets,
            args,
            modifiers,
            property_ids: &self.property_ids,
            property_kinds: &self.property_kinds,
            service_values: &self.service_values,
        };
        let mut native_ctx = NativeActionContext {
            size: std::mem::size_of::<NativeActionContext>(),
            host: (&mut host as *mut NativeActionHost).cast(),
            time_ms,
            get_target_count: native_action_get_target_count,
            get_target: native_action_get_target,
            get_arg_count: native_action_get_arg_count,
            get_arg: native_action_get_arg,
            get_modifier_count: native_action_get_modifier_count,
            get_modifier: native_action_get_modifier,
            write_keyframe: native_action_write_keyframe,
            get_service: native_action_get_service,
        };
        let status = unsafe { (self.callback)(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            let detail = if status == NATIVE_STATUS_UNSUPPORTED {
                "unsupported by the current native plugin API (e.g. a built-in property, \
                 which native actions cannot keyframe yet)"
            } else {
                "failed"
            };
            diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::UnknownAction,
                crate::diagnostics::DiagnosticPhase::Build,
                format!("native action '{}' {detail} with status {status}", self.signature.name),
            ));
        }
    }
}
