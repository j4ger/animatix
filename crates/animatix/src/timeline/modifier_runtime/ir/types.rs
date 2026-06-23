use crate::ast::{BinaryOp, Expr, LoopPattern, UnaryOp};
use crate::timeline::Value;
use std::collections::HashMap;
use std::fmt;

/// Built-in mathematical and utility functions available in modifier expressions.
#[derive(Clone, Debug, PartialEq)]
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
}

/// A compiled expression in the modifier IR.
#[derive(Clone, Debug, PartialEq)]
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
    /// Assign a value to a target object's property.
    Assign {
        /// Object path segments.
        target: Vec<String>,
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
        /// Iterable expression.
        iterable: CompiledExpr,
        /// Loop body statements.
        body: Vec<ModifierIrStmt>,
    },
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
            }
        }
    }
}

impl std::error::Error for IrLowerError {}
