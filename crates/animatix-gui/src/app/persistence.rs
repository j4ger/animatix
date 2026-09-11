use std::collections::BTreeMap;
use std::path::PathBuf;

use egui_tiles::{Container, Linear, LinearDir, Tile, TileId, Tiles, Tree};

use super::*;
use crate::app::interaction::keyboard::SavedShortcut;

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

fn clamp_ratio(ratio: f32, available: f32, min: f32, max: f32) -> f32 {
    (ratio * available).clamp(min, max)
}

/// Per-preset region preferences (ratios, pixel bounds, detail tab).
struct PresetRegions {
    left: (f32, f32, f32),
    detail: (f32, f32, f32),
    bottom: (f32, f32, f32),
    tab: WorkspaceTab,
    hide_surroundings: bool,
}

fn preset_regions(preset: LayoutPreset) -> PresetRegions {
    use metrics::*;
    match preset {
        LayoutPreset::Animate => PresetRegions {
            left: (LEFT_RATIO, LEFT_MIN, LEFT_MAX),
            detail: (DETAIL_INSPECTOR_RATIO, DETAIL_INSPECTOR_MIN, DETAIL_INSPECTOR_MAX),
            bottom: (BOTTOM_RATIO, BOTTOM_MIN, BOTTOM_MAX),
            tab: WorkspaceTab::Inspector,
            hide_surroundings: false,
        },
        LayoutPreset::Code => PresetRegions {
            left: (0.14, 180.0, 300.0),
            detail: (DETAIL_CODE_RATIO, DETAIL_CODE_MIN, DETAIL_CODE_MAX),
            bottom: (0.18, 160.0, 260.0),
            tab: WorkspaceTab::Code,
            hide_surroundings: false,
        },
        LayoutPreset::Inspect => PresetRegions {
            left: (0.12, 160.0, 260.0),
            detail: (0.30, 300.0, 460.0),
            bottom: (0.30, 220.0, 420.0),
            tab: WorkspaceTab::Inspector,
            hide_surroundings: false,
        },
        LayoutPreset::Focus => PresetRegions {
            left: (LEFT_RATIO, LEFT_MIN, LEFT_MAX),
            detail: (DETAIL_INSPECTOR_RATIO, DETAIL_INSPECTOR_MIN, DETAIL_INSPECTOR_MAX),
            bottom: (BOTTOM_RATIO, BOTTOM_MIN, BOTTOM_MAX),
            tab: WorkspaceTab::Inspector,
            hide_surroundings: true,
        },
    }
}

/// Build a workspace tree for a preset at a reference window size.
///
/// Layout:
/// ```text
/// ┌───────────┬───────────────────────────┬─────────────┐
/// │  Sidebar  │          Preview          │   Detail    │
/// │           │                           │ ┌─────────┐ │
/// │           │                           │ │Inspector│ │
/// │           │                           │ │  Code   │ │
/// │           │                           │ └─────────┘ │
/// ├───────────┴───────────────────────────┴─────────────┤
/// │            Timeline | Curves (full width)            │
/// └──────────────────────────────────────────────────────┘
/// ```
///
/// The right "detail" region is a tab group holding the Inspector and the code
/// editor, so only one of them is ever on screen. The bottom region is a tab
/// group holding the Timeline and the Curves editor, spanning the full width;
/// only one of them is ever on screen.
pub(super) fn build_tree_for(preset: LayoutPreset, width: f32, height: f32) -> Tree<WorkspaceTab> {
    let regions = preset_regions(preset);
    let mut tiles = Tiles::default();

    let sidebar = tiles.insert_pane(WorkspaceTab::Sidebar);
    let preview = tiles.insert_pane(WorkspaceTab::Preview);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);
    let code = tiles.insert_pane(WorkspaceTab::Code);
    let timeline = tiles.insert_pane(WorkspaceTab::Timeline);
    let curves = tiles.insert_pane(WorkspaceTab::Curves);

    // Inspector and Code share one tab group: mutually exclusive detail views.
    let detail = tiles.insert_tab_tile(vec![inspector, code]);
    // Timeline and Curves share the bottom tab group: mutually exclusive.
    let bottom = tiles.insert_tab_tile(vec![timeline, curves]);

    let left_px = clamp_ratio(regions.left.0, width, regions.left.1, regions.left.2);
    let detail_px = clamp_ratio(regions.detail.0, width, regions.detail.1, regions.detail.2);
    let preview_px = (width - left_px - detail_px).max(metrics::PREVIEW_MIN);

    let mut top_row = Linear::new(LinearDir::Horizontal, vec![sidebar, preview, detail]);
    top_row.shares[sidebar] = left_px;
    top_row.shares[preview] = preview_px;
    top_row.shares[detail] = detail_px;
    let top_row = tiles.insert_container(top_row);

    let dock_h = (height - VERTICAL_CHROME).max(1.0);
    let bottom_px = clamp_ratio(regions.bottom.0, dock_h, regions.bottom.1, regions.bottom.2);
    let root = tiles.insert_container(Linear::new_binary(
        LinearDir::Vertical,
        [top_row, bottom],
        (1.0 - bottom_px / dock_h).clamp(0.1, 0.9),
    ));

    let mut tree = Tree::new("workspace", root, tiles);
    activate_detail_tab(&mut tree, regions.tab);
    activate_bottom_tab(&mut tree, WorkspaceTab::Timeline);
    if regions.hide_surroundings {
        set_pane_visible(&mut tree, WorkspaceTab::Sidebar, false);
        set_detail_visible(&mut tree, false);
        set_bottom_visible(&mut tree, false);
    }
    tree
}

