use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::app::document::timeline_diff::KeyframeId;

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

// ── Domain commands ─────────────────────────────────────────────────────

/// Domain commands that mutate the document, timeline, or file system.
///
/// These are the commands that can be snapshotted for undo/redo.
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
    ReorderScenes(Vec<String>),
    SetTransition {
        from_scene: String,
        transition: animatix_syntax::ast::Transition,
    },
    SetPlayTarget {
        from_scene: String,
        target: Option<String>,
    },
    SetSceneDuration {
        scene: String,
        duration_s: Option<f64>,
    },
    DuplicateScene(String),
    DeleteScene(String),
    AddScene(String),

    // ── Actor ─────────────────────────────────────────────────────────
    CreateActor {
        ty: String,
        label: String,
        position: [f32; 2],
        props: Vec<animatix_syntax::ast::Property>,
    },
    RenameActor {
        old_label: String,
        new_label: String,
    },
    DuplicateActor(String),
    DuplicateSelectedActors,
    DeleteSelectedActors,
    ReparentActor {
        actor: String,
        new_parent: Option<String>,
    },
    ExtractScene {
        actor_labels: Vec<String>,
        new_scene_name: String,
    },
    MoveToScene {
        actor_labels: Vec<String>,
        target_scene: String,
    },
    ToggleActorVisibility(String),
    ToggleActorLock(String),

    // ── Property / Inspector ──────────────────────────────────────────
    PropertyEdit(PropertyEdit),
    /// Change a document-level config property (for example `colorscheme`).
    SetConfigProperty {
        key: String,
        value: animatix_syntax::ast::Expr,
    },
    /// Detach a targeted callout: bake current from/to/label_at as manual props and remove
    /// `target`.
    DetachCallout {
        actor: String,
        from: [f32; 2],
        to: [f32; 2],
        label_at: [f32; 2],
    },

    // ── Keyframe ──────────────────────────────────────────────────────
    SetKeyframeEasing {
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    DeleteKeyframe {
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
    },
    /// Set the canonical keyframe multi-selection in the UI store.
    SetSelectedKeyframes(Vec<KeyframeId>),
    /// Move a keyframe to a new time. Emitted by timeline drag.
    MoveKeyframe {
        scene: Option<String>,
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
    /// Replace a keyframe's value in place, without moving or merging it.
    /// Emitted by the Curves panel's vertical keyframe drag.
    SetKeyframeValue {
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
        value: PropertyValue,
    },
    /// Move several keyframes as one undoable step. Emitted by a multi-keyframe
    /// timeline drag so the whole selection shares one undo entry.
    MoveKeyframes(Vec<MoveKeyframeSpec>),
    /// Delete several keyframes as one undoable step.
    DeleteKeyframes(Vec<KeyframeId>),
    /// Resize an action block's duration. Emitted by timeline drag handles.
    ResizeAction {
        verb: String,
        targets: Vec<String>,
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    },

    // ── Editor sync modes ─────────────────────────────────────────────
    ToggleEditorSync,
    EditorChanged,

    // ── Clipboard ─────────────────────────────────────────────────────
    PasteActors,

    // ── Undo / Redo ───────────────────────────────────────────────────
    Undo,
    Redo,

    // ── Navigation ────────────────────────────────────────────────────
    ScrollToLine(usize, usize),

    // ── Viewport ──────────────────────────────────────────────────────
    ZoomToSelection,
    ZoomToAll,

    // ── Alignment ─────────────────────────────────────────────────────
    AlignActors(Align),
    DistributeActors(Axis),

    // ── Group / Ungroup ───────────────────────────────────────────────
    GroupSelectedActors,
    UngroupSelectedActors,

    // ── Insertion Palette ────────────────────────────────────────────
    InsertionFromPalette,

    // ── Find / Replace ────────────────────────────────────────────────
    FindReplaceAll,
}

/// One keyframe move inside [`Command::MoveKeyframes`].
///
/// Times are scene-local seconds, matching [`Command::MoveKeyframe`].
#[derive(Debug, Clone, PartialEq)]
pub struct MoveKeyframeSpec {
    pub scene: Option<String>,
    pub actor: String,
    pub property: String,
    pub old_time_s: f64,
    pub new_time_s: f64,
}

/// Horizontal or vertical axis for distribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Alignment type for multi-select alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Bottom,
    Middle,
}

// ── View actions ────────────────────────────────────────────────────────

/// View actions that change UI visibility or panel state without affecting
/// the document. These are not undoable.
#[derive(Debug, Clone)]
pub enum ViewAction {
    ShowInspector,
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
    SaveAsDialog,
    OpenSettings,
    OpenShortcuts,
    OpenInsertionPalette,
    OpenExportDialog,
    OpenCommandPalette,
    OpenPluginStatus,
    OpenFindReplace,
    DeselectActors,
}

// ── Drag events ─────────────────────────────────────────────────────────

/// Drag lifecycle events that bookend inspector or canvas drag interactions.
/// These are used for snapshot guards and cursor state, not for document
/// mutations directly.
#[derive(Debug, Clone)]
pub enum DragEvent {
    DragEnded,
    InspectorInputDragStarted,
    InspectorInputDragEnded,
}

// ── Unified action wrapper ──────────────────────────────────────────────

/// The unified action type consumed by the shell dispatcher each frame.
///
/// `Command` variants are domain-level mutations that can be undone.
/// `ViewAction` variants affect UI visibility only.
/// `DragEvent` variants are interaction bookkeeping.
#[derive(Debug, Clone)]
pub enum ShellAction {
    Command(Command),
    View(ViewAction),
    Drag(DragEvent),
}

// ── Property edit types ─────────────────────────────────────────────────

/// Describes a property edit made in the inspector panel.
pub type PropertyValue = animatix::timeline::PropertyValue;

mod api;
mod property_edit;
mod queue;
#[cfg(test)]
mod tests;

pub use property_edit::PropertyEdit;
pub(crate) use property_edit::property_value_to_expr;
pub use queue::{ActionQueue, CommandQueue, CommandSender, UndoEntry};
