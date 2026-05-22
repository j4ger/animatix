use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::timeline::{Environment, EvalError, Value};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum BuiltinFn {
    Sin,
    Cos,
    Lerp,
    Format,
    Tan,
    Sqrt,
    Exp,
    Log,
    Atan2,
    Clamp,
    Abs,
    Min,
    Max,
    Floor,
    Ceil,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompiledExpr {
    Const(Value),
    LoadEnv(String),
    MakeVec(Vec<CompiledExpr>),
    Unary(UnaryOp, Box<CompiledExpr>),
    Binary(Box<CompiledExpr>, BinaryOp, Box<CompiledExpr>),
    Select(Box<CompiledExpr>, Box<CompiledExpr>, Box<CompiledExpr>),
    CallBuiltin(BuiltinFn, Vec<CompiledExpr>),
    Index(Box<CompiledExpr>, Box<CompiledExpr>),
    Method(Box<CompiledExpr>, String, Vec<CompiledExpr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModifierExpr {
    Compiled(CompiledExpr),
    Unsupported(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModifierIrStmt {
    Assign {
        target: Vec<String>,
        property: String,
        value: ModifierExpr,
    },
    Let {
        name: String,
        value: ModifierExpr,
    },
    If {
        condition: ModifierExpr,
        then_branch: Vec<ModifierIrStmt>,
        else_branch: Vec<ModifierIrStmt>,
    },
    For {
        var: String,
        iterable: CompiledExpr,
        body: Vec<ModifierIrStmt>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierIrProgram {
    pub statements: Vec<ModifierIrStmt>,
}

pub type ModifierOverrides = HashMap<String, HashMap<String, Value>>;

#[derive(Clone, Debug, PartialEq)]
pub enum IrLowerError {
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