/// Default workspace layout: Animate, sized to the initial window.
pub(super) fn default_tree() -> Tree<WorkspaceTab> {
    build_tree_for(
        LayoutPreset::Animate,
        INITIAL_WINDOW_SIZE.0 as f32,
        INITIAL_WINDOW_SIZE.1 as f32,
    )
}

/// Apply a preset's proportions and detail tab to an existing tree.
///
/// Returns `false` when the tree lacks the expected panes (e.g. a layout
/// persisted before the detail region existed); callers rebuild in that case.
pub(super) fn apply_layout_preset(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) -> bool {
    let Some(sidebar) = tree.tiles.find_pane(&WorkspaceTab::Sidebar) else {
        return false;
    };
    let Some(preview) = tree.tiles.find_pane(&WorkspaceTab::Preview) else {
        return false;
    };
    let Some(inspector) = tree.tiles.find_pane(&WorkspaceTab::Inspector) else {
        return false;
    };
    let Some(timeline) = tree.tiles.find_pane(&WorkspaceTab::Timeline) else {
        return false;
    };
    // A layout persisted before the Curves editor lacks the bottom tab group.
    // Returning `false` makes the caller rebuild from `build_tree_for`, which
    // is the migration path for pre-Curves layouts.
    let Some(_curves) = tree.tiles.find_pane(&WorkspaceTab::Curves) else {
        return false;
    };
    let Some(detail) = tree.tiles.parent_of(inspector) else {
        return false;
    };
    let Some(bottom) = tree.tiles.parent_of(timeline) else {
        return false;
    };
    let Some(top_row) = tree.tiles.parent_of(sidebar) else {
        return false;
    };

    let regions = preset_regions(preset);
    let left_px = clamp_ratio(regions.left.0, width, regions.left.1, regions.left.2);
    let detail_px = clamp_ratio(regions.detail.0, width, regions.detail.1, regions.detail.2);
    let preview_px = (width - left_px - detail_px).max(metrics::PREVIEW_MIN);
    if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(top_row) {
        linear.shares.set_share(sidebar, left_px);
        linear.shares.set_share(preview, preview_px);
        linear.shares.set_share(detail, detail_px);
    }

    let dock_h = (height - VERTICAL_CHROME).max(1.0);
    let bottom_px = clamp_ratio(regions.bottom.0, dock_h, regions.bottom.1, regions.bottom.2);
    let top_frac = (1.0 - bottom_px / dock_h).clamp(0.1, 0.9);
    if let Some(root) = tree.root {
        if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(root) {
            linear.shares.set_share(top_row, top_frac);
            linear.shares.set_share(bottom, 1.0 - top_frac);
        }
    }

    let visible = !regions.hide_surroundings;
    tree.set_visible(sidebar, visible);
    set_pane_visible(tree, WorkspaceTab::Timeline, true);
    set_pane_visible(tree, WorkspaceTab::Curves, true);
    set_pane_visible(tree, WorkspaceTab::Inspector, true);
    set_pane_visible(tree, WorkspaceTab::Code, true);
    set_detail_visible(tree, visible);
    set_bottom_visible(tree, visible);
    if visible {
        activate_detail_tab(tree, regions.tab);
        // Only pick a default when the group has no active tab (e.g. after
        // Focus hid it); a user's Timeline/Curves choice is preserved.
        if active_bottom_tab(tree).is_none() {
            activate_bottom_tab(tree, WorkspaceTab::Timeline);
        }
    }
    true
}

