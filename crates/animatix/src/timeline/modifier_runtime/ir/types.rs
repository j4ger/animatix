use std::collections::HashMap;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::ast::{BinaryOp, Expr, LoopPattern, UnaryOp};
use crate::timeline::Value;
use crate::timeline::animation_track::SceneAnchor;

/// Built-in mathematical and utility functions available in modifier expressions.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuiltinFn {
    /// Sine function.
    Sin,
    /// Cosine function.
    Cos,
    /// Linear interpolation between two values.
    Lerp,
    /// String formatting function.
    Format,
    /// Tangent function.
    Tan,
    /// Square root.
    Sqrt,
    /// Exponential function.
    Exp,
    /// Logarithm.
    Log,
    /// Two-argument arctangent.
    Atan2,
    /// Clamp a value between bounds.
    Clamp,
    /// Absolute value.
    Abs,
    /// Minimum of values.
    Min,
    /// Maximum of values.
    Max,
    /// Floor function.
    Floor,
    /// Ceiling function.
    Ceil,
    /// Convert degrees to radians.
    Deg,
    /// Convert radians to degrees.
    Rad,
    /// Swap two elements in a list (returns new list).
    ListSwap,
    /// Set an element in a list (returns new list).
    ListSet,
}

/// A compiled expression in the modifier IR.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CompiledExpr {
    /// A constant value.
    Const(Value),
    /// Load a value from the environment by name.
    LoadEnv(String),
    /// Construct a vector of expressions.
    MakeVec(Vec<CompiledExpr>),
    /// Unary operation.
    Unary(UnaryOp, Box<CompiledExpr>),
    /// Binary operation.
    Binary(Box<CompiledExpr>, BinaryOp, Box<CompiledExpr>),
    /// Ternary conditional selection.
    Select(Box<CompiledExpr>, Box<CompiledExpr>, Box<CompiledExpr>),
    /// Call a built-in function.
    CallBuiltin(BuiltinFn, Vec<CompiledExpr>),
    /// Index into a collection.
    Index(Box<CompiledExpr>, Box<CompiledExpr>),
    /// Call a method on an expression.
    Method(Box<CompiledExpr>, String, Vec<CompiledExpr>),
    /// Create a closure value (parameter names, compiled body expression).
    /// The environment is captured at evaluation time.
    Closure(Vec<String>, Box<CompiledExpr>),
    /// Construct an object value (type name, compiled field expressions).
    Construct(String, Vec<(String, CompiledExpr)>),
    /// Lazily resolve an actor anchor point from the frame environment.
    /// `{actor}.{anchor}` → reads `{actor}.at` + `{actor}.size` from env.
    AnchorLookup {
        /// Actor label whose anchor point to resolve.
        actor: String,
        /// Which anchor point (top, right, center, etc.).
        anchor: SceneAnchor,
    },
}

impl CompiledExpr {
    /// Returns `true` if this compiled expression references the given identifier.
    pub fn references_ident(&self, name: &str) -> bool {
        match self {
            CompiledExpr::LoadEnv(id) => id == name,
            CompiledExpr::MakeVec(items) => items.iter().any(|item| item.references_ident(name)),
            CompiledExpr::Unary(_, expr) => expr.references_ident(name),
            CompiledExpr::Binary(left, _, right) => {
                left.references_ident(name) || right.references_ident(name)
            },
            CompiledExpr::Select(cond, then_expr, else_expr) => {
                cond.references_ident(name)
                    || then_expr.references_ident(name)
                    || else_expr.references_ident(name)
            },
            CompiledExpr::CallBuiltin(_, args) => args.iter().any(|arg| arg.references_ident(name)),
            CompiledExpr::Index(container, index) => {
                container.references_ident(name) || index.references_ident(name)
            },
            CompiledExpr::Method(receiver, _, args) => {
                receiver.references_ident(name) || args.iter().any(|arg| arg.references_ident(name))
            },
            CompiledExpr::Closure(_, body) => body.references_ident(name),
            CompiledExpr::Construct(_, fields) => {
                fields.iter().any(|(_, value)| value.references_ident(name))
            },
            CompiledExpr::Const(_) | CompiledExpr::AnchorLookup { .. } => false,
        }
    }
}

/// Expression used in modifier IR, either compiled or unsupported.
#[derive(Clone, Debug, PartialEq)]
pub enum ModifierExpr {
    /// A successfully compiled expression.
    Compiled(CompiledExpr),
    /// An expression that could not be compiled.
    Unsupported(Expr),
}

/// A statement in the modifier IR.
#[derive(Clone, Debug, PartialEq)]
pub enum ModifierIrStmt {
    /// Assign a value to a target object's property (all-static target path).
    Assign {
        /// Object path segments.
        target: Vec<String>,
        /// Property name to assign.
        property: String,
        /// Value expression.
        value: ModifierExpr,
    },
    /// Assign a value to a runtime-indexed target (e.g. `bars[i].color = red`).
    /// The base is the array label, index is compiled to a frame-time expression,
    /// property is the last segment (always static), and value is the RHS.
    AssignIndexed {
        /// Array base label (e.g. "bars").
        base: String,
        /// Frame-time index expression.
        index: CompiledExpr,
        /// Property name to assign.
        property: String,
        /// Value expression.
        value: ModifierExpr,
    },
    /// Bind a local variable.
    Let {
        /// Variable name.
        name: String,
        /// Bound expression.
        value: ModifierExpr,
    },
    /// Conditional statement.
    If {
        /// Condition expression.
        condition: ModifierExpr,
        /// Statements if true.
        then_branch: Vec<ModifierIrStmt>,
        /// Statements if false.
        else_branch: Vec<ModifierIrStmt>,
    },
    /// Loop over an iterable.
    For {
        /// Loop variable pattern (single or tuple destructuring).
        var: LoopPattern,
        /// Optional index variable name (e.g. `i` in `for item, i in items`).
        index_var: Option<String>,
        /// Iterable expression.
        iterable: CompiledExpr,
        /// Loop body statements.
        body: Vec<ModifierIrStmt>,
    },
    /// No-op statement (used as a placeholder during lowering).
    Noop,
}

/// A program in the modifier IR.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierIrProgram {
    /// Top-level statements.
    pub statements: Vec<ModifierIrStmt>,
}

/// Overrides for modifier properties, keyed by object and property name.
pub type ModifierOverrides = HashMap<String, HashMap<String, Value>>;

/// Errors during lowering to modifier IR.
#[derive(Clone, Debug, PartialEq)]
pub enum IrLowerError {
    /// A statement kind that is not supported.
    UnsupportedStatement(&'static str),
}

impl fmt::Display for IrLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrLowerError::UnsupportedStatement(kind) => {
                write!(f, "Unsupported IR statement: {kind}")
            },
        }
    }
}

impl std::error::Error for IrLowerError {}
