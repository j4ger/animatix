use super::*;

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Any => write!(f, "Any"),
            Type::Num => write!(f, "Num"),
            Type::Duration => write!(f, "Duration"),
            Type::Timeline => write!(f, "Timeline"),
            Type::Unit => write!(f, "Unit"),
            Type::Str => write!(f, "Str"),
            Type::Bool => write!(f, "Bool"),
            Type::Vec2 => write!(f, "Vec2"),
            Type::Vec3 => write!(f, "Vec3"),
            Type::Vec4 => write!(f, "Vec4"),
            Type::Color => write!(f, "Color"),
            Type::Easing => write!(f, "Easing"),
            Type::Actor(name) => write!(f, "Actor({name})"),
            Type::Component(name) => write!(f, "Component({name})"),
            Type::Scene => write!(f, "Scene"),
            Type::List(inner) => write!(f, "List<{inner}>"),
            Type::Union(items) => {
                let inner = items.iter().map(ToString::to_string).collect::<Vec<_>>().join(" | ");
                write!(f, "{inner}")
            },
            Type::Enum(variants) => write!(f, "Enum({})", variants.join(", ")),
            Type::Tuple(items) => {
                let inner = items.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                write!(f, "({inner})")
            },
            Type::Function { params, ret } => {
                let params = params.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                write!(f, "({params}) -> {ret}")
            },
        }
    }
}

impl Type {
    /// Convert an internal type back to the user-facing annotation shape.
    pub fn to_annotation(&self) -> TypeAnnotation {
        match self {
            Type::Any => TypeAnnotation::Any,
            Type::Num => TypeAnnotation::Num,
            Type::Duration => TypeAnnotation::Duration,
            Type::Timeline => TypeAnnotation::Timeline,
            Type::Unit => TypeAnnotation::Unit,
            Type::Str => TypeAnnotation::Str,
            Type::Bool => TypeAnnotation::Bool,
            Type::Vec2 => TypeAnnotation::Vec2,
            Type::Vec3 => TypeAnnotation::Vec3,
            Type::Vec4 => TypeAnnotation::Vec4,
            Type::Color => TypeAnnotation::Color,
            Type::Easing => TypeAnnotation::Easing,
            Type::Actor(_) | Type::Component(_) => TypeAnnotation::Actor,
            Type::Scene => TypeAnnotation::Scene,
            Type::List(inner) => TypeAnnotation::List(Box::new(inner.to_annotation())),
            Type::Tuple(items) => {
                TypeAnnotation::Tuple(items.iter().map(Type::to_annotation).collect())
            },
            Type::Union(types) => {
                TypeAnnotation::Union(types.iter().map(Type::to_annotation).collect())
            },
            Type::Enum(variants) => TypeAnnotation::Enum(variants.clone()),
            Type::Function { params, ret } => TypeAnnotation::Function {
                params: params.iter().map(Type::to_annotation).collect(),
                ret: Box::new(ret.to_annotation()),
            },
        }
    }

    /// Convert a user-facing annotation into the richer internal type.
    ///
    /// Named aliases are intentionally left as `Any` here; callers that need
    /// alias resolution must use [`TypeEnv::resolve_annotation`].
    pub fn from_annotation(annotation: &TypeAnnotation) -> Self {
        match annotation {
            TypeAnnotation::Num => Type::Num,
            TypeAnnotation::Duration => Type::Duration,
            TypeAnnotation::Timeline => Type::Timeline,
            TypeAnnotation::Unit => Type::Unit,
            TypeAnnotation::Str => Type::Str,
            TypeAnnotation::Bool => Type::Bool,
            TypeAnnotation::Vec2 => Type::Vec2,
            TypeAnnotation::Vec3 => Type::Vec3,
            TypeAnnotation::Vec4 => Type::Vec4,
            TypeAnnotation::Color => Type::Color,
            TypeAnnotation::Easing => Type::Easing,
            TypeAnnotation::Actor => Type::Actor("Actor".to_string()),
            TypeAnnotation::Scene => Type::Scene,
            TypeAnnotation::List(inner) => Type::List(Box::new(Type::from_annotation(inner))),
            TypeAnnotation::Tuple(items) => {
                Type::Tuple(items.iter().map(Type::from_annotation).collect())
            },
            TypeAnnotation::Function { params, ret } => Type::Function {
                params: params.iter().map(Type::from_annotation).collect(),
                ret: Box::new(Type::from_annotation(ret)),
            },
            TypeAnnotation::Union(types) => {
                Type::Union(types.iter().map(Type::from_annotation).collect())
            },
            TypeAnnotation::Enum(variants) => Type::Enum(variants.clone()),
            TypeAnnotation::Alias(_) => Type::Any,
            TypeAnnotation::Any => Type::Any,
        }
    }

