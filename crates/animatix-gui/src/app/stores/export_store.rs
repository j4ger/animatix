use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::Instant;

use crate::app::i18n::StatusText;
use crate::app::shell::export_dialog::{ExportDialogState, ExportStatus};

/// Owns all export-related state: dialog visibility, export thread,
/// progress tracking, and cancellation flag.
pub struct ExportStore {
    pub export_dialog_open: bool,
    pub export_state: ExportDialogState,
    pub export_status: ExportStatus,
    pub export_thread: Option<
        std::thread::JoinHandle<(
            Result<(), animatix_render::encode::ExportError>,
            std::path::PathBuf,
        )>,
    >,
    pub export_progress: Arc<AtomicU32>,
    pub export_cancelled: Arc<AtomicBool>,
    pub export_start_time: Option<Instant>,
    pub export_total_frames: u32,
    pub export_warnings: Vec<StatusText>,
    pub export_warning_receiver:
        Option<std::sync::mpsc::Receiver<animatix_render::encode::ExportWarning>>,
}

impl ExportStore {
    pub fn new() -> Self {
        Self {
            export_dialog_open: false,
            export_state: ExportDialogState::default(),
            export_status: ExportStatus::Idle,
            export_thread: None,
            export_progress: Arc::new(AtomicU32::new(0)),
            export_cancelled: Arc::new(AtomicBool::new(false)),
            export_start_time: None,
            export_total_frames: 0,
            export_warnings: Vec::new(),
            export_warning_receiver: None,
        }
    }

    /// Call this every frame to check if an export thread finished.
    pub fn poll_export_status(&mut self) {
        if let Some(warnings) = &self.export_warning_receiver {
            self.export_warnings.extend(warnings.try_iter().map(export_warning));
        }
        if let Some(handle) = self.export_thread.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok((Ok(()), path)) => {
                        self.export_status = ExportStatus::Complete { path };
                    },
                    Ok((Err(animatix_render::encode::ExportError::Cancelled), _)) => {
                        self.export_status = ExportStatus::Idle;
                    },
                    Ok((Err(e), _)) => {
                        self.export_status = ExportStatus::Failed(export_error(e));
                    },
                    Err(_) => {
                        self.export_status =
                            ExportStatus::Failed(StatusText::message("gui-export-thread", []));
                    },
                }
            } else {
                self.export_thread = Some(handle);
            }
        }
    }
}

fn export_error(error: animatix_render::encode::ExportError) -> StatusText {
    use animatix_render::encode::ExportError::*;
    match error {
        RendererCreation(detail) => {
            StatusText::message("gui-export-renderer", [("detail", detail.to_string())])
        },
        ImageEncode(detail) => {
            StatusText::message("gui-export-image-encode", [("detail", detail.to_string())])
        },
        ImageSave(detail) => {
            StatusText::message("gui-export-image-save", [("detail", detail.to_string())])
        },
        VideoEncode(detail) => {
            StatusText::message("gui-export-video-encode", [("detail", detail.to_string())])
        },
        GifEncode(detail) => {
            StatusText::message("gui-export-gif-encode", [("detail", detail.to_string())])
        },
        Internal(detail) => {
            StatusText::message("gui-export-internal", [("detail", detail.to_string())])
        },
        InvalidPath(_) => StatusText::message("gui-export-nul", []),
        ThreadPanicked => StatusText::message("gui-export-render-thread", []),
        Cancelled => StatusText::message("gui-export-cancelled", []),
        FrameRender { frame, message } => StatusText::message(
            "gui-export-frame-failed",
            [("frame", frame.to_string()), ("detail", message)],
        ),
    }
}

fn export_warning(warning: animatix_render::encode::ExportWarning) -> StatusText {
    use animatix_render::encode::ExportWarning::*;
    match warning {
        AudioStartsAfterExport {
            source,
            cutoff_s,
            start_s,
        } => StatusText::message(
            "gui-export-warning-audio-start",
            [
                ("source", source),
                ("cutoff", format!("{cutoff_s:.3}")),
                ("start", format!("{start_s:.3}")),
            ],
        ),
        AudioCutOff {
            source,
            cutoff_s,
            end_s,
        } => StatusText::message(
            "gui-export-warning-audio-cutoff",
            [
                ("source", source),
                ("cutoff", format!("{cutoff_s:.3}")),
                ("end", format!("{end_s:.3}")),
            ],
        ),
        CaptionsCutOff {
            scene,
            cutoff_s,
            end_s,
        } => {
            let cutoff = format!("{cutoff_s:.3}");
            let end = format!("{end_s:.3}");
            match scene {
                Some(scene) => StatusText::message(
                    "gui-export-warning-scene-captions",
                    [("scene", scene), ("cutoff", cutoff), ("end", end)],
                ),
                None => StatusText::message(
                    "gui-export-warning-captions",
                    [("cutoff", cutoff), ("end", end)],
                ),
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn export_warning_keeps_canonical_times_and_authored_source() {
        use crate::app::i18n::Language;

        let warning = animatix_render::encode::ExportWarning::AudioCutOff {
            source: "旁白.wav".into(),
            cutoff_s: 12.5,
            end_s: 15.0,
        };
        let canonical = warning.to_string();
        let status = export_warning(warning);
        assert_eq!(status.display(Language::English), canonical);
        let chinese = status.display(Language::SimplifiedChinese);
        assert!(chinese.contains("旁白.wav"));
        assert!(chinese.contains("12.500"));
        assert!(chinese.contains("15.000"));
    }

    #[test]
    fn poll_export_status_reports_complete_and_clears_handle() {
        let handle = std::thread::spawn(|| (Ok(()), PathBuf::from("out.png")));
        while !handle.is_finished() {
            std::thread::yield_now();
        }

        let mut store = ExportStore::new();
        store.export_thread = Some(handle);

        store.poll_export_status();

        assert!(matches!(
            store.export_status,
            ExportStatus::Complete { path } if path.as_path() == std::path::Path::new("out.png")
        ));
        assert!(store.export_thread.is_none());
    }
}
