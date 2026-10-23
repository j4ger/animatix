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

/// Whether a given window width should use the compact layout.
pub(super) fn compact_for_width(width: f32) -> bool {
    width > 0.0 && width < COMPACT_BREAKPOINT
}

/// Capture the docked pane visibility that compact mode must restore later.
pub(super) fn capture_compact_restore(tree: &Tree<WorkspaceTab>) -> CompactRestore {
    CompactRestore {
        sidebar_visible: pane_visible(tree, WorkspaceTab::Sidebar),
        detail_visible: detail_visible(tree),
    }
}

/// Reconcile the dock tree with the desired compact flag.
///
/// On entry the detail dock is hidden (the overlay drawer replaces it; the
/// sidebar tile stays visible and renders its icon rail). On exit the captured
/// visibility is restored, so a user who had closed the detail column, or was
/// in the Focus preset, does not get panes forced back open. The caller stores
/// `restore` across frames (`ViewStore::compact_restore`).
pub(super) fn reconcile_compact(
    tree: &mut Tree<WorkspaceTab>,
    was_compact: bool,
    compact: bool,
    restore: &mut Option<CompactRestore>,
) -> CompactTransition {
    if was_compact == compact {
        return CompactTransition::Unchanged;
    }
    if compact {
        *restore = Some(capture_compact_restore(tree));
        apply_compact_entry(tree);
        CompactTransition::Entered
    } else {
        let state = restore.take().unwrap_or(CompactRestore {
            sidebar_visible: true,
            detail_visible: true,
        });
        restore_compact_tree(tree, state);
        CompactTransition::Exited
    }
}

/// Force the dock into its compact shape: sidebar visible (as the icon rail)
/// and the detail group hidden (it renders as an overlay drawer).
///
/// Idempotent, so a tree rebuilt while compact (preset apply, reset, migration)
/// can be re-shaped without waiting for a transition.
pub(super) fn apply_compact_entry(tree: &mut Tree<WorkspaceTab>) {
    // The rail is the only way to reach the sidebar views in compact mode, so
    // it stays visible even if the user had hidden the sidebar (e.g. the Focus
    // preset). The captured state restores on widen.
    set_pane_visible(tree, WorkspaceTab::Sidebar, true);
    set_detail_visible(tree, false);
}

/// Apply a captured pane-visibility pair to the dock tree.
pub(super) fn restore_compact_tree(tree: &mut Tree<WorkspaceTab>, state: CompactRestore) {
    set_pane_visible(tree, WorkspaceTab::Sidebar, state.sidebar_visible);
    set_detail_visible(tree, state.detail_visible);
}

/// Width of the compact sidebar rail as an actual allocation.
///
/// Kept below the sidebar's pixel floor on purpose: shares drive layout, and
/// `Behavior::min_size` only constrains interactive resizing, so the rail can be
/// narrower than `LEFT_MIN`.
pub(super) fn enforce_compact_rail(tree: &mut Tree<WorkspaceTab>, width: f32) {
    if width <= 0.0 {
        return;
    }
    let (Some(sidebar), Some(preview)) = (
        tree.tiles.find_pane(&WorkspaceTab::Sidebar),
        tree.tiles.find_pane(&WorkspaceTab::Preview),
    ) else {
        return;
    };
    let Some(top_row) = tree.tiles.parent_of(sidebar) else {
        return;
    };
    if !(tree.is_visible(sidebar) && tree.is_visible(preview)) {
        return;
    }
    let rail = crate::app::panels::COMPACT_RAIL_WIDTH.min((width * 0.25).max(0.0));
    if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(top_row) {
        linear.shares.set_share(sidebar, rail);
        linear.shares.set_share(preview, (width - rail).max(0.0));
    }
}

/// Per-frame layout pass for compact mode: fixed rail on the left plus the
/// vertical (bottom-region) bounds, which still apply at any width.
pub(super) fn enforce_compact_layout(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    enforce_compact_rail(tree, width);
    enforce_vertical_bounds(tree, preset, height);
}

/// Whether a single pane tile (not its tab group) is currently visible.
pub(super) fn pane_visible(tree: &Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    tree.tiles.find_pane(&tab).is_some_and(|id| tree.is_visible(id))
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

pub(super) fn app_state_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("app_state.ron");
    }
    PathBuf::from(".animatix-app-state.ron")
}

pub(super) fn load_app_state() -> Option<PathBuf> {
    let path = app_state_path();
    let content = fs::read_to_string(&path).ok()?;
    let state: AppState = ron::from_str(&content).ok()?;
    state.recent_file
}

/// Load persisted autosave preferences, falling back to defaults when absent.
pub(super) fn load_autosave_prefs() -> AutosavePrefs {
    let Some(content) = fs::read_to_string(app_state_path()).ok() else {
        return AutosavePrefs::default();
    };
    ron::from_str::<AppState>(&content)
        .map(|state| state.autosave)
        .unwrap_or_default()
}

/// Merge `autosave` preferences into the existing app state, preserving the
/// recent-file fields that `save_app_state` owns.
pub(super) fn save_autosave_prefs(autosave: AutosavePrefs) {
    let path = app_state_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("Failed to create persistence directory {}: {}", parent.display(), e);
        }
    }
    let mut state = fs::read_to_string(&path)
        .ok()
        .and_then(|content| ron::from_str::<AppState>(&content).ok())
        .unwrap_or_default();
    state.autosave = autosave;

    match ron::ser::to_string_pretty(&state, ron::ser::PrettyConfig::default()) {
        Ok(serialized) => {
            if let Err(e) = fs::write(&path, serialized) {
                tracing::warn!("Failed to write app state file {}: {}", path.display(), e);
            }
        },
        Err(e) => tracing::warn!("Failed to serialize app state: {}", e),
    }
}

/// Most-recently opened files, newest first.
pub(super) fn load_recent_files() -> Vec<PathBuf> {
    let Some(content) = fs::read_to_string(app_state_path()).ok() else {
        return Vec::new();
    };
    let state: AppState = ron::from_str(&content).unwrap_or_default();
    if state.recent_files.is_empty() {
        // Migrate a pre-list install that only stored a single recent file.
        state.recent_file.into_iter().collect()
    } else {
        state.recent_files
    }
}

pub(super) fn save_app_state(recent_file: &Path) {
    let path = app_state_path();
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            tracing::warn!("Failed to create persistence directory {}: {}", parent.display(), e);
        }
    }
    let mut state = fs::read_to_string(&path)
        .ok()
        .and_then(|content| ron::from_str::<AppState>(&content).ok())
        .unwrap_or_default();
    state.recent_file = Some(recent_file.to_path_buf());
    state.recent_files.retain(|existing| existing != recent_file);
    state.recent_files.insert(0, recent_file.to_path_buf());
    state.recent_files.truncate(MAX_RECENT_FILES);

    if let Ok(serialized) = ron::ser::to_string_pretty(&state, ron::ser::PrettyConfig::default()) {
        if let Err(e) = fs::write(&path, serialized) {
            tracing::warn!("Failed to write app state file {}: {}", path.display(), e);
        }
    }
}

pub(super) fn clear_app_state() {
    let path = app_state_path();
    if path.exists() {
        if let Err(e) = fs::remove_file(&path) {
            tracing::warn!("Failed to remove app state file {}: {}", path.display(), e);
        }
    }
}

mod layout;
#[cfg(test)]
mod tests;

pub(crate) use layout::*;
