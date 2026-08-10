use std::collections::VecDeque;
use std::path::PathBuf;

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
        actor: String,
        property: String,
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    DeleteKeyframe {
        actor: String,
        property: String,
        time_s: f64,
    },
    MoveKeyframe {
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
    ResizeAction {
        verb: String,
        targets: Vec<String>,
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    },

    // Scene
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
                actor,
                property,
                time_s,
                easing,
            } => Command::SetKeyframeEasing {
                actor,
                property,
                time_s,
                easing,
            },
            UndoLabel::DeleteKeyframe {
                actor,
                property,
                time_s,
            } => Command::DeleteKeyframe {
                actor,
                property,
                time_s,
            },
            UndoLabel::MoveKeyframe {
                actor,
                property,
                old_time_s,
                new_time_s,
            } => Command::MoveKeyframe {
                actor,
                property,
                old_time_s,
                new_time_s,
            },
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
            UndoLabel::DuplicateScene(s) => Command::DuplicateScene(s),
            UndoLabel::DeleteScene(s) => Command::DeleteScene(s),
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
        actor: String,
        property: String,
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    DeleteKeyframe {
        actor: String,
        property: String,
        time_s: f64,
    },
    /// Set the canonical keyframe multi-selection in the UI store.
    SetSelectedKeyframes(Vec<(String, String, u64)>),
    /// Move a keyframe to a new time. Emitted by timeline drag.
    MoveKeyframe {
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
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
    OpenExportDialog,
    OpenCommandPalette,
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

// =========================================================================
// ── Property edit types ─────────────────────────────────────────────────
// =========================================================================

/// Describes a property edit made in the inspector panel.
#[derive(Debug, Clone)]
pub struct PropertyEdit {
    pub actor: String,
    pub property: String,
    pub value: PropertyValue,
    /// When true, create a keyframe at current time instead of overwriting defaults.
    pub create_keyframe: bool,
    /// Optional explicit time override (seconds). When set, the edit applies at
    /// this time instead of the current playback time. Used by motion path editing.
    pub time_s: Option<f64>,
}

/// The typed value of a property edit.
///
/// This is the same schema-driven value model used by the core timeline so GUI
/// edits, inspector display, and engine evaluation do not need a parallel enum.
pub type PropertyValue = animatix::timeline::PropertyValue;

pub(crate) fn property_value_to_expr(
    pv: PropertyValue,
) -> Result<animatix_syntax::ast::Expr, String> {
    let expr = match &pv {
        PropertyValue::Vec2([x, y]) => animatix_syntax::ast::Expr::Tuple(vec![
            animatix_syntax::ast::Expr::Num(*x as f64),
            animatix_syntax::ast::Expr::Num(*y as f64),
        ]),
        PropertyValue::F32(v) => animatix_syntax::ast::Expr::Num(*v as f64),
        PropertyValue::Color([r, g, b, a]) | PropertyValue::Vec4([r, g, b, a]) => {
            if (*a - 1.0).abs() < 0.001 && r.fract() == 0.0 && g.fract() == 0.0 && b.fract() == 0.0
            {
                animatix_syntax::ast::Expr::Call(
                    "rgb".into(),
                    vec![
                        animatix_syntax::ast::Expr::Num((*r * 255.0) as i64 as f64),
                        animatix_syntax::ast::Expr::Num((*g * 255.0) as i64 as f64),
                        animatix_syntax::ast::Expr::Num((*b * 255.0) as i64 as f64),
                    ],
                )
            } else {
                animatix_syntax::ast::Expr::Call(
                    "rgba".into(),
                    vec![
                        animatix_syntax::ast::Expr::Num(*r as f64),
                        animatix_syntax::ast::Expr::Num(*g as f64),
                        animatix_syntax::ast::Expr::Num(*b as f64),
                        animatix_syntax::ast::Expr::Num(*a as f64),
                    ],
                )
            }
        },
        PropertyValue::Bool(b) => animatix_syntax::ast::Expr::Bool(*b),
        PropertyValue::String(s) => animatix_syntax::ast::Expr::Str(s.clone()),
        PropertyValue::Enum(s) => animatix_syntax::ast::Expr::Str(s.clone()),
        PropertyValue::StringList(items) => animatix_syntax::ast::Expr::List(
            items.iter().map(|s| animatix_syntax::ast::Expr::Str(s.clone())).collect(),
        ),
        PropertyValue::PointList(points) => animatix_syntax::ast::Expr::List(
            points
                .iter()
                .map(|&[x, y]| {
                    animatix_syntax::ast::Expr::Tuple(vec![
                        animatix_syntax::ast::Expr::Num(x as f64),
                        animatix_syntax::ast::Expr::Num(y as f64),
                    ])
                })
                .collect(),
        ),
        other => return Err(format!("cannot serialize property value {other:?}")),
    };
    crate::validation::validate_roundtrip(&expr, &pv)?;
    Ok(expr)
}

// =========================================================================
// ── Undo / redo ─────────────────────────────────────────────────────────
// =========================================================================

use crate::app::document::history::UiSnapshot;

/// An entry on the undo stack. Stores the command, the source text
/// before/after the command was applied, and UI state snapshots.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub command: UndoLabel,
    pub source_before: String,
    pub source_after: String,
    pub ui_before: UiSnapshot,
    pub ui_after: UiSnapshot,
}

/// Per-frame action queue consumed by the shell.
pub type ActionQueue = VecDeque<ShellAction>;
