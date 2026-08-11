//! Symbol-aware type inference for Animatix expressions.
//!
//! This module provides the internal type model shared by the syntax
//! typechecker and the analyzer. It deliberately keeps user-facing
//! [`TypeAnnotation`](crate::ast::TypeAnnotation) separate from the richer
//! internal [`Type`] used during inference.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinaryOp, Expr, TypeAnnotation};

/// Built-in colorscheme namespaces whose two-segment paths are colors.
pub const COLOR_NAMESPACES: &[&str] = &["accent", "text", "surface", "stroke"];

/// Built-in functions that construct a color value.
pub const COLOR_CONSTRUCTOR_FNS: &[&str] = &["rgb", "rgba", "hsv", "hsl", "hsla"];

/// Named color literals accepted by the runtime and static type layer.
pub const NAMED_COLOR_NAMES: &[&str] = &[
    "red", "RED", "green", "GREEN", "blue", "BLUE", "black", "BLACK", "white", "WHITE", "yellow",
    "YELLOW", "orange", "ORANGE",
];

/// Named color literal values in RGBA order.
pub fn named_color_rgba(name: &str) -> Option<[f64; 4]> {
    match name {
        "red" | "RED" => Some([1.0, 0.0, 0.0, 1.0]),
        "green" | "GREEN" => Some([0.0, 1.0, 0.0, 1.0]),
        "blue" | "BLUE" => Some([0.0, 0.0, 1.0, 1.0]),
        "black" | "BLACK" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" | "WHITE" => Some([1.0, 1.0, 1.0, 1.0]),
        "yellow" | "YELLOW" => Some([1.0, 1.0, 0.0, 1.0]),
        "orange" | "ORANGE" => Some([1.0, 0.65, 0.0, 1.0]),
        _ => None,
    }
}

/// A namespaced value, either a concrete type or a nested namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamespaceType {
    /// A concrete exported value type.
    Value(Type),
    /// A nested namespace with named members.
    Namespace(HashMap<String, NamespaceType>),
}

impl Default for NamespaceType {
    fn default() -> Self {
        NamespaceType::Namespace(HashMap::new())
    }
}

/// Internal inferred type for expressions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// Accepts any value.
    Any,
    /// Number.
    Num,
    /// String.
    Str,
    /// Boolean.
    Bool,
    /// 2D vector.
    Vec2,
    /// 3D vector.
    Vec3,
    /// 4D vector.
    Vec4,
    /// RGBA color.
    Color,
    /// Actor label.
    Actor(String),
    /// Component instance.
    Component(String),
    /// Homogeneous list.
    List(Box<Type>),
    /// Heterogeneous tuple.
    Tuple(Vec<Type>),
    /// Union of accepted types.
    Union(Vec<Type>),
    /// Function value.
    Function {
        /// Function parameter types.
        params: Vec<Type>,
        /// Function return type.
        ret: Box<Type>,
    },
}

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
            Type::List(inner) => write!(f, "List<{inner}>"),
            Type::Union(items) => {
                let inner = items.iter().map(ToString::to_string).collect::<Vec<_>>().join(" | ");
                write!(f, "{inner}")
            },
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
            Type::Vec3 => TypeAnnotation::Any,
            Type::Vec4 => TypeAnnotation::Vec4,
            Type::Color => TypeAnnotation::Color,
            Type::Actor(_) | Type::Component(_) => TypeAnnotation::Actor,
            Type::List(inner) => TypeAnnotation::List(Box::new(inner.to_annotation())),
            Type::Union(types) => {
                TypeAnnotation::Union(types.iter().map(Type::to_annotation).collect())
            },
            Type::Tuple(_) | Type::Function { .. } => TypeAnnotation::Any,
        }
    }

    /// Convert a user-facing annotation into the richer internal type.
    pub fn from_annotation(annotation: &TypeAnnotation) -> Self {
        match annotation {
            TypeAnnotation::Num => Type::Num,
            TypeAnnotation::Str => Type::Str,
            TypeAnnotation::Bool => Type::Bool,
            TypeAnnotation::Vec2 => Type::Vec2,
            TypeAnnotation::Vec4 => Type::Vec4,
            TypeAnnotation::Color => Type::Color,
            TypeAnnotation::Actor => Type::Actor("Actor".to_string()),
            TypeAnnotation::Scene => Type::Any,
            TypeAnnotation::List(inner) => Type::List(Box::new(Type::from_annotation(inner))),
            TypeAnnotation::Union(types) => {
                Type::Union(types.iter().map(Type::from_annotation).collect())
            },
            TypeAnnotation::Alias(_) => Type::Any,
            TypeAnnotation::Any => Type::Any,
        }
    }
}

