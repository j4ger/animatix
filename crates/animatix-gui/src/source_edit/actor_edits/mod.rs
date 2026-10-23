//! Edits related to actors: property changes, insertion, reordering,
//! reparenting, duplication, and renaming.

mod common;
mod delete;
mod duplicate;
mod effects;
mod insert;
mod properties;
mod rename;
mod reparent;

#[cfg(test)]
mod tests;

pub(super) use delete::delete_actor;
pub(super) use duplicate::{duplicate_actor, paste_actors};
pub(super) use effects::{insert_effect, insert_effect_param, set_effect_param};
pub(super) use insert::{insert_actor, insert_snippet};
pub(super) use properties::{insert_property, remove_property, set_property};
pub(crate) use rename::rename_all_references;
pub(super) use reparent::{reorder_container_children, reparent_actor};
