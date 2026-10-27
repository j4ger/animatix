//! Behavior system: `BehaviorId`, `BehaviorFamily`, `BehaviorDef`, and `BehaviorRegistry`.
//!
//! Provides dense, O(1) indexable behavior definitions for all built-ins and runtime extensions.

mod def;
mod registry;
#[cfg(test)]
mod tests;

pub(crate) use def::BehaviorSource;
pub use def::{BehaviorDef, BehaviorFamily, BehaviorId};
pub use registry::{BehaviorRegistry, behavior_id_for_builtin};
