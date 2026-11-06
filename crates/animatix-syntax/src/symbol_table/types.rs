use super::*;

/// Map an effect parameter's declared value kind to its analysis type.
pub(crate) fn effect_param_type(kind: crate::schema::PropertyValueKind) -> Option<typing::Type> {
    use crate::schema::PropertyValueKind as K;
    match kind {
        K::F32 | K::U32 => Some(typing::Type::Num),
        K::Bool => Some(typing::Type::Bool),
        K::Vec2 => Some(typing::Type::Vec2),
        K::Vec4 => Some(typing::Type::Vec4),
        _ => None,
    }
}

/// Build a type-env component signature from parameter declarations.
///
/// Shared by [`SymbolTable::type_env`] (annotations only, `infer_default`
/// returns `None`) and the type checker (annotation first, inferred default
/// second), so the two layers cannot disagree about a component parameter's
/// type.
pub(crate) fn component_signature<'a>(
    params: impl Iterator<Item = (&'a str, Option<&'a TypeAnnotation>, Option<&'a Expr>)>,
    infer_default: &mut dyn FnMut(&Expr) -> Option<typing::Type>,
) -> typing::ComponentSignature {
    let mut signature = typing::ComponentSignature::default();
    for (name, annotation, default) in params {
        if let Some(annotation) = annotation {
            signature
                .params
                .insert(name.to_string(), typing::Type::from_annotation(annotation));
        } else if let Some(default) = default
            && let Some(ty) = infer_default(default)
        {
            signature.params.insert(name.to_string(), ty);
        }
    }
    signature
}