/// Enforce the preset's pixel bounds on the docked regions each frame.
///
/// Shares are relative, so a window smaller than the reference size used at
/// build time would scale every region down proportionally and drop them below
/// their pixel floors. This pass reads the current widths, clamps them into
/// `[min, max]`, and lets the preview absorb the difference — "proportion by
/// default, pixels as the floor/ceiling" applied continuously. Regions already
/// inside their bounds are left untouched, so user resizing is preserved.
pub(super) fn enforce_layout_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    enforce_horizontal_bounds(tree, preset, width);
    enforce_vertical_bounds(tree, preset, height);
}

/// Horizontal pass of [`enforce_layout_bounds`]: sidebar | preview (| detail).
///
/// Skipped in compact mode, where the sidebar is a fixed-width rail (see
/// [`enforce_compact_rail`]) and the detail column is hidden.
fn enforce_horizontal_bounds(tree: &mut Tree<WorkspaceTab>, preset: LayoutPreset, width: f32) {
    let regions = preset_regions(preset);

    let (Some(sidebar), Some(preview), Some(inspector)) = (
        tree.tiles.find_pane(&WorkspaceTab::Sidebar),
        tree.tiles.find_pane(&WorkspaceTab::Preview),
        tree.tiles.find_pane(&WorkspaceTab::Inspector),
    ) else {
        return;
    };
    let Some(detail) = tree.tiles.parent_of(inspector) else {
        return;
    };
    let Some(top_row) = tree.tiles.parent_of(sidebar) else {
        return;
    };

    if tree.is_visible(sidebar) && tree.is_visible(preview) {
        let detail_visible = tree.is_visible(detail);
        let mut kids = vec![sidebar, preview];
        if detail_visible {
            kids.push(detail);
        }
        if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(top_row) {
            let px = linear.shares.split(&kids, width);
            let mut left = px[0].clamp(regions.left.1, regions.left.2);
            let mut detail_px = if detail_visible {
                px[2].clamp(regions.detail.1, regions.detail.2)
            } else {
                0.0
            };
            let mut preview_px = (width - left - detail_px).max(0.0);
            // Reclaim in reverse priority when the window is too small: the
            // detail column yields first, then the sidebar shrinks toward an
            // icon-rail width, and only then does the preview give ground. This
            // keeps every region present instead of collapsing one to nothing.
            if preview_px < metrics::PREVIEW_MIN {
                let need = metrics::PREVIEW_MIN - preview_px;
                let take = detail_px.min(need);
                detail_px -= take;
                preview_px += take;
            }
            if preview_px < metrics::PREVIEW_MIN {
                let need = metrics::PREVIEW_MIN - preview_px;
                let take = (left - RAIL_MIN).max(0.0).min(need);
                left -= take;
                preview_px += take;
            }
            linear.shares.set_share(sidebar, left);
            linear.shares.set_share(preview, preview_px);
            if detail_visible {
                linear.shares.set_share(detail, detail_px);
            }
        }
    }
}

/// Vertical pass of [`enforce_layout_bounds`]: top row above the bottom tab
/// group (Timeline | Curves). Runs in every mode, including compact.
fn enforce_vertical_bounds(tree: &mut Tree<WorkspaceTab>, preset: LayoutPreset, height: f32) {
    let regions = preset_regions(preset);
    let (Some(sidebar), Some(timeline)) = (
        tree.tiles.find_pane(&WorkspaceTab::Sidebar),
        tree.tiles.find_pane(&WorkspaceTab::Timeline),
    ) else {
        return;
    };
    // The bottom region is a tab group; resize its container, not the pane, so
    // the Timeline and Curves tabs share one allocation. A layout persisted
    // before Curves has the Timeline directly under the root linear, so fall
    // back to the pane itself (its old behavior) until a preset rebuilds it.
    let Some(bottom_region) = bottom_region_child(tree, timeline) else {
        return;
    };
    let Some(top_row) = tree.tiles.parent_of(sidebar) else {
        return;
    };

    if tree.is_visible(bottom_region) {
        if let Some(root) = tree.root {
            let kids = vec![top_row, bottom_region];
            if let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(root) {
                let px = linear.shares.split(&kids, height);
                let bottom_px = px[1].clamp(regions.bottom.1, regions.bottom.2);
                linear.shares.set_share(top_row, (height - bottom_px).max(0.0));
                linear.shares.set_share(bottom_region, bottom_px);
            }
        }
    }
}

