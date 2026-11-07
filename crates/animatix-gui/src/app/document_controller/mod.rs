//! Document-level AST mutation operations.
//!
//! `DocumentController` borrows the three core stores and provides focused
//! methods for manipulating the AST, source text, editor buffer, and UI state.
//! It does **not** manage undo snapshots — the caller (`GuiShell`) is
//! responsible for calling `snapshot()` before delegating to a controller
//! method.

use crate::app::document::timeline_diff::KeyframeId;
use crate::app::preview::DragState;
use crate::app::stores::*;
use crate::source_edit;

use api::resolve_timeline_for_scene;
pub(crate) use commit::{commit_ast, commit_ast_with_debounce};

pub(crate) struct DocumentController<'a> {
    pub document_store: &'a mut DocumentStore,
    pub preview_store: &'a mut PreviewStore,
    pub ui_store: &'a mut UiStore,
}

mod api;
mod commit;
mod keyframes;
mod scenes;
