use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::bindings::{self, Binding};
use super::history::History;
use crate::app::stores::SourceStore;

pub(super) struct Session {
    pub path: PathBuf,
    text: String,
    pub bindings: Vec<Binding>,
    pub history: Option<Arc<History>>,
}

impl Session {
    pub fn open(source: &SourceStore) -> Result<Self, String> {
        let history =
            History::open(source.file_path(), false).map_err(|error| error.to_string())?;
        let mut session = Self {
            path: source.file_path().to_owned(),
            text: source.text().to_owned(),
            bindings: Vec::new(),
            history: history.map(Arc::new),
        };
        session.bindings = session.reopen_bindings(source.text())?;
        if source.is_dirty()
            && let Ok(saved) = std::fs::read_to_string(source.file_path())
        {
            // Retain the disk version when the first take is made from a draft.
            session.bindings = if session.exact_bindings(source.text())?.is_some() {
                session.bindings
            } else {
                bindings::edit(&saved, source.text(), &session.reopen_bindings(&saved)?)
            };
        }
        Ok(session)
    }

    fn exact_bindings(&self, source: &str) -> Result<Option<Vec<Binding>>, String> {
        let Some(history) = &self.history else {
            return Ok(None);
        };
        let (matched, old) = history.bindings(source).map_err(|error| error.to_string())?;
        Ok(matched.then_some(old))
    }

    fn reopen_bindings(&self, source: &str) -> Result<Vec<Binding>, String> {
        let Some(history) = &self.history else {
            return Ok(bindings::reconcile(source, &[]));
        };
        let (matched, old) = history.bindings(source).map_err(|error| error.to_string())?;
        Ok(if matched {
            old
        } else {
            bindings::reconcile(source, &old)
        })
    }

    pub fn update(&mut self, source: &SourceStore) -> Result<bool, String> {
        if self.text == source.text() {
            return Ok(false);
        }
        let text = source.text();
        self.bindings = if let Some(exact) = self.exact_bindings(text)? {
            exact
        } else if source.is_dirty() {
            bindings::edit(&self.text, text, &self.bindings)
        } else {
            bindings::reconcile(text, &self.bindings)
        };
        self.text = text.to_owned();
        Ok(true)
    }

    pub fn history_for_write(&mut self) -> Result<Arc<History>, String> {
        if !self.path.is_file() {
            return Err("Save the scene before recording.".into());
        }
        if self.history.as_ref().is_none_or(|history| !history.is_writable()) {
            let history = History::open(&self.path, true)
                .map_err(|error| error.to_string())?
                .ok_or("Could not open voiceover history.")?;
            if !history.is_writable() {
                return Err("This voiceover is open for editing in another window.".into());
            }
            let (exact, rows) = history.bindings(&self.text).map_err(|error| error.to_string())?;
            let borrowed = !rows.is_empty();
            if borrowed {
                self.bindings = if exact {
                    rows
                } else {
                    bindings::reconcile(&self.text, &rows)
                };
            }
            self.history = Some(Arc::new(history));
            if !borrowed && let Ok(saved) = std::fs::read_to_string(&self.path) {
                self.checkpoint_text(&saved, true)?;
            }
        }
        let history = self.history.as_ref().unwrap();
        history
            .snapshot(&self.text, &self.bindings, false)
            .map_err(|error| error.to_string())?;
        Ok(Arc::clone(history))
    }

    fn checkpoint_text(&self, text: &str, saved: bool) -> Result<(), String> {
        if let Some(history) = self.history.as_ref().filter(|history| history.is_writable()) {
            let mapped = if self.text == text {
                self.bindings.clone()
            } else {
                bindings::edit(&self.text, text, &self.bindings)
            };
            history.snapshot(text, &mapped, saved).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn checkpoint(&mut self, source: &SourceStore, saved: bool) -> Result<(), String> {
        self.update(source)?;
        self.checkpoint_text(source.text(), saved)
    }

    pub fn binding(&self, scene: Option<&str>, selector: &str) -> Option<&Binding> {
        self.bindings
            .iter()
            .find(|binding| binding.scene.as_deref() == scene && binding.selector == selector)
    }

    pub fn is_for(&self, path: &Path) -> bool {
        self.path == path
    }
}
