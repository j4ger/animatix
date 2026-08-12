//! Live theme-directory watching (`theme-json` feature).
//!
//! Unlike [`ThemeWatcher`](super::theme_watcher::ThemeWatcher), which watches a
//! single file, this watcher watches every theme in a directory and reloads the
//! full [`ThemeRegistry`]. That keeps inherited themes fresh when a base file
//! changes.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use super::theme_registry::{ThemeRegistry, ThemeRegistryError};

/// Status returned by [`ThemeRegistryWatcher::update`].
#[derive(Debug, Clone)]
pub enum ThemeRegistryWatcherEvent {
    /// No reloadable change since the previous update.
    NoChange,
    /// The theme directory was reloaded successfully.
    Reloaded(ThemeRegistry),
    /// The watcher noticed a change but the registry could not be loaded.
    Error(String),
}

/// Watches a theme directory and reloads the full registry after a debounce.
pub struct ThemeRegistryWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    last_event: Option<Instant>,
    debounce_duration: Duration,
    directory: PathBuf,
}

impl ThemeRegistryWatcher {
    /// Create a watcher for `directory` with the default 300ms debounce.
    pub fn new(directory: impl Into<PathBuf>) -> Result<Self, String> {
        Self::with_debounce(directory, Duration::from_millis(300))
    }

    /// Create a watcher with an explicit debounce duration.
    pub fn with_debounce(
        directory: impl Into<PathBuf>,
        debounce_duration: Duration,
    ) -> Result<Self, String> {
        let directory = directory.into();
        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Err(e) = tx.send(res) {
                    tracing::warn!("Theme registry watcher send failed: {:?}", e);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(100)),
        )
        .map_err(|e| format!("Failed to create theme registry watcher: {e}"))?;
        watcher
            .watch(&directory, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch theme directory: {e}"))?;
        Ok(Self {
            _watcher: watcher,
            rx,
            last_event: None,
            debounce_duration,
            directory,
        })
    }

    /// Drain pending events and reload once the debounce has elapsed.
    pub fn update(&mut self, now: Instant) -> ThemeRegistryWatcherEvent {
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
                    tracing::warn!("Theme registry watcher error: {:?}", e);
                },
            }
        }

        if let Some(last) = self.last_event {
            if now.duration_since(last) >= self.debounce_duration {
                self.last_event = None;
                return match ThemeRegistry::from_directory(&self.directory) {
                    Ok(registry) => ThemeRegistryWatcherEvent::Reloaded(registry),
                    Err(ThemeRegistryError::Io(e)) => ThemeRegistryWatcherEvent::Error(format!(
                        "failed to read theme directory: {e}"
                    )),
                    Err(ThemeRegistryError::Theme(e)) => {
                        ThemeRegistryWatcherEvent::Error(format!("failed to load theme file: {e}"))
                    },
                    Err(e) => ThemeRegistryWatcherEvent::Error(e.to_string()),
                };
            }
        }
        ThemeRegistryWatcherEvent::NoChange
    }

    /// The directory currently being watched.
    pub fn directory(&self) -> &PathBuf {
        &self.directory
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn write_theme(dir: &Path, name: &str, base: &str) {
        let json =
            format!(r##"{{ "name": "{name}", "dark": {{ "surface": {{ "base": "{base}" }} }} }}"##);
        std::fs::write(dir.join(format!("{name}.json")), json).unwrap();
    }

    #[test]
    fn registry_watcher_reloads_after_directory_change() {
        let dir = std::env::temp_dir()
            .join(format!("eparts-theme-registry-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        write_theme(&dir, "base", "#101418");
        std::fs::write(dir.join("child.json"), r##"{ "name":"child", "extends":"base" }"##)
            .unwrap();

        let mut watcher = ThemeRegistryWatcher::with_debounce(&dir, Duration::from_millis(50))
            .expect("create watcher");
        std::thread::sleep(Duration::from_millis(100));

        // Change the base file so the child's inherited value changes.
        std::fs::write(
            dir.join("base.json"),
            r##"{ "name":"base", "dark": { "surface": { "base": "#abcdef" } } }"##,
        )
        .unwrap();

        let mut saw_reload = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let ThemeRegistryWatcherEvent::Reloaded(registry) = watcher.update(Instant::now()) {
                let child = registry.resolved("child").expect("child resolved");
                assert_eq!(
                    child.dark_theme().surface.base,
                    egui::Color32::from_rgb(0xab, 0xcd, 0xef)
                );
                saw_reload = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_reload, "expected registry reload after base change");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
