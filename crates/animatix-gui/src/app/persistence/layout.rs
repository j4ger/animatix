use super::*;

struct PresetRegions {
    left: (f32, f32, f32),
    bottom: (f32, f32, f32),
}

fn preset_regions(preset: LayoutPreset) -> PresetRegions {
    use metrics::*;
    match preset {
        LayoutPreset::Animate => PresetRegions {
            left: (LEFT_RATIO, LEFT_MIN, LEFT_MAX),
            bottom: (0.32, 200.0, 420.0),
        },
        LayoutPreset::Code => PresetRegions {
            left: (0.14, 170.0, 290.0),
            bottom: (0.18, 138.0, 245.0),
        },
    }
}

fn clamp_ratio(ratio: f32, available: f32, min: f32, max: f32) -> f32 {
    (ratio * available).clamp(min, max)
}

pub(crate) fn build_tree_for(preset: LayoutPreset, width: f32, height: f32) -> Tree<WorkspaceTab> {
    let mut tiles = Tiles::default();
    let sidebar = tiles.insert_pane(WorkspaceTab::Sidebar);
    let preview = tiles.insert_pane(WorkspaceTab::Preview);
    let code = tiles.insert_pane(WorkspaceTab::Code);
    let inspector = tiles.insert_pane(WorkspaceTab::Inspector);
    let timeline = tiles.insert_pane(WorkspaceTab::Timeline);
    let bottom = tiles.insert_tab_tile(vec![timeline]);
    let top = tiles.insert_container(Linear::new(
        LinearDir::Horizontal,
        vec![sidebar, code, preview, inspector],
    ));
    let root = tiles.insert_container(Linear::new(LinearDir::Vertical, vec![top, bottom]));
    let mut tree = Tree::new(egui::Id::unique("workspace"), root, tiles);
    tree.set_visible(inspector, false);
    apply_layout_preset(&mut tree, preset, width, height);
    tree
}

pub(crate) fn default_tree() -> Tree<WorkspaceTab> {
    build_tree_for(
        LayoutPreset::Animate,
        INITIAL_WINDOW_SIZE.0 as f32,
        INITIAL_WINDOW_SIZE.1 as f32,
    )
}

pub(crate) fn apply_layout_preset(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) -> bool {
    for tab in [
        WorkspaceTab::Sidebar,
        WorkspaceTab::Preview,
        WorkspaceTab::Timeline,
    ] {
        if !set_pane_visible(tree, tab, true) {
            return false;
        }
    }
    if !show_timeline(tree)
        || !set_pane_visible(tree, WorkspaceTab::Code, preset == LayoutPreset::Code)
    {
        return false;
    }
    let regions = preset_regions(preset);
    let left = clamp_ratio(regions.left.0, width, regions.left.1, regions.left.2);
    let editor = (width * 0.44).clamp(300.0, 740.0);
    let detail = (width * 0.21).clamp(224.0, 390.0);
    let preview = (width
        - left
        - if preset == LayoutPreset::Code {
            editor
        } else {
            0.0
        }
        - if detail_visible(tree) { detail } else { 0.0 })
    .max(metrics::PREVIEW_MIN);
    set_horizontal_shares(tree, [left, editor, preview, detail]);
    let dock_h = (height - VERTICAL_CHROME).max(1.0);
    let bottom = clamp_ratio(regions.bottom.0, dock_h, regions.bottom.1, regions.bottom.2);
    set_vertical_shares(tree, dock_h, bottom, false);
    true
}

const TOP_TABS: [WorkspaceTab; 4] = [
    WorkspaceTab::Sidebar,
    WorkspaceTab::Code,
    WorkspaceTab::Preview,
    WorkspaceTab::Inspector,
];

/// Some custom dock arrangements intentionally aren't a single horizontal split.
fn top_split(tree: &Tree<WorkspaceTab>) -> Option<(TileId, [TileId; 4])> {
    let [sidebar, editor, preview, inspector] = TOP_TABS.map(|tab| tree.tiles.find_pane(&tab));
    let panes = [sidebar?, editor?, preview?, inspector?];
    let parent = tree.tiles.parent_of(panes[0])?;
    let Container::Linear(linear) = tree.tiles.get_container(parent)? else {
        return None;
    };
    (linear.dir == LinearDir::Horizontal
        && panes.iter().all(|&pane| linear.children.contains(&pane)))
    .then_some((parent, panes))
}

fn set_horizontal_shares(tree: &mut Tree<WorkspaceTab>, widths: [f32; 4]) {
    if let Some((parent, panes)) = top_split(tree)
        && let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(parent)
    {
        for (pane, width) in panes.into_iter().zip(widths) {
            linear.shares.set_share(pane, width.max(1.0));
        }
    }
}