    /// Parse a source/manifest type annotation string into a [`Type`].
    ///
    /// This is the single source of truth for the annotation grammar shared by
    /// native plugin descriptors and analyzer manifests. Unknown or malformed
    /// strings return `None`; enum annotations
    /// (`Enum(left, right, top)` or `Enum<...>`) are parsed into [`Type::Enum`].
    pub fn parse(text: &str) -> Option<Type> {
        let annotation = crate::parser::parse_type_annotation(text)?;
        type_from_external_annotation(&annotation)
    }

    /// Convert the shared callable contract type to syntax's inference type.
    pub fn from_callable_type(ty: &animatix_core::callable::CallableType) -> Self {
        use animatix_core::callable::CallableType as C;
        match ty {
            C::Any => Type::Any,
            C::Num => Type::Num,
            C::Bool => Type::Bool,
            C::Str => Type::Str,
            C::Vec2 => Type::Vec2,
            C::Vec3 => Type::Vec3,
            C::Vec4 => Type::Vec4,
            C::Color => Type::Color,
            C::Duration => Type::Duration,
            C::Timeline => Type::Timeline,
            C::Unit => Type::Unit,
            C::Actor => Type::Actor("Actor".to_string()),
            C::Scene => Type::Scene,
            C::Easing => Type::Easing,
            C::List(inner) => Type::List(Box::new(Type::from_callable_type(inner))),
            C::Tuple(items) => Type::Tuple(items.iter().map(Type::from_callable_type).collect()),
            C::Union(items) => Type::Union(items.iter().map(Type::from_callable_type).collect()),
            C::Enum(items) => Type::Enum(items.clone()),
            C::Function { params, ret } => Type::Function {
                params: params.iter().map(|(_, param)| Type::from_callable_type(param)).collect(),
                ret: Box::new(Type::from_callable_type(ret)),
            },
        }
    }

    /// Convert syntax's inference type to a shared callable contract type.
    /// Function parameter names are absent from `Type`; positional placeholder
    /// names keep this conversion total. Named-call validation uses the original
    /// callable descriptor or `FnDecl` signature instead.
    pub fn to_callable_type(&self) -> animatix_core::callable::CallableType {
        use animatix_core::callable::CallableType as C;
        match self {
            Type::Any => C::Any,
            Type::Num => C::Num,
            Type::Bool => C::Bool,
            Type::Str => C::Str,
            Type::Vec2 => C::Vec2,
            Type::Vec3 => C::Vec3,
            Type::Vec4 => C::Vec4,
            Type::Color => C::Color,
            Type::Duration => C::Duration,
            Type::Timeline => C::Timeline,
            Type::Unit => C::Unit,
            Type::Easing => C::Easing,
            Type::Actor(_) | Type::Component(_) => C::Actor,
            Type::Scene => C::Scene,
            Type::List(inner) => C::List(Box::new(inner.to_callable_type())),
            Type::Tuple(items) => C::Tuple(items.iter().map(Type::to_callable_type).collect()),
            Type::Union(items) => C::Union(items.iter().map(Type::to_callable_type).collect()),
            Type::Enum(items) => C::Enum(items.clone()),
            Type::Function { params, ret } => C::Function {
                params: params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| (format!("arg_{index}"), param.to_callable_type()))
                    .collect(),
                ret: Box::new(ret.to_callable_type()),
            },
        }
    }
}

/// Convert parsed annotations used by external descriptors, which have no
/// module alias table. The two ABI spellings remain explicit external names;
/// other unknown names stay errors rather than silently becoming `Any`.
fn type_from_external_annotation(annotation: &TypeAnnotation) -> Option<Type> {
    match annotation {
        TypeAnnotation::Alias(name) => match name.as_str() {
            "U32" => Some(Type::Num),
            "String" => Some(Type::Str),
            _ => None,
        },
        TypeAnnotation::List(inner) => {
            Some(Type::List(Box::new(type_from_external_annotation(inner)?)))
        },
        TypeAnnotation::Tuple(items) => items
            .iter()
            .map(type_from_external_annotation)
            .collect::<Option<Vec<_>>>()
            .map(Type::Tuple),
        TypeAnnotation::Union(items) => items
            .iter()
            .map(type_from_external_annotation)
            .collect::<Option<Vec<_>>>()
            .map(Type::Union),
        TypeAnnotation::Function { params, ret } => Some(Type::Function {
            params: params.iter().map(type_from_external_annotation).collect::<Option<Vec<_>>>()?,
            ret: Box::new(type_from_external_annotation(ret)?),
        }),
        TypeAnnotation::Enum(variants) if variants.is_empty() => None,
        other => Some(Type::from_annotation(other)),
    }
}
