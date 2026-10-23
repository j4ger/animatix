//! Core AST definitions for the Animatix language.

use std::fmt::Debug;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

mod expr;
mod span;
mod stmt;
mod types;

pub use expr::*;
pub use span::*;
pub use stmt::*;
pub use types::*;
