//! Store fixtures shared by the handler test modules.

use std::path::PathBuf;

use animatix::timeline::SceneDimensions;

use crate::app::PreviewPaneState;
use crate::app::stores::{DocumentStore, PreviewStore};
use crate::document::DocumentSession;
use crate::editor::EditorBuffer;

/// A `DocumentStore` over a valid in-memory source, with no temp files.
pub(crate) fn make_document_store(source: &str) -> DocumentStore {
    let path = PathBuf::from("test.amx");
    let mut document =
        DocumentSession::from_source(path.clone(), source.to_string()).expect("valid source");
    document.rebuild().expect("valid source should rebuild");
    let editor = EditorBuffer::new(&path, document.source_text.clone());
    DocumentStore::new(document, editor)
}

/// A preview store over the given duration and scene dimensions.
pub(crate) fn preview_store(duration_s: f64, dimensions: SceneDimensions) -> PreviewStore {
    PreviewStore::new(PreviewPaneState::new(duration_s, dimensions))
}
