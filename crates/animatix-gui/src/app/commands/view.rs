#[derive(Debug, Clone)]
#[allow(dead_code)] // Reserved for ViewCommand infrastructure (panel migration Phase 2)
pub enum ViewCommand {
    ScrollToLine(usize, usize),
    ZoomToSelection,
    ZoomToAll,
    SetTimelineZoom(f32),
    SetTimelineScroll(f32),
    SetLoopRegion {
        start: Option<f64>,
        end: Option<f64>,
    },
    ToggleCollapseActor(String),
    TogglePropertyLane(String),
    SetPreviewZoom(f32),
    SetPreviewZoomCentered {
        zoom: f32,
        center_x: f32,
        center_y: f32,
    },
    SetPreviewPan(egui::Vec2),
    SetToolMode(crate::app::preview::ToolMode),
    SetSidebarTab(crate::app::panels::SidebarTab),
    SetPropertyViewMode(crate::app::panels::inspector::PropertyViewMode),
    SetKeyframeViewMode(crate::app::panels::inspector::KeyframeViewMode),
    SetPivotOffset {
        actor: String,
        offset: [f32; 2],
    },
}

impl From<ViewCommand> for super::Command {
    fn from(c: ViewCommand) -> Self {
        match c {
            ViewCommand::ScrollToLine(a, b) => super::Command::ScrollToLine(a, b),
            ViewCommand::ZoomToSelection => super::Command::ZoomToSelection,
            ViewCommand::ZoomToAll => super::Command::ZoomToAll,
            ViewCommand::SetTimelineZoom(v) => super::Command::SetTimelineZoom(v),
            ViewCommand::SetTimelineScroll(v) => super::Command::SetTimelineScroll(v),
            ViewCommand::SetLoopRegion { start, end } => {
                super::Command::SetLoopRegion { start, end }
            },
            ViewCommand::ToggleCollapseActor(v) => super::Command::ToggleCollapseActor(v),
            ViewCommand::TogglePropertyLane(v) => super::Command::TogglePropertyLane(v),
            ViewCommand::SetPreviewZoom(v) => super::Command::SetPreviewZoom(v),
            ViewCommand::SetPreviewZoomCentered {
                zoom,
                center_x,
                center_y,
            } => super::Command::SetPreviewZoomCentered {
                zoom,
                center_x,
                center_y,
            },
            ViewCommand::SetPreviewPan(v) => super::Command::SetPreviewPan(v),
            ViewCommand::SetToolMode(v) => super::Command::SetToolMode(v),
            ViewCommand::SetSidebarTab(v) => super::Command::SetSidebarTab(v),
            ViewCommand::SetPropertyViewMode(v) => super::Command::SetPropertyViewMode(v),
            ViewCommand::SetKeyframeViewMode(v) => super::Command::SetKeyframeViewMode(v),
            ViewCommand::SetPivotOffset { actor, offset } => {
                super::Command::SetPivotOffset { actor, offset }
            },
        }
    }
}
