//! Store fixtures shared by every in-crate GUI test module.
//!
//! Test modules used to rebuild these stores by hand; each copy drifted (some
//! parsed the source, some did not). Everything here is `#[cfg(test)]`-only and
//! nothing in this module is part of the shipped GUI.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use animatix::timeline::SceneDimensions;

use crate::app::PreviewPaneState;
use crate::app::persistence::default_tree;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore, WorkspaceStore};
use crate::document::DocumentSession;
use crate::editor::EditorBuffer;

/// A `DocumentStore` over a placeholder source that does not parse.
///
/// Use this when the test only needs the store's plumbing (history, snapshots)
/// and never reads a timeline.
pub(crate) fn document_store_empty() -> DocumentStore {
    let path = PathBuf::from("test.amx");
    let document = DocumentSession::from_error(path.clone());
    let editor = EditorBuffer::new(&path, document.source_text.clone());
    DocumentStore::new(document, editor)
}

/// A `DocumentStore` over an in-memory source that must parse and rebuild.
///
/// No temp files: `from_source` + `rebuild` produces the same parsed AST and
/// timeline the disk-loading path builds.
pub(crate) fn make_document_store(source: &str) -> DocumentStore {
    let path = PathBuf::from("test.amx");
    let mut document =
        DocumentSession::from_source(path.clone(), source.to_string()).expect("valid source");
    document.rebuild().expect("valid source should rebuild");
    let editor = EditorBuffer::new(&path, document.source_text.clone());
    DocumentStore::new(document, editor)
}

/// A `DocumentStore` produced by the real disk-loading path.
///
/// Some tests need a session that came through `DocumentSession::load` (a real
/// `file_path`, workspace resolution, asset base dirs). The temp directory is
/// not cleaned up, matching the long-standing convention of `document.rs`.
pub(crate) fn document_store_loaded(source: &str) -> DocumentStore {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "animatix_test_domain_{}_{}",
        std::process::id(),
        unique
    ));
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    let path = dir.join("test.amx");
    std::fs::write(&path, source).expect("write test source");
    let document = DocumentSession::load(path).expect("load parsed document");
    let editor = EditorBuffer::new(&document.file_path, document.source_text.clone());
    DocumentStore::new(document, editor)
}

/// A preview store over the given duration and scene dimensions.
pub(crate) fn preview_store(duration_s: f64, dimensions: SceneDimensions) -> PreviewStore {
    PreviewStore::new(PreviewPaneState::new(duration_s, dimensions))
}

/// A UI store seeded with the default panel tree.
pub(crate) fn ui_store() -> UiStore {
    UiStore::new(default_tree())
}

/// A workspace store rooted at a throwaway directory.
pub(crate) fn workspace_store() -> WorkspaceStore {
    WorkspaceStore::new(
        PathBuf::from("/tmp/test"),
        HashSet::new(),
        vec![],
        PathBuf::from(".test_persistence.ron"),
        None,
        None,
    )
}