pub(crate) fn enforce_layout_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    height: f32,
) {
    enforce_horizontal_bounds(tree, preset, width, false);
    enforce_vertical_bounds(tree, preset, height);
}

pub(crate) fn enforce_horizontal_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    width: f32,
    compact: bool,
) {
    if width <= 0.0 {
        return;
    }
    let Some((parent, panes)) = top_split(tree) else {
        return;
    };
    let visible = panes.map(|pane| tree.is_visible(pane));
    let kids: Vec<_> = panes.into_iter().zip(visible).filter_map(|(p, v)| v.then_some(p)).collect();
    let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get(parent) else {
        return;
    };
    let mut allocated = linear.shares.split(&kids, width).into_iter();
    let mut widths = visible.map(|v| {
        if v {
            allocated.next().unwrap_or_default()
        } else {
            0.0
        }
    });
    if visible[0] {
        let left = preset_regions(preset).left;
        widths[0] = if compact {
            crate::app::panels::COMPACT_RAIL_WIDTH
        } else {
            widths[0].clamp(left.1, left.2)
        };
    }
    let usable = (width - widths[0]).max(1.0);
    if visible[1] {
        let min_editor = if compact { 260.0 } else { 310.0 };
        widths[1] = widths[1].clamp(min_editor, 740.0).min(usable * 0.73);
    }
    if visible[3] {
        let remaining = (usable - widths[1] - metrics::PREVIEW_MIN).max(120.0);
        widths[3] = widths[3].clamp(224.0, 390.0).min(remaining);
    }
    widths[2] = (usable - widths[1] - widths[3]).max(1.0);
    let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(parent) else {
        return;
    };
    for i in 0..4 {
        if visible[i] {
            linear.shares.set_share(panes[i], widths[i].max(1.0));
        }
    }
}

fn set_vertical_shares(tree: &mut Tree<WorkspaceTab>, height: f32, bottom: f32, clamp: bool) {
    let Some(root) = tree.root else {
        return;
    };
    let Some(timeline) = tree.tiles.find_pane(&WorkspaceTab::Timeline) else {
        return;
    };
    let Some(region) = direct_region(tree, root, timeline) else {
        return;
    };
    if !tree.is_visible(region) {
        return;
    }
    let Some(Tile::Container(Container::Linear(linear))) = tree.tiles.get_mut(root) else {
        return;
    };
    if linear.dir != LinearDir::Vertical || linear.children.len() != 2 {
        return;
    }
    let bottom = if clamp {
        let px = linear.shares.split(&linear.children, height);
        let Some(index) = linear.children.iter().position(|id| *id == region) else {
            return;
        };
        px[index].clamp(bottom, 420.0)
    } else {
        bottom
    }
    .min(height * 0.65);
    if let Some(top) = linear.children.iter().copied().find(|id| *id != region) {
        linear.shares.set_share(top, (height - bottom).max(1.0));
        linear.shares.set_share(region, bottom.max(1.0));
    }
}

pub(crate) fn enforce_vertical_bounds(
    tree: &mut Tree<WorkspaceTab>,
    preset: LayoutPreset,
    height: f32,
) {
    if height > 0.0 {
        set_vertical_shares(tree, height, preset_regions(preset).bottom.1, true);
    }
}

fn direct_region(tree: &Tree<WorkspaceTab>, root: TileId, mut pane: TileId) -> Option<TileId> {
    loop {
        let parent = tree.tiles.parent_of(pane)?;
        if parent == root {
            return Some(pane);
        }
        pane = parent;
    }
}

pub(crate) fn set_detail_visible(tree: &mut Tree<WorkspaceTab>, visible: bool) -> bool {
    set_pane_visible(tree, WorkspaceTab::Inspector, visible)
}
pub(crate) fn detail_visible(tree: &Tree<WorkspaceTab>) -> bool {
    pane_visible(tree, WorkspaceTab::Inspector)
}

pub(crate) fn show_editor(tree: &mut Tree<WorkspaceTab>) -> bool {
    show_pane(tree, WorkspaceTab::Code)
}
pub(crate) fn show_timeline(tree: &mut Tree<WorkspaceTab>) -> bool {
    show_pane(tree, WorkspaceTab::Timeline)
}
pub(crate) fn show_inspector(tree: &mut Tree<WorkspaceTab>) -> bool {
    show_pane(tree, WorkspaceTab::Inspector)
}

