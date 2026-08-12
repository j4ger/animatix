//! Live theme-file watching (`theme-json` feature).
//!
//! [`ThemeWatcher`] watches one `.theme.json` file, debounces write bursts,
//! reloads it when the file settles, and hands the parsed [`ThemeFile`] back
//! to the caller. It deliberately has no egui dependency: the host app decides
//! when to install the new theme via `set_theme`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use super::theme_json::{ThemeFile, ThemeJsonError};

/// Status returned by [`ThemeWatcher::update`].
#[derive(Debug, Clone)]
pub enum ThemeWatcherEvent {
    /// No reloadable change since the previous update.
    NoChange,
    /// A theme file was reloaded successfully.
    Reloaded(Box<ThemeFile>),
    /// The watcher noticed a change but could not parse the file.
    Error(String),
}

/// Watches a JSON theme file and reports reloads after a debounce period.
pub struct ThemeWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    last_event: Option<Instant>,
    debounce_duration: Duration,
    watched_path: PathBuf,
}

impl ThemeWatcher {
    /// Create a watcher for `path` with the default 300ms debounce.
    pub fn new(path: &Path) -> Result<Self, String> {
        Self::with_debounce(path, Duration::from_millis(300))
    }

    /// Create a watcher with an explicit debounce duration.
    pub fn with_debounce(path: &Path, debounce_duration: Duration) -> Result<Self, String> {
        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Err(e) = tx.send(res) {
                    tracing::warn!("Theme watcher send failed: {:?}", e);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(100)),
        )
        .map_err(|e| format!("Failed to create theme watcher: {e}"))?;
        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch theme file: {e}"))?;
        Ok(Self {
            _watcher: watcher,
            rx,
            last_event: None,
            debounce_duration,
            watched_path: path.to_path_buf(),
        })
    }

    /// Drain pending events and return a reload once the debounce has elapsed.
    pub fn update(&mut self, now: Instant) -> ThemeWatcherEvent {
        while let Ok(result) = self.rx.try_recv() {
            match result {
                Ok(event) => {
                    if matches!(
                        event.kind,
                        notify::EventKind::Modify(_)
                            | notify::EventKind::Create(_)
                            | notify::EventKind::Remove(_)
                            | notify::EventKind::Any
                    ) {
                        self.last_event = Some(now);
                    }
                },
                Err(e) => {
                    tracing::warn!("Theme watcher error: {:?}", e);
                },
            }
        }

        if let Some(last) = self.last_event {
            if now.duration_since(last) >= self.debounce_duration {
                self.last_event = None;
                return match ThemeFile::load(&self.watched_path) {
                    Ok(theme) => ThemeWatcherEvent::Reloaded(Box::new(theme)),
                    Err(ThemeJsonError::Io(e)) => {
                        ThemeWatcherEvent::Error(format!("failed to read theme file: {e}"))
                    },
                    Err(ThemeJsonError::Json(e)) => {
                        ThemeWatcherEvent::Error(format!("invalid theme JSON: {e}"))
                    },
                };
            }
        }
        ThemeWatcherEvent::NoChange
    }

    /// Switch to watching a different theme file.
    pub fn watch(&mut self, new_path: &Path) -> Result<(), String> {
        if new_path != self.watched_path {
            if let Err(e) = self._watcher.unwatch(&self.watched_path) {
                tracing::warn!("Failed to unwatch theme file {:?}: {:?}", self.watched_path, e);
            }
            self._watcher
                .watch(new_path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch new theme file: {e}"))?;
            self.watched_path = new_path.to_path_buf();
            self.last_event = None;
        }
        Ok(())
    }

    /// The path currently being watched.
    pub fn watched_path(&self) -> &Path {
        &self.watched_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn write_theme(path: &Path, base: &str) {
        let json = format!(
            r##"{{ "name": "{base}", "dark": {{ "surface": {{ "base": "#101418" }} }} }}"##
        );
        std::fs::write(path, json).expect("write theme");
    }

    #[test]
    fn watcher_reloads_after_file_change() {
        let dir = std::env::temp_dir().join(format!("eparts-theme-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("theme.json");
        write_theme(&path, "first");

        let mut watcher =
            ThemeWatcher::with_debounce(&path, Duration::from_millis(50)).expect("create watcher");
        // Let notify register the watch before the first write so the event is observed.
        std::thread::sleep(Duration::from_millis(100));
        write_theme(&path, "first");

        let mut saw_reload = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let ThemeWatcherEvent::Reloaded(theme) = watcher.update(Instant::now()) {
                assert_eq!(theme.name.as_deref(), Some("first"));
                saw_reload = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_reload, "expected theme reload after first write");

        write_theme(&path, "second");
        let mut saw_second = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let ThemeWatcherEvent::Reloaded(theme) = watcher.update(Instant::now()) {
                assert_eq!(theme.name.as_deref(), Some("second"));
                saw_second = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_second, "expected reload after file change");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watcher_can_switch_path() {
        let dir = std::env::temp_dir().join(format!("eparts-theme-switch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let first = dir.join("one.json");
        let second = dir.join("two.json");
        write_theme(&first, "one");
        write_theme(&second, "two");

        let mut watcher =
            ThemeWatcher::with_debounce(&first, Duration::from_millis(50)).expect("create watcher");
        std::thread::sleep(Duration::from_millis(100));
        watcher.watch(&second).expect("switch path");
        std::thread::sleep(Duration::from_millis(100));
        write_theme(&second, "two");
        assert_eq!(watcher.watched_path(), second);

        let mut saw_two = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let ThemeWatcherEvent::Reloaded(theme) = watcher.update(Instant::now()) {
                assert_eq!(theme.name.as_deref(), Some("two"));
                saw_two = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_two, "expected reload from switched path");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watcher_reports_invalid_json_as_error() {
        let dir = std::env::temp_dir().join(format!("eparts-theme-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("theme.json");
        std::fs::write(&path, "{}").expect("write initial theme");

        let mut watcher =
            ThemeWatcher::with_debounce(&path, Duration::from_millis(50)).expect("create watcher");
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(&path, "{ not json").expect("write invalid theme");

        let mut saw_error = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(watcher.update(Instant::now()), ThemeWatcherEvent::Error(_)) {
                saw_error = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_error, "expected invalid JSON error");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
