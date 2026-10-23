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
#[allow(dead_code)] // Effect enum reserved for future side-effect dispatch
pub mod actor;
pub mod document;
pub mod keyframe;
pub mod playback;
pub mod scene;
pub mod view;

pub use actor::ActorCommand;
pub use document::DocumentCommand;
pub use keyframe::KeyframeCommand;
pub use playback::PlaybackCommand;
pub use scene::SceneCommand;
pub use view::ViewCommand;

/// Label for undo/redo stack entries. A subset of `Command` variants that
/// actually mutate the document and get stored in undo history.
/// The `label` is diagnostic only — undo/redo restores source text directly.
#[derive(Debug, Clone)]
pub enum UndoLabel {
    // Document
    FindReplaceAll,
    InsertionFromPalette,
    SetConfigProperty,
    PropertyEdit(PropertyEdit),
    /// Undo label for callout detach; constructed when undo integration is wired up.
    #[allow(dead_code)] // Undo wiring for DetachCallout is planned but not yet connected.
    DetachCallout {
        actor: String,
    },

    // Actor
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
    DeleteSelectedActors,
    PasteActors,
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
    AlignActors(Align),
    DistributeActors(Axis),
    GroupSelectedActors,
    UngroupSelectedActors,

    // Keyframe
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
    MoveKeyframe {
        scene: Option<String>,
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
    SetKeyframeValue {
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
    },
    /// Undo label for a batched multi-keyframe drag.
    MoveKeyframes,
    /// Undo label for a batched multi-keyframe delete.
    DeleteKeyframes,
    ResizeAction {
        verb: String,
        targets: Vec<String>,
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    },

    // Scene
    ReorderScenes,
    AddScene(String),
    DuplicateScene(String),
    DeleteScene(String),
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
}

impl From<UndoLabel> for Command {
    fn from(l: UndoLabel) -> Self {
        match l {
            UndoLabel::FindReplaceAll => Command::FindReplaceAll,
            UndoLabel::InsertionFromPalette => Command::InsertionFromPalette,
            UndoLabel::SetConfigProperty => Command::InsertionFromPalette,
            UndoLabel::PropertyEdit(e) => Command::PropertyEdit(e),
            UndoLabel::DetachCallout { actor } => Command::DetachCallout {
                actor,
                from: [0.0; 2],
                to: [0.0; 2],
                label_at: [0.0; 2],
            },
            UndoLabel::CreateActor {
                ty,
                label,
                position,
                props,
            } => Command::CreateActor {
                ty,
                label,
                position,
                props,
            },
            UndoLabel::RenameActor {
                old_label,
                new_label,
            } => Command::RenameActor {
                old_label,
                new_label,
            },
            UndoLabel::DuplicateActor(s) => Command::DuplicateActor(s),
            UndoLabel::DeleteSelectedActors => Command::DeleteSelectedActors,
            UndoLabel::PasteActors => Command::PasteActors,
            UndoLabel::ReparentActor { actor, new_parent } => {
                Command::ReparentActor { actor, new_parent }
            },
            UndoLabel::ExtractScene {
                actor_labels,
                new_scene_name,
            } => Command::ExtractScene {
                actor_labels,
                new_scene_name,
            },
            UndoLabel::MoveToScene {
                actor_labels,
                target_scene,
            } => Command::MoveToScene {
                actor_labels,
                target_scene,
            },
            UndoLabel::AlignActors(a) => Command::AlignActors(a),
            UndoLabel::DistributeActors(a) => Command::DistributeActors(a),
            UndoLabel::GroupSelectedActors => Command::GroupSelectedActors,
            UndoLabel::UngroupSelectedActors => Command::UngroupSelectedActors,
            UndoLabel::SetKeyframeEasing {
                scene,
                actor,
                property,
                time_s,
                easing,
            } => Command::SetKeyframeEasing {
                scene,
                actor,
                property,
                time_s,
                easing,
            },
            UndoLabel::DeleteKeyframe {
                scene,
                actor,
                property,
                time_s,
            } => Command::DeleteKeyframe {
                scene,
                actor,
                property,
                time_s,
            },
            UndoLabel::MoveKeyframe {
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            } => Command::MoveKeyframe {
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            },
            // The value payload is not needed to replay undo (source text is
            // restored directly), so the label carries only identity/time.
            UndoLabel::SetKeyframeValue {
                scene,
                actor,
                property,
                time_s,
            } => Command::SetKeyframeValue {
                scene,
                actor,
                property,
                time_s,
                value: PropertyValue::F32(0.0),
            },
            // Batch variants carry no payload in the undo label; undo restores
            // source text directly rather than replaying the command.
            UndoLabel::MoveKeyframes => Command::MoveKeyframes(Vec::new()),
            UndoLabel::DeleteKeyframes => Command::DeleteKeyframes(Vec::new()),
            UndoLabel::ResizeAction {
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            } => Command::ResizeAction {
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            },
            UndoLabel::AddScene(s) => Command::AddScene(s),
            UndoLabel::DuplicateScene(s) => Command::DuplicateScene(s),
            UndoLabel::DeleteScene(s) => Command::DeleteScene(s),
            UndoLabel::ReorderScenes => Command::ReorderScenes(Vec::new()),
            UndoLabel::SetTransition {
                from_scene,
                transition,
            } => Command::SetTransition {
                from_scene,
                transition,
            },
            UndoLabel::SetPlayTarget { from_scene, target } => {
                Command::SetPlayTarget { from_scene, target }
            },
            UndoLabel::SetSceneDuration { scene, duration_s } => {
                Command::SetSceneDuration { scene, duration_s }
            },
        }
    }
}

/// Effect enum reserved for future side-effect dispatch.
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
    /// Mark that a rebuild has been scheduled (status update already set).
    RebuildScheduled,
}

