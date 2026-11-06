use std::collections::BTreeMap;
use std::path::PathBuf;

use egui_tiles::{Container, Linear, LinearDir, Tile, TileId, Tiles, Tree};

use super::*;
use crate::app::interaction::keyboard::SavedShortcut;
use serde::{Deserialize, Serialize};

/// Region proportions for the responsive default layout.
///
/// Every region is allocated as `clamp(ratio * available, min, max)`: a
/// proportion by default, with pixel bounds as the floor/ceiling so extreme
/// window sizes never produce unusable panes. `Behavior::min_size` is the final
/// per-tile floor. Shares are relative, so the proportions hold as the window
/// is resized.
mod metrics {
    pub const LEFT_RATIO: f32 = 0.16;
    pub const LEFT_MIN: f32 = 200.0;
    pub const LEFT_MAX: f32 = 360.0;

    pub const DETAIL_INSPECTOR_RATIO: f32 = 0.21;
    pub const DETAIL_INSPECTOR_MIN: f32 = 260.0;
    pub const DETAIL_INSPECTOR_MAX: f32 = 420.0;

    pub const DETAIL_CODE_RATIO: f32 = 0.38;
    pub const DETAIL_CODE_MIN: f32 = 420.0;
    pub const DETAIL_CODE_MAX: f32 = 720.0;

    pub const PREVIEW_MIN: f32 = 360.0;

    pub const BOTTOM_RATIO: f32 = 0.25;
    pub const BOTTOM_MIN: f32 = 180.0;
    pub const BOTTOM_MAX: f32 = 420.0;
}

/// Vertical chrome outside the dock area: toolbar + status bar.
const VERTICAL_CHROME: f32 = 28.0 + 22.0;

/// Narrowest the sidebar may be squeezed before the preview starts giving ground.
const RAIL_MIN: f32 = 48.0;

/// Window width (logical px) below which the workspace downgrades to the compact
/// layout: a sidebar icon rail instead of the tab bar + content, and the detail
/// region promoted to an overlay drawer instead of a docked column.
pub(super) const COMPACT_BREAKPOINT: f32 = 1000.0;

/// Docked pane visibility captured when compact mode engages, restored when the
/// window widens again. Preserving it (rather than forcing panes visible) keeps
/// explicit user choices — a closed detail column, a Focus layout — intact
/// across a resize round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompactRestore {
    pub sidebar_visible: bool,
    pub detail_visible: bool,
}

/// Outcome of reconciling the dock tree with the desired compact flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompactTransition {
    /// The flag did not change; the tree was left untouched.
    Unchanged,
    /// Entered compact mode: the detail dock was hidden.
    Entered,
    /// Left compact mode: the captured pane visibility was restored.
    Exited,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AppState {
    #[serde(default)]
    recent_file: Option<PathBuf>,
    #[serde(default)]
    recent_files: Vec<PathBuf>,
    /// Crash-recovery autosave preferences. Absent in pre-autosave files.
    #[serde(default)]
    autosave: AutosavePrefs,
}

/// Cap on the recent-files list.
const MAX_RECENT_FILES: usize = 12;

mod impls;
mod layout;

pub(super) use impls::*;
#[cfg(test)]
mod tests;

pub(crate) use layout::*;