fn show_pane(tree: &mut Tree<WorkspaceTab>, tab: WorkspaceTab) -> bool {
    let Some(mut tile) = tree.tiles.find_pane(&tab) else {
        return false;
    };
    tree.set_visible(tile, true);
    while let Some(parent) = tree.tiles.parent_of(tile) {
        tile = parent;
        tree.set_visible(tile, true);
    }
    tree.make_active(|_id, tile| matches!(tile, Tile::Pane(pane) if *pane == tab));
    true
}

pub(crate) fn set_pane_visible(
    tree: &mut Tree<WorkspaceTab>,
    tab: WorkspaceTab,
    visible: bool,
) -> bool {
    let Some(tile) = tree.tiles.find_pane(&tab) else {
        return false;
    };
    tree.set_visible(tile, visible);
    true
}

pub(crate) fn persistence_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("workspace_layout.ron");
    }

    PathBuf::from(".animatix-workspace-layout.ron")
}

pub(crate) fn load_workspace_persistence(path: &Path) -> Option<WorkspacePersistence> {
    let content = fs::read_to_string(path).ok()?;
    let mut saved: WorkspacePersistence = ron::from_str(&content).ok()?;
    migrate_workspace(&mut saved);
    Some(saved)
}

/// Old trees grouped Editor/Inspector and paired Timeline with a permanent
/// Curves pane. Keep window/settings, map the old active Code tab to Write.
pub(super) fn migrate_workspace(saved: &mut WorkspacePersistence) {
    let tree = &mut saved.tree;
    let old_pair = tree
        .tiles
        .find_pane(&WorkspaceTab::Code)
        .zip(tree.tiles.find_pane(&WorkspaceTab::Inspector));
    let Some((editor, inspector)) = old_pair else {
        saved.tree = default_tree();
        return;
    };
    let legacy_group = tree.tiles.parent_of(editor).filter(|&parent| {
        tree.tiles.parent_of(inspector) == Some(parent)
            && matches!(tree.tiles.get_container(parent), Some(Container::Tabs(_)))
    });
    if let Some(group) = legacy_group.filter(|_| saved.layout_version < WORKSPACE_LAYOUT_VERSION) {
        let write = tree.is_visible(group)
            && matches!(tree.tiles.get_container(group), Some(Container::Tabs(tabs)) if tabs.is_active(editor));
        let [width, height] = saved.window_size.unwrap_or([1440.0, 960.0]);
        let preset = if write {
            LayoutPreset::Code
        } else {
            LayoutPreset::Animate
        };
        saved.layout_preset = Some(preset);
        saved.tree = build_tree_for(preset, width, height);
    } else if let Some(curves) = tree.tiles.find_pane(&WorkspaceTab::Curves) {
        tree.remove_recursively(curves);
    }
}

pub(crate) fn toggle_curve_comparison(tree: &mut Tree<WorkspaceTab>) {
    if let Some(curves) = tree.tiles.find_pane(&WorkspaceTab::Curves) {
        tree.remove_recursively(curves);
        show_timeline(tree);
        return;
    }
    let Some(timeline) = tree.tiles.find_pane(&WorkspaceTab::Timeline) else {
        return;
    };
    let parent = tree.tiles.parent_of(timeline);
    let curves = tree.tiles.insert_pane(WorkspaceTab::Curves);
    if let Some(Tile::Container(Container::Tabs(tabs))) = parent.and_then(|p| tree.tiles.get_mut(p))
    {
        tabs.add_child(curves);
    } else {
        let group = tree.tiles.insert_tab_tile(vec![timeline, curves]);
        if let Some(Tile::Container(container)) = parent.and_then(|p| tree.tiles.get_mut(p)) {
            let _ = container.replace_child(timeline, group);
        } else if tree.root == Some(timeline) {
            tree.root = Some(group);
        }
    }
    show_pane(tree, WorkspaceTab::Curves);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SettingsPersistence {
    pub rebuild_debounce_ms: u64,
    pub scrub_step_s: f64,
    pub snap_fps: f32,
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

pub(crate) const WORKSPACE_LAYOUT_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorkspacePersistence {
    pub(crate) tree: Tree<WorkspaceTab>,
    #[serde(default)]
    pub(crate) layout_version: u8,
    #[serde(default)]
    pub(crate) layout_preset: Option<LayoutPreset>,
    #[serde(default)]
    pub(crate) window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub(crate) window_maximized: Option<bool>,
    #[serde(default)]
    pub(crate) settings: Option<SettingsPersistence>,
}

/// Persisted crash-recovery autosave preferences. Both fields are
/// `#[serde(default)]` so files written before autosave existed keep loading.
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
