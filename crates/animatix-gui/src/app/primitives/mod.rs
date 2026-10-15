//! Primitive abstraction adapter layer for the GUI.
//!
//! Provides a unified interface for actor templates, canvas gizmos,
//! and primitive inspection.

pub mod gizmos;
pub mod template;

pub use gizmos::{GizmoAffordance, actor_gizmo_affordances};
pub use template::ActorTemplate;