/// Component parameter signature.
#[derive(Clone, Debug, Default)]
pub struct ComponentSignature {
    /// Parameter name -> inferred parameter type.
    pub params: HashMap<String, Type>,
}

/// A symbol-aware type environment.
///
/// The environment is intentionally owned and cloneable so the analyzer can
/// snapshot it for hover/completion queries.
#[derive(Clone, Debug, Default)]
pub struct TypeEnv {
    scopes: Vec<HashMap<String, Type>>,
    actors: HashMap<String, Type>,
    components: HashMap<String, ComponentSignature>,
    instances: HashMap<String, String>,
    arrays: HashMap<String, Type>,
    aliases: HashMap<String, TypeAnnotation>,
    namespaces: HashMap<String, NamespaceType>,
    builtins: HashMap<String, Type>,
    functions: HashMap<String, Type>,
    construct_types: HashMap<String, Type>,
}

impl TypeEnv {
    /// Create an empty type environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an environment seeded with built-in literals and functions.
    pub fn with_stdlib() -> Self {
        let mut env = Self::new();
        env.register_stdlib();
        env
    }

    /// Register a user-facing type alias, resolving it lazily so aliases may
    /// reference aliases declared later in the file.
    pub fn register_alias(&mut self, name: &str, annotation: &TypeAnnotation) {
        self.aliases.insert(name.to_string(), annotation.clone());
    }

    /// Resolve a type annotation, following named aliases.
    ///
    /// Unresolved aliases resolve to `Any` instead of failing so the gradual
    /// type checker keeps accepting otherwise valid files; callers that need a
    /// hard diagnostic should use [`Self::unresolved_alias`] on the annotation
    /// before treating the value as usable.
    pub fn resolve_annotation(&self, annotation: &TypeAnnotation) -> Type {
        match annotation {
            TypeAnnotation::Alias(name) => self
                .resolve_alias_path(name, &mut HashSet::new())
                .unwrap_or(Type::Any),
            _ => Type::from_annotation(annotation),
        }
    }

    /// Return the first unresolved alias path in this annotation, if any.
    pub fn unresolved_alias(&self, annotation: &TypeAnnotation) -> Option<String> {
        match annotation {
            TypeAnnotation::Alias(name) => {
                self.resolve_alias_path(name, &mut HashSet::new()).err()
            },
            TypeAnnotation::List(inner) => self.unresolved_alias(inner),
            TypeAnnotation::Union(types) => types.iter().find_map(|ty| self.unresolved_alias(ty)),
            _ => None,
        }
    }

    fn resolve_alias_path(
        &self,
        name: &str,
        visiting: &mut HashSet<String>,
    ) -> Result<Type, String> {
        if !visiting.insert(name.to_string()) {
            return Err(name.to_string());
        }
        let result = match self.aliases.get(name) {
            Some(TypeAnnotation::Alias(next)) => self.resolve_alias_path(next, visiting),
            Some(annotation) => Ok(Type::from_annotation(annotation)),
            None => Err(name.to_string()),
        };
        visiting.remove(name);
        result
    }

