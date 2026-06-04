use std::collections::VecDeque;
use std::path::PathBuf;

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

    // ── Scene ─────────────────────────────────────────────────────────
    SelectScene(String),
    ReorderScenes(Vec<String>),
    SetTransition { from_scene: String, transition: animatix_syntax::ast::Transition },
    SetPlayTarget { from_scene: String, target: Option<String> },
    DuplicateScene(String),
    DeleteScene(String),

    // ── Actor ─────────────────────────────────────────────────────────
    CreateActor { ty: String, label: String, position: [f32; 2], props: Vec<animatix_syntax::ast::Property> },
    RenameActor { old_label: String, new_label: String },
    DuplicateActor(String),
    DeleteSelectedActors,
    ReparentActor { actor: String, new_parent: Option<String> },
    ExtractScene { actor_labels: Vec<String>, new_scene_name: String },
    MoveToScene { actor_labels: Vec<String>, target_scene: String },
    ToggleActorVisibility(String),
    ToggleActorLock(String),

    // ── Property / Inspector ──────────────────────────────────────────
    PropertyEdit(PropertyEdit),

    // ── Keyframe ──────────────────────────────────────────────────────
    SetKeyframeEasing { actor: String, property: String, time_s: f64, easing: animatix_syntax::easing::Easing },
    DeleteKeyframe { actor: String, property: String, time_s: f64 },
    /// Move a keyframe to a new time. Emitted by timeline drag.
    MoveKeyframe { actor: String, property: String, old_time_s: f64, new_time_s: f64 },

    // ── Editor sync modes ─────────────────────────────────────────────
    ToggleEditorSync,
    EditorChanged,

    // ── Clipboard ─────────────────────────────────────────────────────
    PasteActors,

    // ── Modules ───────────────────────────────────────────────────────
    ImportModule(String),

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
#[derive(Debug, Clone)]
pub enum PropertyValue {
    Vec2([f32; 2]),
    Float(f32),
    Color([f32; 4]),
    Text(String),
    StringList(Vec<String>),
    PointList(Vec<[f32; 2]>),
}

impl TryFrom<PropertyValue> for animatix_syntax::ast::Expr {
    type Error = String;
    fn try_from(pv: PropertyValue) -> Result<Self, Self::Error> {
        let expr = match &pv {
            PropertyValue::Vec2([x, y]) => {
                animatix_syntax::ast::Expr::Tuple(vec![
                    animatix_syntax::ast::Expr::Num(*x as f64),
                    animatix_syntax::ast::Expr::Num(*y as f64),
                ])
            }
            PropertyValue::Float(v) => animatix_syntax::ast::Expr::Num(*v as f64),
            PropertyValue::Color([r, g, b, a]) => {
                if (*a - 1.0).abs() < 0.001
                    && r.fract() == 0.0
                    && g.fract() == 0.0
                    && b.fract() == 0.0
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
            }
            PropertyValue::Text(s) => animatix_syntax::ast::Expr::Str(s.clone()),
            PropertyValue::StringList(items) => {
                animatix_syntax::ast::Expr::Tuple(items.iter().cloned().map(animatix_syntax::ast::Expr::Ident).collect())
            }
            PropertyValue::PointList(points) => {
                animatix_syntax::ast::Expr::Tuple(points.iter().map(|&[x, y]| {
                    animatix_syntax::ast::Expr::Tuple(vec![
                        animatix_syntax::ast::Expr::Num(x as f64),
                        animatix_syntax::ast::Expr::Num(y as f64),
                    ])
                }).collect())
            }
        };
        crate::validation::validate_roundtrip(&expr, &pv)?;
        Ok(expr)
    }
}

// =========================================================================
// ── Undo / redo ─────────────────────────────────────────────────────────
// =========================================================================

/// An entry on the undo stack. Stores the command *and* the source text
/// before the command was applied, so that undo can restore the exact state.
/// This is a pragmatic stepping stone toward fully semantic undo.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub command: Command,
    pub source_before: String,
}

/// Per-frame action queue consumed by the shell.
pub type ActionQueue = VecDeque<ShellAction>;
