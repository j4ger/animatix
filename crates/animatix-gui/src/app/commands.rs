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

/// A unified command enum that replaces the 40+ `Option<T>` fields in `UiActions`.
/// Every user intent is expressed as a `Command` and pushed into a `VecDeque<Command>`
/// for ordered, frame-batched processing.
#[derive(Debug, Clone)]
pub enum Command {
    // ── Document / File ───────────────────────────────────────────────
    OpenFile(PathBuf),
    Save,
    Reload,
    Rebuild,

    // ── Workspace / Explorer ──────────────────────────────────────────
    ToggleExpandDir(PathBuf),

    // ── UI / Panels ───────────────────────────────────────────────────
    ShowInspector,
    ToggleDiagnosticsPanel,
    OpenExportDialog,
    ScrollToLine(usize, usize),

    // ── Playback ──────────────────────────────────────────────────────
    TogglePlayback,
    ScrubTo(f64),
    PrevKeyframe,
    NextKeyframe,

    // ── Scene ─────────────────────────────────────────────────────────
    SelectScene(String),
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    PrevScene,
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    NextScene,
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    AddScene,
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    DeleteScene(String),
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    RenameScene { old_name: String, new_name: String },
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    ReorderScenes(Vec<String>),
    SetTransition { from_scene: String, transition: animatix::ast::Transition },
    SetPlayTarget { from_scene: String, target: Option<String> },

    // ── Actor ─────────────────────────────────────────────────────────
    CreateActor { ty: String, label: String, position: [f32; 2] },
    RenameActor { old_label: String, new_label: String },
    DuplicateActor(String),
    DeleteSelectedActors,
    ReparentActor { actor: String, new_parent: Option<String> },
    ExtractScene { actor_labels: Vec<String>, new_scene_name: String },
    MoveToScene { actor_labels: Vec<String>, target_scene: String },

    // ── Property / Inspector ──────────────────────────────────────────
    PropertyEdit(PropertyEdit),

    // ── Keyframe ──────────────────────────────────────────────────────
    SetKeyframeEasing { actor: String, property: String, time_s: f64, easing: animatix::easing::Easing },
    DeleteKeyframe { actor: String, property: String, time_s: f64 },
    /// Move a keyframe to a new time. Emitted by timeline drag.
    MoveKeyframe { actor: String, property: String, old_time_s: f64, new_time_s: f64 },

    // ── Editor sync modes ─────────────────────────────────────────────
    ToggleEditorSync,
    #[allow(dead_code)] // Handled but not yet dispatched from GUI
    ToggleKeyframeMode,
    EditorChanged,

    // ── Drag / Interaction lifecycle ──────────────────────────────────
    DragEnded,
    InspectorInputDragStarted,
    InspectorInputDragEnded,

    // ── Clipboard ─────────────────────────────────────────────────────
    PasteActors,

    // ── Undo / Redo ───────────────────────────────────────────────────
    Undo,
    Redo,

    // ── Render ────────────────────────────────────────────────────────
    #[allow(dead_code)] // WIP: will be used when repaint-on-demand is implemented
    RequestRepaint,
}

/// Describes a property edit made in the inspector panel.
#[derive(Debug, Clone)]
pub struct PropertyEdit {
    pub actor: String,
    pub property: String,
    pub value: PropertyValue,
    /// When true, create a keyframe at current time instead of overwriting defaults.
    pub create_keyframe: bool,
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

impl From<PropertyValue> for animatix::ast::Expr {
    fn from(pv: PropertyValue) -> Self {
        match pv {
            PropertyValue::Vec2([x, y]) => {
                animatix::ast::Expr::Tuple(vec![
                    animatix::ast::Expr::Num(x as f64),
                    animatix::ast::Expr::Num(y as f64),
                ])
            }
            PropertyValue::Float(v) => animatix::ast::Expr::Num(v as f64),
            PropertyValue::Color([r, g, b, a]) => {
                if (a - 1.0).abs() < 0.001
                    && r.fract() == 0.0
                    && g.fract() == 0.0
                    && b.fract() == 0.0
                {
                    animatix::ast::Expr::Call(
                        "rgb".into(),
                        vec![
                            animatix::ast::Expr::Num((r * 255.0) as i64 as f64),
                            animatix::ast::Expr::Num((g * 255.0) as i64 as f64),
                            animatix::ast::Expr::Num((b * 255.0) as i64 as f64),
                        ],
                    )
                } else {
                    animatix::ast::Expr::Call(
                        "rgba".into(),
                        vec![
                            animatix::ast::Expr::Num(r as f64),
                            animatix::ast::Expr::Num(g as f64),
                            animatix::ast::Expr::Num(b as f64),
                            animatix::ast::Expr::Num(a as f64),
                        ],
                    )
                }
            }
            PropertyValue::Text(s) => animatix::ast::Expr::Str(s),
            PropertyValue::StringList(items) => {
                animatix::ast::Expr::Tuple(items.into_iter().map(animatix::ast::Expr::Ident).collect())
            }
            PropertyValue::PointList(points) => {
                animatix::ast::Expr::Tuple(points.into_iter().map(|[x, y]| {
                    animatix::ast::Expr::Tuple(vec![
                        animatix::ast::Expr::Num(x as f64),
                        animatix::ast::Expr::Num(y as f64),
                    ])
                }).collect())
            }
        }
    }
}

/// An entry on the undo stack. Stores the command *and* the source text
/// before the command was applied, so that undo can restore the exact state.
/// This is a pragmatic stepping stone toward fully semantic undo.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub command: Command,
    pub source_before: String,
}

/// Per-frame command queue consumed by the shell.
pub type CommandQueue = VecDeque<Command>;