    /// Seed built-in named colors, color constructors, numeric functions, and
    /// known construct types.
    pub fn register_stdlib(&mut self) {
        for name in NAMED_COLOR_NAMES {
            self.builtins.insert((*name).to_string(), Type::Color);
        }
        for name in COLOR_CONSTRUCTOR_FNS {
            self.functions.insert((*name).to_string(), Type::Color);
        }
        for name in ["format"] {
            self.functions.insert(name.to_string(), Type::Str);
        }
        for name in [
            "abs",
            "clamp",
            "ceil",
            "cos",
            "deg",
            "exp",
            "floor",
            "lerp",
            "log",
            "max",
            "min",
            "rad",
            "rand",
            "seeded_rand",
            "sin",
            "sqrt",
            "tan",
        ] {
            self.functions.insert(name.to_string(), Type::Num);
        }
        self.construct_types.insert("Color".to_string(), Type::Color);
        self.construct_types
            .insert("Point".to_string(), Type::Tuple(vec![Type::Num, Type::Num]));
    }

    /// Push a lexical scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop a lexical scope.
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Bind a local variable, parameter, or loop variable.
    pub fn bind(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        } else {
            self.builtins.insert(name.to_string(), ty);
        }
    }

    /// Declare an actor label with its primitive/component type name.
    pub fn declare_actor(&mut self, label: &str, ty: &str) {
        self.actors.insert(label.to_string(), Type::Actor(ty.to_string()));
    }

    /// Declare an array base label and its element type.
    pub fn declare_array(&mut self, label: &str, element_type: Type) {
        self.arrays.insert(label.to_string(), Type::List(Box::new(element_type)));
    }

    /// Register a component signature.
    pub fn register_component(&mut self, name: &str, signature: ComponentSignature) {
        self.components.insert(name.to_string(), signature);
    }

    /// Declare a component instance label.
    pub fn declare_component_instance(&mut self, label: &str, component_name: &str) {
        self.instances.insert(label.to_string(), component_name.to_string());
    }

    /// Look up a bare identifier: local scope, actor, component instance,
    /// array base, then builtin constant.
    pub fn lookup_ident(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        if let Some(ty) = self.actors.get(name) {
            return Some(ty.clone());
        }
        if let Some(component) = self.instances.get(name) {
            return Some(Type::Component(component.clone()));
        }
        if let Some(ty) = self.arrays.get(name) {
            return Some(ty.clone());
        }
        self.builtins.get(name).cloned()
    }

    /// Register a concrete value in an aliased namespace.
    pub fn register_namespace_value(&mut self, alias: &str, name: &str, ty: Type) {
        let entry = self
            .namespaces
            .entry(alias.to_string())
            .or_insert_with(|| NamespaceType::Namespace(HashMap::new()));
        if let NamespaceType::Namespace(values) = entry {
            values.insert(name.to_string(), NamespaceType::Value(ty));
        }
    }

    /// Register a nested namespace value.
    pub fn register_namespace(&mut self, alias: &str, namespace: NamespaceType) {
        self.namespaces.insert(alias.to_string(), namespace);
    }

    /// Look up a namespaced path such as `deck.bar` or `accent.primary`.
    fn lookup_path(&self, parts: &[String]) -> Option<Type> {
        if parts.len() >= 2 && COLOR_NAMESPACES.contains(&parts[0].as_str()) {
            return Some(Type::Color);
        }
        let first = parts.first()?;
        if let Some(namespace) = self.namespaces.get(first) {
            let mut current = namespace;
            for part in &parts[1..parts.len() - 1] {
                match current {
                    NamespaceType::Namespace(values) => current = values.get(part)?,
                    NamespaceType::Value(_) => return None,
                }
            }
            if let Some(last) = parts.last() {
                match current {
                    NamespaceType::Namespace(values) => {
                        if let Some(NamespaceType::Value(ty)) = values.get(last) {
                            return Some(ty.clone());
                        }
                    },
                    NamespaceType::Value(ty) if parts.len() == 2 => {
                        return Some(ty.clone());
                    },
                    NamespaceType::Value(_) => {},
                }
            }
        }
        let base = self.lookup_ident(first)?;
        if parts.len() == 1 {
            return Some(base);
        }
        match base {
            Type::Actor(actor_ty) => {
                self.lookup_type_path(Type::Actor(actor_ty), first, &parts[1..])
            },
            Type::Component(component) => {
                self.lookup_type_path(Type::Component(component), first, &parts[1..])
            },
            _ => None,
        }
    }

    fn lookup_type_path(&self, receiver: Type, base_name: &str, parts: &[String]) -> Option<Type> {
        let Some(first) = parts.first() else {
            return Some(receiver);
        };
        match receiver {
            Type::Actor(actor_ty) => {
                let ty = property_type(&actor_ty, first)?;
                self.lookup_type_path(ty, first, &parts[1..])
            },
            Type::Component(component) => {
                let sig = self.components.get(&component)?;
                let ty = sig
                    .params
                    .get(first)
                    .cloned()
                    .or_else(|| self.arrays.get(&format!("{base_name}.{first}")).cloned())?;
                self.lookup_type_path(ty, first, &parts[1..])
            },
            _ => None,
        }
    }

    /// Resolve an actor/component property by name.
    fn lookup_property(&self, receiver: &Type, name: &str) -> Option<Type> {
        match receiver {
            Type::Actor(actor_ty) => property_type(actor_ty, name),
            Type::Component(component) => {
                let sig = self.components.get(component)?;
                sig.params.get(name).cloned()
            },
            _ => None,
        }
    }

    /// Look up a function return type.
    pub fn function_type(&self, name: &str) -> Option<Type> {
        self.functions.get(name).cloned()
    }

    /// Look up a construct type.
    pub fn construct_type(&self, name: &str) -> Option<Type> {
        self.construct_types.get(name).cloned()
    }
}

