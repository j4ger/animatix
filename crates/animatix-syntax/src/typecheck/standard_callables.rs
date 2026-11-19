use std::collections::HashMap;
use std::sync::LazyLock;

use animatix_core::callable::ArgumentParameter;

/// A parameter type prepared for syntax-level call checks.
pub(crate) struct CallableParamType<'a> {
    pub(super) name: &'a str,
    pub(super) ty: crate::typing::Type,
    pub(super) optional: bool,
    pub(super) variadic: bool,
}

/// Standard callable signature converted from the canonical descriptor table
/// once, rather than once for every call on each analyzer update.
pub(crate) struct StandardCallableSignature {
    pub(super) params: Vec<CallableParamType<'static>>,
    pub(super) binding_params: Vec<ArgumentParameter<'static>>,
}

static STANDARD_CALLABLE_SIGNATURES: LazyLock<HashMap<&'static str, StandardCallableSignature>> =
    LazyLock::new(|| {
        animatix_core::builtins::callable::callables()
            .iter()
            .map(|descriptor| {
                let params = descriptor
                    .params
                    .iter()
                    .map(|param| CallableParamType {
                        name: param.name.as_str(),
                        ty: crate::typing::Type::from_callable_type(&param.ty),
                        optional: param.optional || param.default.is_some(),
                        variadic: param.variadic,
                    })
                    .collect::<Vec<_>>();
                let binding_params = params
                    .iter()
                    .map(|param| ArgumentParameter {
                        name: param.name,
                        variadic: param.variadic,
                    })
                    .collect();
                (
                    descriptor.name.as_str(),
                    StandardCallableSignature {
                        params,
                        binding_params,
                    },
                )
            })
            .collect()
    });

pub(crate) fn standard_callable_signature(
    name: &str,
) -> Option<&'static StandardCallableSignature> {
    let name = name.strip_prefix("std::").unwrap_or(name);
    STANDARD_CALLABLE_SIGNATURES.get(name)
}
