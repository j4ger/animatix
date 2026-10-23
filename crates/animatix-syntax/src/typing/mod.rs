//! Symbol-aware type inference for Animatix expressions.
//!
//! This module provides the internal type model shared by the syntax
//! typechecker and the analyzer. It deliberately keeps user-facing
//! [`TypeAnnotation`](crate::ast::TypeAnnotation) separate from the richer
//! internal `Type` used during inference.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::ast::{BinaryOp, Expr, TypeAnnotation};
pub use crate::builtins::{
    COLOR_CONSTRUCTOR_FUNCTIONS as COLOR_CONSTRUCTOR_FNS, COLOR_NAMES as NAMED_COLOR_NAMES,
    COLOR_NAMESPACES, MATH_FUNCTIONS as MATH_FUNCTION_NAMES, named_color_rgba,
};
use crate::schema::property_specs;

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
    /// Scene reference.
    Scene,
    /// Homogeneous list.
    List(Box<Type>),
    /// Heterogeneous tuple.
    Tuple(Vec<Type>),
    /// Union of accepted types.
    Union(Vec<Type>),
    /// Named enum choices used by manifest/native extension properties.
    Enum(Vec<String>),
    /// Function value.
    Function {
        /// Function parameter types.
        params: Vec<Type>,
        /// Function return type.
        ret: Box<Type>,
    },
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

/// The language's built-in bare identifiers, keyed by borrowed names.
///
/// Consulted by [`TypeEnv::lookup_ident`] *after* the environment's own
/// `builtins` map, so a user binding still shadows a built-in. Built once per
/// process; see [`TypeEnv::with_stdlib`] for why this is not copied into every
/// environment.
static STDLIB_BUILTINS: LazyLock<HashMap<&'static str, Type>> = LazyLock::new(|| {
    let mut table: HashMap<&'static str, Type> =
        NAMED_COLOR_NAMES.iter().map(|name| (*name, Type::Color)).collect();
    // Numeric constants used as bare identifiers, plus the frame-time variable.
    table.extend([
        ("pi", Type::Num),
        ("tau", Type::Num),
        ("e", Type::Num),
        ("t", Type::Num),
    ]);
    table
});

/// The language's built-in functions and their return types.
static STDLIB_FUNCTIONS: LazyLock<HashMap<&'static str, Type>> = LazyLock::new(|| {
    let mut table: HashMap<&'static str, Type> =
        COLOR_CONSTRUCTOR_FNS.iter().map(|name| (*name, Type::Color)).collect();
    table.extend(MATH_FUNCTION_NAMES.iter().map(|name| (*name, Type::Num)));
    table.insert("format", Type::Str);
    table
});

/// The language's built-in construct types.
static STDLIB_CONSTRUCTS: LazyLock<HashMap<&'static str, Type>> = LazyLock::new(|| {
    HashMap::from([
        ("Color", Type::Color),
        ("Point", Type::Tuple(vec![Type::Num, Type::Num])),
    ])
});

mod env;
mod infer;
mod tables;
#[cfg(test)]
mod tests;
mod type_impl;

pub(crate) use env::transform_type;
pub use infer::*;
pub use tables::*;