/// Resolve a property type for a primitive actor type.
pub fn property_type(actor_type: &str, property: &str) -> Option<Type> {
    match property {
        "color" | "fill" | "stroke" => Some(Type::Color),
        "at" | "anchor" | "position" | "offset" | "size" => Some(Type::Vec2),
        "opacity" | "scale" | "rotation" | "font_size" | "stroke_width" | "line_height"
        | "letter_spacing" | "word_spacing" | "head_size" | "standoff" => Some(Type::Num),
        "text" | "content" | "code" | "label" | "url" | "place" => Some(Type::Str),
        _ => match actor_type {
            "Graph" | "PlotCurve" => match property {
                "x_domain" | "y_domain" | "t_domain" => Some(Type::Vec2),
                "func" => Some(Type::Function {
                    params: vec![],
                    ret: Box::new(Type::Num),
                }),
                _ => None,
            },
            _ => None,
        },
    }
}

/// Known property types per `(actor_type, property)` pair.
///
/// This is the shared source for the syntax typechecker, analyzer-level
/// property diagnostics, and property completions. It intentionally remains
/// additive relative to [`property_type`], which covers the generic schema used
/// by actor processing.
pub fn known_property_types() -> &'static HashMap<(String, String), Type> {
    static CACHE: std::sync::OnceLock<HashMap<(String, String), Type>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        for (ty, property, property_type) in property_specs() {
            map.insert((ty.to_string(), property.to_string()), property_type);
        }
        map
    })
}

/// Known property names per actor type, derived from [`known_property_types`].
///
/// Keeping this derived avoids the analyzer maintaining a second hand-written
/// property registry that can drift from the type checker.
pub fn known_properties() -> &'static HashMap<String, Vec<String>> {
    static CACHE: std::sync::OnceLock<HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = HashMap::<String, Vec<String>>::new();
        for (ty, property, _) in property_specs() {
            let properties = map.entry(ty.to_string()).or_default();
            if !properties.iter().any(|existing| existing == property) {
                properties.push(property.to_string());
            }
        }
        map
    })
}

