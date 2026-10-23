//! Preview panel: canvas with rulers, zoom/pan, drag interaction, and overlays.

use animatix::timeline::SceneDimensions;
use egui::{RichText, Vec2};
use eparts::widget::UiExt;

use crate::app::commands::{ActorCommand, DocumentCommand, PlaybackCommand};
use crate::app::components::button::{Button, toolbar_separator};
use crate::app::components::text_tooltip;
use crate::app::design_tokens::spatial::{RADIUS_L, STROKE_WIDTH, preview as preview_spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::{RULER_SIZE, nice_tick_interval};
pub(crate) use crate::app::preview::context::PreviewContext;
use crate::app::preview::{self, DragState, ToolMode, fit_preview, selection};

/// Floating Presentation HUD: pill hovering at the bottom center of the canvas in presentation mode.
mod coords;
mod drag;
mod hud;
mod panel;

pub(crate) use panel::preview_panel_ui;
