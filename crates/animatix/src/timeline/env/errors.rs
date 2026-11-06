//! `EvalError` trait implementations: display, std error, and lowering conversion.

use std::fmt;

use super::EvalError;

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UndefinedVariable(v) => write!(f, "Undefined variable: {}", v),
            EvalError::TypeMismatch(e) => write!(f, "Type mismatch: {}", e),
            EvalError::NotCallable(n) => write!(f, "Not callable: {}", n),
            EvalError::UnsupportedMethod(name) => {
                write!(f, "Unsupported runtime method call: {}", name)
            },
            EvalError::UnsupportedIndex => {
                write!(f, "Unsupported runtime index expression")
            },
            EvalError::UnsupportedConstruct(name) => {
                write!(f, "Unsupported runtime construct expression: {}", name)
            },
        }
    }
}

impl std::error::Error for EvalError {}

impl From<crate::timeline::modifier_runtime::ir::IrLowerError> for EvalError {
    fn from(err: crate::timeline::modifier_runtime::ir::IrLowerError) -> Self {
        EvalError::UnsupportedConstruct(err.to_string())
    }
}