/// Single ordered source for the known property registry.
fn property_specs() -> Vec<(&'static str, &'static str, Type)> {
    let mut specs = Vec::new();

    let common: &[(&'static str, Type)] = &[
        ("position", Type::Vec2),
        ("anchor", Type::Vec2),
        ("offset", Type::Vec2),
        ("scale", Type::Num),
        ("rotation", Type::Num),
        ("opacity", Type::Num),
        ("color", Type::Color),
        ("at", Type::Vec2),
    ];
    for ty in [
        "Text", "Code", "Typst", "Rect", "Ellipse", "Polygon", "Line", "Button", "Svg", "Image",
        "Graph", "PlotCurve",
    ] {
        specs.extend(
            common
                .iter()
                .map(|(property, property_type)| (ty, *property, property_type.clone())),
        );
    }

    specs.extend([
        // Text-specific
        ("Text", "text", Type::Str),
        ("Text", "content", Type::Str),
        ("Text", "font_size", Type::Num),
        ("Text", "font_family", Type::Str),
        ("Text", "font_weight", Type::Num),
        ("Text", "font_style", Type::Str),
        ("Text", "line_height", Type::Num),
        ("Text", "letter_spacing", Type::Num),
        ("Text", "word_spacing", Type::Num),
        ("Text", "max_width", Type::Num),
        ("Text", "text_align", Type::Str),
        ("Text", "overflow", Type::Str),
        // Typst-specific
        ("Typst", "content", Type::Str),
        ("Typst", "font_size", Type::Num),
        ("Typst", "font_family", Type::Str),
        ("Typst", "font_weight", Type::Num),
        ("Typst", "font_style", Type::Str),
        ("Typst", "line_height", Type::Num),
        ("Typst", "letter_spacing", Type::Num),
        ("Typst", "word_spacing", Type::Num),
        ("Typst", "max_width", Type::Num),
        ("Typst", "text_align", Type::Str),
        ("Typst", "overflow", Type::Str),
        // Code-specific
        ("Code", "code", Type::Str),
        ("Code", "content", Type::Str),
        ("Code", "language", Type::Str),
        ("Code", "font_weight", Type::Num),
        ("Code", "font_style", Type::Str),
        ("Code", "line_height", Type::Num),
        ("Code", "letter_spacing", Type::Num),
        ("Code", "word_spacing", Type::Num),
        ("Code", "max_width", Type::Num),
        ("Code", "text_align", Type::Str),
        ("Code", "overflow", Type::Str),
        // Shape-specific
        ("Rect", "fill", Type::Color),
        ("Rect", "stroke", Type::Color),
        ("Rect", "stroke_width", Type::Num),
        ("Rect", "size", Type::Vec2),
        ("Rect", "radius", Type::Num),
        ("Ellipse", "fill", Type::Color),
        ("Ellipse", "stroke", Type::Color),
        ("Ellipse", "stroke_width", Type::Num),
        ("Ellipse", "size", Type::Vec2),
        ("Ellipse", "radius", Type::Num),
        ("Polygon", "fill", Type::Color),
        ("Polygon", "stroke", Type::Color),
        ("Polygon", "stroke_width", Type::Num),
        ("Polygon", "size", Type::Vec2),
        ("Polygon", "radius", Type::Num),
        // Line
        ("Line", "start", Type::Vec2),
        ("Line", "end", Type::Vec2),
        ("Line", "stroke", Type::Color),
        ("Line", "stroke_width", Type::Num),
        // Button
        ("Button", "text", Type::Str),
        ("Button", "size", Type::Vec2),
        ("Button", "fill", Type::Color),
        ("Button", "stroke", Type::Color),
        // Svg/Image
        ("Svg", "url", Type::Str),
        ("Svg", "size", Type::Vec2),
        ("Image", "url", Type::Str),
        ("Image", "size", Type::Vec2),
        // Graph
        ("Graph", "x_range", Type::Vec2),
        ("Graph", "y_range", Type::Vec2),
        ("Graph", "function", Type::Str),
        ("PlotCurve", "x_range", Type::Vec2),
        ("PlotCurve", "y_range", Type::Vec2),
        ("PlotCurve", "function", Type::Str),
    ]);

    specs
}

/// Compute the common supertype for a list of inferred types.
pub fn common_type(types: &[Type]) -> Type {
    if types.is_empty() {
        return Type::Any;
    }
    if types.iter().all(|t| *t == types[0]) {
        return types[0].clone();
    }
    if types.iter().all(|t| matches!(t, Type::Color | Type::Vec4)) {
        return Type::Vec4;
    }
    if types.iter().all(|t| matches!(t, Type::Num)) {
        return Type::Num;
    }
    Type::Any
}

/// Check whether `actual` is a subtype of `expected`.
pub fn is_subtype(actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
        (_, Type::Any) => true,
        (Type::Any, _) => true,
        (a, b) if a == b => true,
        (Type::Color, Type::Vec4) => true,
        (Type::List(a), Type::List(b)) => is_subtype(a, b),
        (Type::Actor(_), Type::Actor(_)) => true,
        (Type::Component(_), Type::Actor(_)) => true,
        (Type::Union(actual_types), Type::Union(expected_types)) => actual_types
            .iter()
            .any(|actual| expected_types.iter().any(|expected| is_subtype(actual, expected))),
        (Type::Union(actual_types), expected) => {
            actual_types.iter().any(|actual| is_subtype(actual, expected))
        },
        (actual, Type::Union(expected_types)) => {
            expected_types.iter().any(|expected| is_subtype(actual, expected))
        },
        _ => false,
    }
}

