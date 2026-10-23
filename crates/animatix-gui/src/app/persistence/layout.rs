use super::*;

pub(crate) fn clamp_ratio(ratio: f32, available: f32, min: f32, max: f32) -> f32 {
    (ratio * available).clamp(min, max)
}

/// Per-preset region preferences (ratios, pixel bounds, detail tab).
pub(crate) struct PresetRegions {
    left: (f32, f32, f32),
    detail: (f32, f32, f32),
    bottom: (f32, f32, f32),
    tab: WorkspaceTab,
    hide_surroundings: bool,
}

pub(crate) fn preset_regions(preset: LayoutPreset) -> PresetRegions {
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
pub(crate) fn build_tree_for(preset: LayoutPreset, width: f32, height: f32) -> Tree<WorkspaceTab> {
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
pub(crate) fn default_tree() -> Tree<WorkspaceTab> {
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
pub(crate) fn apply_layout_preset(
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
pub(crate) fn enforce_layout_bounds(
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
pub(crate) fn enforce_horizontal_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
) {
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
pub(crate) fn enforce_vertical_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    height: f32,
) {
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
pub(crate) fn bottom_region_child(tree: &Tree<WorkspaceTab>, timeline: TileId) -> Option<TileId> {
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
pub(crate) fn set_detail_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    set_group_visible(tree, WorkspaceTab::Inspector, visible)
}

/// Show or hide the whole bottom region (Timeline + Curves tab group).
pub(crate) fn set_bottom_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    set_group_visible(tree, WorkspaceTab::Timeline, visible)
}

/// Whether the detail region (Inspector + Code tab group) is visible.
///
/// Visibility lives on the group container, so this reports the whole region
/// rather than the pane's own (always-on) flag.
pub(crate) fn detail_visible(tree: &Tree<WorkspaceTab>) -> bool {
    group_visible(tree, WorkspaceTab::Inspector)
}

/// Whether the bottom region (Timeline + Curves tab group) is visible.
pub(crate) fn bottom_visible(tree: &Tree<WorkspaceTab>) -> bool {
    group_visible(tree, WorkspaceTab::Timeline)
}

pub(crate) fn group_visible(tree: &Tree<WorkspaceTab>, probe: WorkspaceTab) -> bool {
    tree.tiles
        .find_pane(&probe)
        .and_then(|pane| tree.tiles.parent_of(pane))
        .is_none_or(|container| tree.is_visible(container))
}

pub(crate) fn set_group_visible(
    tree: &mut Tree<WorkspaceTab>,
    probe: WorkspaceTab,
    visible: bool,
) -> bool {
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
pub(crate) fn activate_detail_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    activate_group_tab(tree, &DETAIL_TABS, WorkspaceTab::Inspector, tab)
}

/// Activate a tab inside the bottom region, making the region visible.
pub(crate) fn activate_bottom_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    activate_group_tab(tree, &BOTTOM_TABS, WorkspaceTab::Timeline, tab)
}

pub(crate) fn activate_group_tab(
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
pub(crate) fn set_active_detail_tab(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    if !DETAIL_TABS.contains(&tab) || tree.tiles.find_pane(&tab).is_none() {
        return false;
    }
    tree.make_active(|_id, tile| matches!(tile, Tile::Pane(pane) if *pane == tab))
}

/// Which detail tab is currently active.
pub(crate) fn active_detail_tab(tree: &Tree<WorkspaceTab>) -> Option<WorkspaceTab> {
    active_group_tab(tree, &DETAIL_TABS, WorkspaceTab::Inspector)
}

/// Which bottom tab is currently active.
pub(crate) fn active_bottom_tab(tree: &Tree<WorkspaceTab>) -> Option<WorkspaceTab> {
    active_group_tab(tree, &BOTTOM_TABS, WorkspaceTab::Timeline)
}

pub(crate) fn active_group_tab(
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

pub(crate) fn set_pane_visible(
    tree: &mut Tree<WorkspaceTab>,
    tab: WorkspaceTab,
    visible: bool,
) -> bool {
    match tree.tiles.find_pane(&tab) {
        Some(id) => {
            tree.set_visible(id, visible);
            true
        },
        None => false,
    }
}

pub(crate) fn persistence_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("workspace_layout.ron");
    }

    PathBuf::from(".animatix-workspace-layout.ron")
}

pub(crate) fn load_workspace_persistence(path: &Path) -> Option<WorkspacePersistence> {
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
pub(crate) fn default_density() -> String {
    "default".to_string()
}

pub(crate) fn default_app_theme() -> String {
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

pub(crate) fn default_autosave_enabled() -> bool {
    true
}

pub(crate) fn default_autosave_interval_s() -> f64 {
    crate::app::stores::ui_store::DEFAULT_AUTOSAVE_INTERVAL_S
}
