use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, RichText, Stroke, Vec2};
use eparts::{Button, Checkbox, NumberField, ProgressBar, TextField};

use crate::app::GuiShell;
use crate::app::components::{TabBar, Tag, layout};
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_XL, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::export_target::ExportScope;
use crate::app::utils::text::{truncate_chars, truncate_middle};

// ─── Export Configuration ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Image,
    Video,
    Gif,
    WebM,
    Mov,
    WebP,
}

#[derive(Clone, Debug)]
pub(crate) struct ExportDialogState {
    pub(crate) format: ExportFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// For Image export — time in seconds.
    pub(crate) time_s: f32,
    /// For Video/GIF export.
    pub(crate) fps: u32,
    /// When true, duration is auto-detected from timeline + hold.
    pub(crate) auto_duration: bool,
    /// Manual duration override (only used when auto_duration is false).
    pub(crate) duration_s: f32,
    /// Seconds to hold after last keyframe (only for auto duration).
    pub(crate) hold_s: f32,
    /// Output file path (relative or absolute).
    pub(crate) output_path: String,
    /// Export scope: ActiveScene or WholeComposition.
    pub(crate) export_scope: ExportScope,
    /// Video encoder selection, shared with CLI export presets.
    pub(crate) video_codec: animatix_render::encode::VideoCodec,
    /// libx264 speed preset, shared with CLI export presets.
    pub(crate) h264_preset: animatix_render::encode::H264Preset,
}

// ─── Export Status ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub(crate) enum ExportStatus {
    Idle,
    Running,
    Complete { path: PathBuf },
    Failed(String),
}

mod actions;
mod impls;
mod progress;
mod settings;
#[cfg(test)]
mod tests;
