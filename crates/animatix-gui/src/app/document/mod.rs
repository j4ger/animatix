//! GUI document-core compatibility module.
//!
//! This module contains shared types and APIs for the GUI's document model.
//! During the migration, it provides the active-timeline abstraction that
//! unifies single-scene Timeline and multi-scene Composition access.
//!
//! See rewrite-plan.md R1 for context.

pub(crate) mod active_timeline;
pub(crate) mod export_target;
pub(crate) mod history;
pub(crate) mod rebuild;
pub(crate) mod rebuild_output;
pub(crate) mod snapshot;
pub(crate) mod source_change;
pub(crate) mod version;