/// Infer the type of an expression in a symbol-aware environment.
pub fn infer_expr_type(expr: &Expr, env: &TypeEnv) -> Type {
    match expr {
        Expr::Num(_) | Expr::Percent(_) => Type::Num,
        Expr::Str(_) => Type::Str,
        Expr::Bool(_) => Type::Bool,
        Expr::Null => Type::Any,
        Expr::Ident(name) => env.lookup_ident(name).unwrap_or(Type::Any),
        Expr::Path(parts) => env.lookup_path(parts).unwrap_or(Type::Any),
        Expr::Index(base, _) => match infer_expr_type(base, env) {
            Type::List(element) => *element,
            Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Color => Type::Num,
            Type::Str => Type::Str,
            _ => Type::Any,
        },
        Expr::Tuple(items) => {
            let types = items.iter().map(|item| infer_expr_type(item, env)).collect::<Vec<_>>();
            match types.as_slice() {
                [_, _] => Type::Vec2,
                [a, b, c] if *a == Type::Num && *b == Type::Num && *c == Type::Num => Type::Vec3,
                [a, b, c, d]
                    if *a == Type::Num && *b == Type::Num && *c == Type::Num && *d == Type::Num =>
                {
                    Type::Vec4
                },
                _ => Type::Tuple(types),
            }
        },
        Expr::List(items) => {
            let types = items.iter().map(|item| infer_expr_type(item, env)).collect::<Vec<_>>();
            Type::List(Box::new(common_type(&types)))
        },
        Expr::Binary(left, op, right) => {
            let left_ty = infer_expr_type(left, env);
            let right_ty = infer_expr_type(right, env);
            match op {
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::Pow => {
                    if left_ty == Type::Str && *op == BinaryOp::Add {
                        Type::Str
                    } else if left_ty == Type::Num && right_ty == Type::Num {
                        Type::Num
                    } else if left_ty == Type::Color && right_ty == Type::Color {
                        Type::Color
                    } else {
                        Type::Any
                    }
                },
                BinaryOp::Eq
                | BinaryOp::Neq
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Lte
                | BinaryOp::Gte
                | BinaryOp::And
                | BinaryOp::Or => Type::Bool,
            }
        },
        Expr::Unary(_, inner) => match infer_expr_type(inner, env) {
            Type::Num => Type::Num,
            Type::Bool => Type::Bool,
            other => other,
        },
        Expr::Call(name, _) => env.function_type(name).unwrap_or(Type::Any),
        Expr::Method(receiver, name, args) => {
            if args.is_empty() {
                let receiver_ty = infer_expr_type(receiver, env);
                if let Some(ty) = env.lookup_property(&receiver_ty, name) {
                    return ty;
                }
                if matches!(receiver_ty, Type::List(_)) && name == "length" {
                    return Type::Num;
                }
                return Type::Any;
            }
            let receiver_ty = infer_expr_type(receiver, env);
            match (name.as_str(), receiver_ty) {
                ("get", Type::List(element)) => *element,
                ("get", Type::Str) => Type::Str,
                _ => Type::Any,
            }
        },
        Expr::Closure(_, _) => Type::Any,
        Expr::Conditional(_, then_expr, else_expr) => common_type(&[
            infer_expr_type(then_expr, env),
            infer_expr_type(else_expr, env),
        ]),
        Expr::Match(_, arms) => {
            let types = arms.iter().map(|(_, arm)| infer_expr_type(arm, env)).collect::<Vec<_>>();
            common_type(&types)
        },
        Expr::Construct(name, _) => env.construct_type(name).unwrap_or(Type::Any),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn std_env() -> TypeEnv {
        TypeEnv::with_stdlib()
    }

    #[test]
    fn named_colors_are_colors() {
        let env = std_env();
        assert_eq!(infer_expr_type(&Expr::Ident("red".to_string()), &env), Type::Color);
        assert_eq!(infer_expr_type(&Expr::Ident("BLUE".to_string()), &env), Type::Color);
    }

    #[test]
    fn color_namespace_paths_are_colors() {
        let env = std_env();
        for ns in COLOR_NAMESPACES {
            let path = Expr::Path(vec![ns.to_string(), "primary".to_string()]);
            assert_eq!(infer_expr_type(&path, &env), Type::Color);
        }
        let scene = Expr::Path(vec!["scene".to_string(), "background".to_string()]);
        assert_eq!(infer_expr_type(&scene, &env), Type::Any);
    }

    #[test]
    fn color_functions_return_colors() {
        let env = std_env();
        for name in COLOR_CONSTRUCTOR_FNS {
            let call = Expr::Call(name.to_string(), vec![]);
            assert_eq!(infer_expr_type(&call, &env), Type::Color);
        }
    }

    #[test]
    fn list_infers_common_color_type() {
        let env = std_env();
        let list = Expr::List(vec![
            Expr::Ident("red".to_string()),
            Expr::Ident("blue".to_string()),
        ]);
        assert_eq!(infer_expr_type(&list, &env), Type::List(Box::new(Type::Color)));
    }

    #[test]
    fn list_infers_vec4_for_color_vec4_mix() {
        let env = std_env();
        let list = Expr::List(vec![
            Expr::Ident("red".to_string()),
            Expr::Tuple(vec![
                Expr::Num(0.0),
                Expr::Num(0.0),
                Expr::Num(0.0),
                Expr::Num(1.0),
            ]),
        ]);
        assert_eq!(infer_expr_type(&list, &env), Type::List(Box::new(Type::Vec4)));
    }

    #[test]
    fn let_bindings_shadow_literal_names() {
        let mut env = std_env();
        env.bind("red", Type::Num);
        assert_eq!(infer_expr_type(&Expr::Ident("red".to_string()), &env), Type::Num);
    }

    #[test]
    fn namespace_paths_resolve() {
        let mut env = std_env();
        env.register_namespace_value("theme", "accent", Type::Color);
        let path = Expr::Path(vec!["theme".to_string(), "accent".to_string()]);
        assert_eq!(infer_expr_type(&path, &env), Type::Color);
    }

    #[test]
    fn nested_namespace_paths_resolve() {
        let mut env = std_env();
        let mut nested = HashMap::new();
        nested.insert("accent".to_string(), NamespaceType::Value(Type::Color));
        env.register_namespace("project", NamespaceType::Namespace(nested));
        let path = Expr::Path(vec!["project".to_string(), "accent".to_string()]);
        assert_eq!(infer_expr_type(&path, &env), Type::Color);
    }

    #[test]
    fn actor_property_paths_infer() {
        let mut env = std_env();
        env.declare_actor("box", "Rect");
        let path = Expr::Path(vec!["box".to_string(), "color".to_string()]);
        assert_eq!(infer_expr_type(&path, &env), Type::Color);
    }

    #[test]
    fn list_common_type_is_subtype_compatible() {
        assert!(is_subtype(&Type::Color, &Type::Vec4));
        assert!(is_subtype(
            &Type::List(Box::new(Type::Color)),
            &Type::List(Box::new(Type::Vec4))
        ));
    }

    #[test]
    fn aliases_resolve_when_declared_later() {
        let mut env = TypeEnv::with_stdlib();
        env.register_alias("Metric", &TypeAnnotation::Alias("Inner".to_string()));
        env.register_alias("Inner", &TypeAnnotation::Union(vec![
            TypeAnnotation::Bool,
            TypeAnnotation::Str,
        ]));
        assert_eq!(
            env.resolve_annotation(&TypeAnnotation::Alias("Metric".to_string())),
            Type::Union(vec![Type::Bool, Type::Str])
        );
        assert!(env.unresolved_alias(&TypeAnnotation::Alias("Metric".to_string())).is_none());
    }

    #[test]
    fn unresolved_alias_is_reported_instead_of_silently_accepted() {
        let env = TypeEnv::with_stdlib();
        assert_eq!(
            env.unresolved_alias(&TypeAnnotation::Alias("Missing".to_string())),
            Some("Missing".to_string())
        );
        assert_eq!(
            env.resolve_annotation(&TypeAnnotation::Alias("Missing".to_string())),
            Type::Any
        );
    }

    #[test]
    fn alias_cycles_are_reported() {
        let mut env = TypeEnv::with_stdlib();
        env.register_alias("A", &TypeAnnotation::Alias("B".to_string()));
        env.register_alias("B", &TypeAnnotation::Alias("A".to_string()));
        assert_eq!(
            env.unresolved_alias(&TypeAnnotation::Alias("A".to_string())),
            Some("A".to_string())
        );
    }

    #[test]
    fn known_properties_are_derived_from_typed_registry() {
        let typed = known_property_types();
        for (ty, properties) in known_properties() {
            for property in properties {
                assert!(
                    typed.contains_key(&(ty.clone(), property.clone())),
                    "{} is listed for {} but has no expected type",
                    property,
                    ty
                );
            }
        }
    }

    #[test]
    fn typst_shares_common_layout_properties() {
        let properties = known_properties();
        let typst = properties.get("Typst").expect("Typst property list");
        for property in ["position", "anchor", "offset", "scale", "rotation", "opacity", "color"] {
            assert!(typst.contains(&property.to_string()), "missing common {property}");
        }
    }

    #[test]
    fn plot_curve_has_graph_ranges() {
        let properties = known_properties();
        let plot_curve = properties.get("PlotCurve").expect("PlotCurve property list");
        for property in ["x_range", "y_range", "function"] {
            assert!(plot_curve.contains(&property.to_string()), "missing {property}");
        }
    }
}
