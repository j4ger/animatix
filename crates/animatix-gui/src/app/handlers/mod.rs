//! Standalone command handler functions.
//!
//! Each command in the `Command` enum has a corresponding free function that
//! takes only the stores it needs.  This makes handlers testable without a
//! full `GuiShell` (no temp dirs, no filesystem) and keeps the dispatch logic
//! thin.
//!
//! # Conventions
//!
//! - **Naming:** `handle_<noun>_<verb>` (e.g. `handle_toggle_playback`).
//! - **Returns:** `Vec<Effect>` — side effects that the shell must execute (repaint, status
//!   messages, toast notifications).
//! - **Stores:** Take only the stores the handler actually mutates. Read-only stores are passed by
//!   shared reference (`&`).
//! - **No direct UI:** Handlers must not call `egui` APIs.  UI code lives in `shell/` or `panels/`.
//! - **Undo:** Domain-mutating handlers must call `snapshot()` *before* mutating state.  The
//!   dispatcher in `shell/mod.rs` handles this for commands that need it.

pub mod actor;
pub mod file;
pub mod keyframe;
pub mod playback;
pub mod property;
pub mod scene;
pub mod ui;
