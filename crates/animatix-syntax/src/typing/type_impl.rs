use super::*;

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Any => write!(f, "Any"),
            Type::Num => write!(f, "Num"),
            Type::Str => write!(f, "Str"),
            Type::Bool => write!(f, "Bool"),
            Type::Vec2 => write!(f, "Vec2"),
            Type::Vec3 => write!(f, "Vec3"),
            Type::Vec4 => write!(f, "Vec4"),
            Type::Color => write!(f, "Color"),
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
            Type::Str => TypeAnnotation::Str,
            Type::Bool => TypeAnnotation::Bool,
            Type::Vec2 => TypeAnnotation::Vec2,
            Type::Vec3 => TypeAnnotation::Vec3,
            Type::Vec4 => TypeAnnotation::Vec4,
            Type::Color => TypeAnnotation::Color,
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
            TypeAnnotation::Str => Type::Str,
            TypeAnnotation::Bool => Type::Bool,
            TypeAnnotation::Vec2 => Type::Vec2,
            TypeAnnotation::Vec3 => Type::Vec3,
            TypeAnnotation::Vec4 => Type::Vec4,
            TypeAnnotation::Color => Type::Color,
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
    /// native plugin descriptors and analyzer manifests (previously duplicated
    /// as `parse_native_type` / `parse_manifest_type`). Unknown or malformed
    /// strings return `None`; enum annotations
    /// (`Enum(left, right, top)` or `Enum<...>`) are parsed into [`Type::Enum`].
    pub fn parse(text: &str) -> Option<Type> {
        let trimmed = text.trim();
        if let Some(variants) = parse_enum_annotation(trimmed) {
            return Some(Type::Enum(variants));
        }
        match trimmed {
            "Num" | "U32" => Some(Type::Num),
            "Str" | "String" => Some(Type::Str),
            "Bool" => Some(Type::Bool),
            "Vec2" => Some(Type::Vec2),
            "Vec3" => Some(Type::Vec3),
            "Vec4" => Some(Type::Vec4),
            "Color" => Some(Type::Color),
            "Any" => Some(Type::Any),
            "List<Vec2>" => Some(Type::List(Box::new(Type::Vec2))),
            _ => None,
        }
    }
}

/// Parse an `Enum(...)` / `Enum<...>` annotation into its variant names.
fn parse_enum_annotation(ty: &str) -> Option<Vec<String>> {
    let inner = ty
        .strip_prefix("Enum<")
        .and_then(|value| value.strip_suffix('>'))
        .or_else(|| ty.strip_prefix("Enum(").and_then(|value| value.strip_suffix(')')))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }
    let variants = inner
        .split([',', '|'])
        .map(str::trim)
        .filter(|variant| !variant.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    (!variants.is_empty()).then_some(variants)
}