/// The vertical sibling that owns the bottom allocation.
///
/// Normally the Timeline/Curves tab group container. For a layout persisted
/// before Curves, the Timeline pane is the root's vertical child directly, so
/// return the pane and preserve the pre-Curves resize behavior.
fn bottom_region_child(tree: &Tree<WorkspaceTab>, timeline: TileId) -> Option<TileId> {
    let Some(curves) = tree.tiles.find_pane(&WorkspaceTab::Curves) else {
        return Some(timeline);
    };
    match (tree.tiles.parent_of(curves), tree.tiles.parent_of(timeline)) {
        (Some(curves_parent), Some(timeline_parent)) if curves_parent == timeline_parent => {
            Some(curves_parent)
        },
        _ => Some(timeline),
    }
}

/// Valid tab-group member lists, keyed by a probe pane used to locate the
/// group's container in the tree.
const DETAIL_TABS: [WorkspaceTab; 2] = [WorkspaceTab::Inspector, WorkspaceTab::Code];
const BOTTOM_TABS: [WorkspaceTab; 2] = [WorkspaceTab::Timeline, WorkspaceTab::Curves];

/// Show or hide the whole detail region (Inspector + Code tab group).
pub(super) fn set_detail_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    set_group_visible(tree, WorkspaceTab::Inspector, visible)
}

/// Show or hide the whole bottom region (Timeline + Curves tab group).
pub(super) fn set_bottom_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    set_group_visible(tree, WorkspaceTab::Timeline, visible)
}

/// Whether the detail region (Inspector + Code tab group) is visible.
///
/// Visibility lives on the group container, so this reports the whole region
/// rather than the pane's own (always-on) flag.
pub(super) fn detail_visible(tree: &Tree<WorkspaceTab>) -> bool {
    group_visible(tree, WorkspaceTab::Inspector)
}

/// Whether the bottom region (Timeline + Curves tab group) is visible.
pub(super) fn bottom_visible(tree: &Tree<WorkspaceTab>) -> bool {
    group_visible(tree, WorkspaceTab::Timeline)
}

fn group_visible(tree: &Tree<WorkspaceTab>, probe: WorkspaceTab) -> bool {
    tree.tiles
        .find_pane(&probe)
        .and_then(|pane| tree.tiles.parent_of(pane))
        .is_none_or(|container| tree.is_visible(container))
}

fn set_group_visible(tree: &mut Tree<WorkspaceTab>, probe: WorkspaceTab, visible: bool) -> bool {
    let Some(pane) = tree.tiles.find_pane(&probe) else {
        return false;
    };
    let Some(container) = tree.tiles.parent_of(pane) else {
        return false;
    };
    tree.set_visible(container, visible);
    true
}

/// Activate a tab inside the detail region, making the region visible.
pub(super) fn activate_detail_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    activate_group_tab(tree, &DETAIL_TABS, WorkspaceTab::Inspector, tab)
}

/// Activate a tab inside the bottom region, making the region visible.
pub(super) fn activate_bottom_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    activate_group_tab(tree, &BOTTOM_TABS, WorkspaceTab::Timeline, tab)
}

fn activate_group_tab(
    tree: &mut Tree<WorkspaceTab>,
    members: &[WorkspaceTab],
    probe: WorkspaceTab,
    tab: WorkspaceTab,
) -> bool {
    if !members.contains(&tab) || tree.tiles.find_pane(&tab).is_none() {
        return false;
    }
    set_group_visible(tree, probe, true);
    tree.make_active(|_id, tile| matches!(tile, Tile::Pane(pane) if *pane == tab))
}

