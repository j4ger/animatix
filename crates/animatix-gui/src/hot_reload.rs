use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher, Event};
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

pub struct HotReloader {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    last_event: Option<Instant>,
    debounce_duration: Duration,
    watched_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum ReloadStatus {
    NoChange,
    ShouldReload { path: PathBuf },
}

impl HotReloader {
    pub fn new(file_path: &Path) -> Result<Self, String> {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Err(e) = tx.send(res) {
                    tracing::warn!("Hot reload watcher send failed: {:?}", e);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(100)),
        )
        .map_err(|e| format!("Failed to create file watcher: {}", e))?;

        watcher
            .watch(file_path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch file: {}", e))?;

        Ok(HotReloader {
            _watcher: watcher,
            rx,
            last_event: None,
            debounce_duration: Duration::from_millis(300),
            watched_path: file_path.to_path_buf(),
        })
    }

    pub fn update(&mut self, app_time: Instant) -> ReloadStatus {
        // Drain all pending events
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(event) => {
                    // Check if this is a modify event for our watched file
                    if matches!(event.kind, notify::EventKind::Modify(notify::event::ModifyKind::Data(_) | notify::event::ModifyKind::Any)) {
                        self.last_event = Some(app_time);
                    }
                }
                Err(e) => {
                    eprintln!("Watch error: {:?}", e);
                }
            }
        }

        // Check if debounce period has passed since last event
        if let Some(last) = self.last_event {
            if app_time.duration_since(last) >= self.debounce_duration {
                self.last_event = None;
                return ReloadStatus::ShouldReload {
                    path: self.watched_path.clone(),
                };
            }
        }

        ReloadStatus::NoChange
    }

    pub fn update_watched_file(&mut self, new_path: &PathBuf) -> Result<(), String> {
        if new_path != &self.watched_path {
            // Unwatch old file
            if let Err(e) = self._watcher.unwatch(&self.watched_path) {
                tracing::warn!("Failed to unwatch file {:?}: {:?}", self.watched_path, e);
            }

            // Watch new file
            self._watcher
                .watch(new_path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch new file: {}", e))?;

            self.watched_path = new_path.clone();
            self.last_event = None;
        }
        Ok(())
    }
}