// =========================================================================
// ── Domain commands ─────────────────────────────────────────────────────
// =========================================================================

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

// =========================================================================
// ── View actions ────────────────────────────────────────────────────────
// =========================================================================

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

// =========================================================================
// ── Drag events ─────────────────────────────────────────────────────────
// =========================================================================

/// Drag lifecycle events that bookend inspector or canvas drag interactions.
/// These are used for snapshot guards and cursor state, not for document
/// mutations directly.
#[derive(Debug, Clone)]
pub enum DragEvent {
    DragEnded,
    InspectorInputDragStarted,
    InspectorInputDragEnded,
}

// =========================================================================
// ── Unified action wrapper ──────────────────────────────────────────────
// =========================================================================

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

impl From<DocumentCommand> for ShellAction {
    fn from(c: DocumentCommand) -> Self {
        ShellAction::Command(c.into())
    }
}
impl From<ActorCommand> for ShellAction {
    fn from(c: ActorCommand) -> Self {
        ShellAction::Command(c.into())
    }
}
impl From<KeyframeCommand> for ShellAction {
    fn from(c: KeyframeCommand) -> Self {
        ShellAction::Command(c.into())
    }
}
impl From<SceneCommand> for ShellAction {
    fn from(c: SceneCommand) -> Self {
        ShellAction::Command(c.into())
    }
}
impl From<PlaybackCommand> for ShellAction {
    fn from(c: PlaybackCommand) -> Self {
        ShellAction::Command(c.into())
    }
}
impl From<ViewCommand> for ShellAction {
    fn from(c: ViewCommand) -> Self {
        ShellAction::Command(c.into())
    }
}
impl From<Command> for ShellAction {
    fn from(c: Command) -> Self {
        ShellAction::Command(c)
    }
}

// =========================================================================
// ── Property edit types ─────────────────────────────────────────────────
// =========================================================================

/// Describes a property edit made in the inspector panel.
pub type PropertyValue = animatix::timeline::PropertyValue;

mod property_edit;
mod queue;
#[cfg(test)]
mod tests;

pub use property_edit::PropertyEdit;
pub(crate) use property_edit::property_value_to_expr;
pub use queue::{ActionQueue, CommandQueue, CommandSender, UndoEntry};