/// Select a tab inside the detail region *without* changing the region's
/// visibility.
///
/// Used by the compact overlay drawer, which renders the active detail tab
/// while the docked region stays hidden. The dock tile and the drawer read the
/// same active tab, so switching in either place stays consistent.
pub(super) fn set_active_detail_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    if !DETAIL_TABS.contains(&tab) || tree.tiles.find_pane(&tab).is_none() {
        return false;
    }
    tree.make_active(|_id, tile| matches!(tile, Tile::Pane(pane) if *pane == tab))
}

/// Which detail tab is currently active.
pub(super) fn active_detail_tab(tree: &Tree<WorkspaceTab>) -> Option<WorkspaceTab> {
    active_group_tab(tree, &DETAIL_TABS, WorkspaceTab::Inspector)
}

/// Which bottom tab is currently active.
pub(super) fn active_bottom_tab(tree: &Tree<WorkspaceTab>) -> Option<WorkspaceTab> {
    active_group_tab(tree, &BOTTOM_TABS, WorkspaceTab::Timeline)
}

fn active_group_tab(
    tree: &Tree<WorkspaceTab>,
    members: &[WorkspaceTab],
    probe: WorkspaceTab,
) -> Option<WorkspaceTab> {
    let pane = tree.tiles.find_pane(&probe)?;
    let container = tree.tiles.parent_of(pane)?;
    let tabs = match tree.tiles.get_container(container)? {
        Container::Tabs(tabs) => tabs,
        _ => return None,
    };
    for tab in members {
        if let Some(id) = tree.tiles.find_pane(tab) {
            if tabs.is_active(id) {
                return Some(*tab);
            }
        }
    }
    None
}

fn set_pane_visible(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab, visible: bool) -> bool {
    match tree.tiles.find_pane(&tab) {
        Some(id) => {
            tree.set_visible(id, visible);
            true
        },
        None => false,
    }
}

pub(super) fn persistence_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("workspace_layout.ron");
    }

    PathBuf::from(".animatix-workspace-layout.ron")
}

pub(super) fn load_workspace_persistence(path: &Path) -> Option<WorkspacePersistence> {
    let content = fs::read_to_string(path).ok()?;
    ron::from_str::<WorkspacePersistence>(&content).ok()
}

// ── Window geometry / workspace layout persistence ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SettingsPersistence {
    pub rebuild_debounce_ms: u64,
    pub scrub_step_s: f64,
    pub nudge_step_px: f32,
    pub nudge_step_shift_px: f32,
    pub rotation_snap_degrees: f32,
    pub snap_fps: f32,
    pub keyframe_merge_window_s: f64,
    pub undo_limit: usize,
    pub grid_size: f32,
    /// IDE appearance: "auto" | "light" | "dark". Defaults to "auto".
    #[serde(default = "default_app_theme")]
    pub app_theme: String,
    /// True when the user prefers reduced motion. Defaults to false.
    #[serde(default)]
    pub reduce_motion: bool,
    /// Density preference: "default" or "compact". Defaults to "default".
    #[serde(default = "default_density")]
    pub density: String,
    /// Optional directory containing eparts JSON theme files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_dir: Option<PathBuf>,
    /// Selected theme name inside `theme_dir`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_name: Option<String>,
    /// Persisted shortcut overrides keyed by stable binding name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shortcuts: BTreeMap<String, SavedShortcut>,
    /// Explicit plugin manifest/library paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_paths: Vec<PathBuf>,
}

/// Load persisted shortcut overrides from the workspace persistence file.
fn default_density() -> String {
    "default".to_string()
}

fn default_app_theme() -> String {
    "auto".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorkspacePersistence {
    pub(crate) tree: Tree<WorkspaceTab>,
    #[serde(default)]
    pub(crate) window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) window_maximized: Option<bool>,
    #[serde(default)]
    pub(crate) settings: Option<SettingsPersistence>,
}

// ── App state persistence (recent file, preferences) ─────────────────────

/// Persisted crash-recovery autosave preferences.
///
/// Both fields are `#[serde(default)]` so an `app_state.ron` written before
/// autosave existed (or by an older build) keeps loading.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct AutosavePrefs {
    #[serde(default = "default_autosave_enabled")]
    pub enabled: bool,
    #[serde(default = "default_autosave_interval_s")]
    pub interval_s: f64,
}

impl Default for AutosavePrefs {
    fn default() -> Self {
        Self {
            enabled: default_autosave_enabled(),
            interval_s: default_autosave_interval_s(),
        }
    }
}

