use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::Instant;

use crate::app::shell::export_dialog::{ExportDialogState, ExportStatus};

/// Owns all export-related state: dialog visibility, export thread,
/// progress tracking, and cancellation flag.
pub struct ExportStore {
    pub export_dialog_open: bool,
    pub export_state: ExportDialogState,
    pub export_status: ExportStatus,
    pub export_thread: Option<
        std::thread::JoinHandle<(Result<(), animatix::renderer::ExportError>, std::path::PathBuf)>,
    >,
    pub export_progress: Arc<AtomicU32>,
    pub export_cancelled: Arc<AtomicBool>,
    pub export_start_time: Option<Instant>,
    pub export_total_frames: u32,
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
        }
    }

    /// Call this every frame to check if an export thread finished.
    pub fn poll_export_status(&mut self) {
        if let Some(handle) = self.export_thread.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok((Ok(()), path)) => {
                        self.export_status = ExportStatus::Complete { path };
                    },
                    Ok((Err(animatix::renderer::ExportError::Cancelled), _)) => {
                        self.export_status = ExportStatus::Idle;
                    },
                    Ok((Err(e), _)) => {
                        self.export_status = ExportStatus::Failed(e.to_string());
                    },
                    Err(_) => {
                        self.export_status = ExportStatus::Failed("Export thread panicked".into());
                    },
                }
            } else {
                self.export_thread = Some(handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

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
