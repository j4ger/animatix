#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::lookup::parse_numeric_vec2;
use super::{Diagnostic, Environment, Interpolate, KurboShape, VelloPath, evaluate_expr};
use crate::ast::Expr;
use crate::timeline::ShapeKind;

mod defaults;
mod shape_type;
mod states;
mod vector;

pub use defaults::*;
pub use shape_type::ShapeType;
pub use states::*;
pub use vector::*;

#[cfg(test)]
mod primitives;
#[cfg(test)]
mod tests;