fn default_autosave_enabled() -> bool {
    true
}

fn default_autosave_interval_s() -> f64 {
    crate::app::stores::ui_store::DEFAULT_AUTOSAVE_INTERVAL_S
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_persistence_roundtrips_plugin_paths() {
        let settings = SettingsPersistence {
            rebuild_debounce_ms: 150,
            scrub_step_s: 0.05,
            nudge_step_px: 1.0,
            nudge_step_shift_px: 8.0,
            rotation_snap_degrees: 15.0,
            snap_fps: 60.0,
            keyframe_merge_window_s: 0.05,
            undo_limit: 100,
            grid_size: 40.0,
            app_theme: "dark".to_string(),
            reduce_motion: false,
            density: "default".to_string(),
            theme_dir: None,
            theme_name: None,
            shortcuts: BTreeMap::new(),
            plugin_paths: vec![PathBuf::from("/tmp/plugins")],
        };
        let serialized = ron::ser::to_string_pretty(&settings, ron::ser::PrettyConfig::default())
            .expect("serialize settings");
        let parsed: SettingsPersistence =
            ron::from_str(&serialized).expect("parse settings roundtrip");
        assert_eq!(parsed.plugin_paths, settings.plugin_paths);
    }

    #[test]
    fn app_state_roundtrips_autosave_prefs() {
        let state = AppState {
            recent_file: Some(PathBuf::from("/tmp/scene.amx")),
            recent_files: vec![PathBuf::from("/tmp/scene.amx")],
            autosave: AutosavePrefs {
                enabled: false,
                interval_s: 45.0,
            },
        };
        let serialized =
            ron::ser::to_string_pretty(&state, ron::ser::PrettyConfig::default()).unwrap();
        let parsed: AppState = ron::from_str(&serialized).unwrap();

        assert!(!parsed.autosave.enabled);
        assert_eq!(parsed.autosave.interval_s, 45.0);
        assert_eq!(parsed.recent_file, state.recent_file);
    }

    #[test]
    fn app_state_without_autosave_field_uses_defaults() {
        // Shape written before autosave existed: no `autosave` key at all.
        let legacy = "(recent_file: Some(\"/tmp/scene.amx\"), recent_files: [\"/tmp/scene.amx\"])";
        let parsed: AppState = ron::from_str(legacy).expect("legacy app state must still load");

        assert!(parsed.autosave.enabled, "autosave defaults on");
        assert_eq!(
            parsed.autosave.interval_s,
            crate::app::stores::ui_store::DEFAULT_AUTOSAVE_INTERVAL_S
        );
    }

    #[test]
    fn autosave_prefs_defaults_are_stable() {
        let prefs = AutosavePrefs::default();
        assert!(prefs.enabled);
        assert_eq!(prefs.interval_s, 20.0);
    }

    #[test]
    fn detail_region_toggles_without_rebuilding() {
        let mut tree = default_tree();
        assert!(tree.tiles.find_pane(&WorkspaceTab::Inspector).is_some());
        assert!(tree.tiles.find_pane(&WorkspaceTab::Code).is_some());
        assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Inspector));

        // Hiding the detail region keeps every pane in the tree.
        assert!(set_detail_visible(&mut tree, false));
        let inspector = tree.tiles.find_pane(&WorkspaceTab::Inspector).unwrap();
        let detail = tree.tiles.parent_of(inspector).unwrap();
        assert!(!tree.is_visible(detail));

        // Activating a tab shows the region again and switches the tab.
        assert!(activate_detail_tab(&mut tree, WorkspaceTab::Code));
        assert!(tree.is_visible(detail));
        assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Code));

        // Other panes survive untouched.
        assert!(tree.tiles.find_pane(&WorkspaceTab::Timeline).is_some());
        assert!(tree.tiles.find_pane(&WorkspaceTab::Preview).is_some());
        assert!(tree.tiles.find_pane(&WorkspaceTab::Sidebar).is_some());
    }

    // ── Compact (narrow-window) downgrade ─────────────────────────────────

    #[test]
    fn compact_decision_uses_the_breakpoint() {
        assert!(compact_for_width(COMPACT_BREAKPOINT - 1.0));
        assert!(!compact_for_width(COMPACT_BREAKPOINT));
        assert!(!compact_for_width(COMPACT_BREAKPOINT + 1.0));
        // A degenerate (unmeasured) width must not flip into compact mode.
        assert!(!compact_for_width(0.0));
        assert!(!compact_for_width(-100.0));
    }

    #[test]
    fn compact_entry_hides_detail_and_restores_on_exit() {
        let mut tree = default_tree();
        let mut restore = None;
        assert!(detail_visible(&tree), "detail starts docked");
        assert!(pane_visible(&tree, WorkspaceTab::Sidebar));

        let entered = reconcile_compact(&mut tree, false, true, &mut restore);
        assert_eq!(entered, CompactTransition::Entered);
        assert!(!detail_visible(&tree), "detail dock hidden while compact");
        assert!(pane_visible(&tree, WorkspaceTab::Sidebar), "rail stays visible");
        assert!(restore.is_some(), "entry captures state to restore");
        // The dock is hidden, but the tab is still selectable (drawer reads it).
        assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Inspector));

        let exited = reconcile_compact(&mut tree, true, false, &mut restore);
        assert_eq!(exited, CompactTransition::Exited);
        assert!(detail_visible(&tree), "widening restores the detail dock");
        assert!(restore.is_none(), "restore state is consumed");

        // A no-op transition leaves the tree alone.
        assert_eq!(
            reconcile_compact(&mut tree, false, false, &mut restore),
            CompactTransition::Unchanged
        );
    }

    #[test]
    fn compact_exit_restores_a_user_hidden_sidebar() {
        // The user collapsed the sidebar (or was in Focus mode); compact mode
        // must still show the rail, but widening must return to *their* state,
        // not force panes back open.
        let mut tree = default_tree();
        set_pane_visible(&mut tree, WorkspaceTab::Sidebar, false);
        let mut restore = None;

        reconcile_compact(&mut tree, false, true, &mut restore);
        assert!(pane_visible(&tree, WorkspaceTab::Sidebar), "rail is reachable while compact");

        reconcile_compact(&mut tree, true, false, &mut restore);
        assert!(!pane_visible(&tree, WorkspaceTab::Sidebar), "user's hidden sidebar is restored");
    }

    #[test]
    fn apply_compact_entry_reshapes_a_rebuilt_tree() {
        // A preset/reset while compact rebuilds the tree with the detail
        // visible; the migration path reshapes it without a transition.
        let mut tree = default_tree();
        assert!(detail_visible(&tree));
        apply_compact_entry(&mut tree);
        assert!(!detail_visible(&tree));
        assert!(pane_visible(&tree, WorkspaceTab::Sidebar));
        // Idempotent.
        apply_compact_entry(&mut tree);
        assert!(!detail_visible(&tree));
    }

    #[test]
    fn enforce_compact_rail_gives_the_sidebar_a_fixed_width() {
        let mut tree = default_tree();
        let width = 900.0;
        enforce_compact_layout(&mut tree, LayoutPreset::Animate, width, 700.0);

        let sidebar = tree.tiles.find_pane(&WorkspaceTab::Sidebar).unwrap();
        let preview = tree.tiles.find_pane(&WorkspaceTab::Preview).unwrap();
        let top_row = tree.tiles.parent_of(sidebar).unwrap();
        let egui_tiles::Tile::Container(Container::Linear(linear)) =
            tree.tiles.get(top_row).unwrap()
        else {
            panic!("expected a linear top row");
        };
        let px = linear.shares.split(&[sidebar, preview], width);
        assert!(
            (px[0] - crate::app::panels::COMPACT_RAIL_WIDTH).abs() < 0.01,
            "sidebar is the icon rail width, got {}",
            px[0]
        );
        assert!((px[1] - (width - crate::app::panels::COMPACT_RAIL_WIDTH)).abs() < 0.01);
    }

    #[test]
    fn set_active_detail_tab_does_not_show_the_region() {
        let mut tree = default_tree();
        set_detail_visible(&mut tree, false);
        assert!(set_active_detail_tab(&mut tree, WorkspaceTab::Code));
        assert_eq!(active_detail_tab(&tree), Some(WorkspaceTab::Code));
        assert!(!detail_visible(&tree), "selecting a tab must not unhide the dock");
        // A pane outside the detail group is rejected.
        assert!(!set_active_detail_tab(&mut tree, WorkspaceTab::Preview));
    }
}
