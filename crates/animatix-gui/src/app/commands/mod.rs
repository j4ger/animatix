use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;

/// Side effects that can be produced by handling a command.
///
/// These are collected by `handle_command` and applied by `GuiShell::apply_effects`
/// after all state mutations for the command have been performed. This separates
/// side-effect concerns (UI notifications, repaint requests, editor interaction)
/// from the pure data mutations in the handler.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Push a toast notification to the UI overlay.
    Toast(crate::app::components::toast::Toast),
    /// Update the status bar text.
    Status(String),
    /// Request a preview repaint on the next frame.
    Repaint,
    /// Scroll the source editor to a specific line.
    EditorScroll(usize),
    /// Highlight a line in the source editor.
    EditorHighlight(usize),
}

// ── Shell commands ──────────────────────────────────────────────────────

/// Commands for document lifecycle, source synchronization, and review state.
///
/// Scene authorship happens through the source editor. Commands here select or
/// inspect the current document, control playback, and manage its lifecycle.
#[derive(Debug, Clone)]
pub enum Command {
    // ── Document / File ───────────────────────────────────────────────
    OpenFile(PathBuf),
    Save,
    Reload,
    ReloadPlugins,
    Rebuild,

    // ── Workspace / Explorer ──────────────────────────────────────────
    ToggleExpandDir(PathBuf),
    SwitchWorkspace(PathBuf),

    // ── Playback ──────────────────────────────────────────────────────
    TogglePlayback,
    ScrubTo(f64),
    PrevKeyframe,
    NextKeyframe,
    FrameStepForward,
    FrameStepBackward,
    StepNext,
    StepPrevious,
    TogglePresentationMode,

    // ── Scene ─────────────────────────────────────────────────────────
    SelectScene(String),
    // ── Editor sync modes ─────────────────────────────────────────────
    ToggleEditorSync,
    EditorChanged,
    ApplySourcePatch(crate::editor::source_patch::SourcePatch),

    // ── Undo / Redo ───────────────────────────────────────────────────
    Undo,
    Redo,

    // ── Navigation ────────────────────────────────────────────────────
    ScrollToLine(usize, usize),
    OpenParagraphTakes {
        epoch: crate::app::document::version::SourceEpoch,
        scene: Option<String>,
        paragraph: String,
    },
    RevealNarration {
        epoch: crate::app::document::version::SourceEpoch,
        scene: Option<String>,
        marker: String,
    },
    ReviewNarrationCue {
        epoch: crate::app::document::version::SourceEpoch,
        scene: Option<String>,
        line: usize,
        marker: String,
    },

    // ── Viewport ──────────────────────────────────────────────────────
    ZoomToSelection,
    ZoomToAll,

    // ── Find / Replace ────────────────────────────────────────────────
    FindReplaceAll,
}

// ── View actions ────────────────────────────────────────────────────────

/// View actions that change UI visibility or panel state without affecting
/// the document. These are not undoable.
#[derive(Debug, Clone)]
pub enum ViewAction {
    ShowInspector,
    UnpinInspector,
    TogglePreviewFullscreen,
    ShowCode,
    /// Show the bottom Curves tab (or hide the bottom region when it is
    /// already the active tab).
    ShowCurves,
    /// Show the bottom Timeline tab.
    ShowTimeline,
    ApplyLayout(crate::app::LayoutPreset),
    ResetLayout,
    OpenFileDialog,
    NewFile,
    NewProject(animatix_render::delivery::ProjectTemplate),
    SaveAsDialog,
    OpenSettings,
    OpenShortcuts,
    OpenExportDialog,
    OpenCommandPalette,
    OpenPluginStatus,
    OpenFindReplace,
}

// ── Unified action wrapper ──────────────────────────────────────────────

/// The unified action type consumed by the shell dispatcher each frame.
///
/// Commands update document synchronization or review state. View actions
/// affect workspace visibility and layout.
#[derive(Debug, Clone)]
pub enum ShellAction {
    Command(Command),
    View(ViewAction),
}

mod api;
mod queue;
#[cfg(test)]
mod tests;

pub use queue::{ActionQueue, CommandQueue, CommandSender, UndoEntry};
