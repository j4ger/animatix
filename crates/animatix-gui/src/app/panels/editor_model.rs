//! View model for the editor panel.
//!
//! The editor is now rendered inside the Sidebar, but the standalone
//! EditorContext still exists for backward compatibility.

use crate::editor::EditorBuffer;
use animatix_syntax::diagnostics::Diagnostic;

/// Immutable view model for the editor panel.
#[allow(dead_code)]
/// View model for panel migration; panels still use mutable context.
pub struct EditorModel<'a> {
    pub editor: &'a EditorBuffer,
    pub diagnostics: &'a [Diagnostic],
    pub source_dirty: &'a str,
    pub is_playing: bool,
}