//! Crash-recovery autosave: periodic sidecar writes, startup detection, and
//! the restore prompt.
//!
//! The recovery file is a sibling of the source (`<path>.autosave`) written
//! atomically through [`crate::document::write_recovery`]. It is only ever
//! written while the document is dirty, so it always holds edits that are not
//! on disk, and it is removed on a successful real save or on a clean exit.

use std::time::{Duration, Instant};

use crate::app::GuiShell;
use crate::app::components::button::Button;
use crate::app::components::dialog;
use crate::app::components::toast::Toast;
use crate::app::design_tokens::spatial::{ROW_L, SPACE_3, SPACE_5};
use crate::app::design_tokens::typography::TextRole;
use crate::document::{clear_recovery, recovery_is_newer, recovery_path, write_recovery};

impl GuiShell {
    /// Persist the current autosave preferences into `app_state.ron`.
    pub(crate) fn persist_autosave_prefs(&self) {
        crate::app::persistence::save_autosave_prefs(self.ui_store.view.autosave.prefs());
    }

    /// Whether the autosave timer could fire for the current document.
    pub(crate) fn autosave_active(&self) -> bool {
        self.ui_store.view.autosave.enabled
            && !self.ui_store.view.welcome_open
            && !self.ui_store.recovery_prompt.is_open
            && self.document_store.source.is_dirty()
    }

    /// True while an undecided recovery prompt holds the only copy of a
    /// previous session's edits.
    ///
    /// Global save commands are refused in this window: a reflexive `Ctrl+S`
    /// (which succeeds even on a clean document) would otherwise delete the
    /// sidecar before the user has chosen Recover or Discard.
    pub(crate) fn recovery_prompt_pending(&self) -> bool {
        self.ui_store.recovery_prompt.is_open
    }

    /// Toast to show when a save is refused because of a pending prompt.
    pub(crate) fn recovery_prompt_save_blocked(&self) -> Toast {
        Toast::warning("Choose Recover or Discard before saving")
    }

    /// How long until the next autosave write, for `request_repaint_after`.
    /// Returns `None` when autosave is idle for this frame.
    pub(crate) fn autosave_repaint_delay(&self, now: Instant) -> Option<Duration> {
        self.autosave_active().then(|| self.ui_store.view.autosave.remaining(now))
    }

    /// Drive the autosave timer. Writes the live editor text to the recovery
    /// sidecar when due; a failed write is logged and retried next interval
    /// rather than every frame.
    pub(crate) fn autosave_tick(&mut self, now: Instant) {
        if self.ui_store.view.welcome_open {
            return;
        }
        // While the recovery prompt is open the sidecar is still the only copy
        // of the previous session's edits; writing over it would discard them.
        // The timer restarts (and the first write becomes due) once the user
        // chooses Recover or Discard.
        if self.ui_store.recovery_prompt.is_open {
            return;
        }
        let source_path = self.document_store.source.file_path().to_path_buf();
        self.ui_store.view.autosave.track_source(&source_path);

        if !self.ui_store.view.autosave.enabled {
            return;
        }
        if !self.document_store.source.is_dirty() {
            // A clean document has nothing worth recovering; clear the clock so
            // the next edit is snapshotted immediately.
            self.ui_store.view.autosave.last_write = None;
            return;
        }
        if !self.ui_store.view.autosave.is_due(now) {
            return;
        }

        let text = self.document_store.source.editor.text().to_string();
        match write_recovery(&source_path, &text) {
            Ok(()) => {
                tracing::debug!(
                    "Autosaved {} bytes to recovery file for {}",
                    text.len(),
                    source_path.display()
                );
            },
            Err(err) => {
                tracing::warn!("Autosave failed for {}: {}", source_path.display(), err);
            },
        }
        // Record the attempt either way so a persistent failure does not retry
        // (and log) on every frame.
        self.ui_store.view.autosave.note_write(now);
    }

    /// Remove the recovery sidecar for the open document, if any.
    ///
    /// Called after a successful real save and on clean exit. Skipped on the
    /// welcome screen, where `file_path` is a placeholder, and while an
    /// undecided recovery prompt is open, where the sidecar is the only copy.
    pub(crate) fn clear_recovery_for_current_document(&self) {
        if self.ui_store.view.welcome_open || self.ui_store.recovery_prompt.is_open {
            return;
        }
        clear_recovery(self.document_store.source.file_path());
    }

    /// Offer recovery when a sidecar exists and is strictly newer than the
    /// source file.
    ///
    /// Called after startup load and after opening a document mid-session, so an
    /// opened file's pending sidecar is surfaced before autosave can overwrite
    /// it. Does not modify the document; the user chooses in the prompt.
    pub(crate) fn detect_recovery_prompt(&mut self) {
        if self.ui_store.view.welcome_open || self.ui_store.recovery_prompt.is_open {
            return;
        }
        let source_path = self.document_store.source.file_path().to_path_buf();
        if !recovery_is_newer(&source_path) {
            return;
        }
        let sidecar = recovery_path(&source_path);
        tracing::info!(
            "Found newer recovery file {} for {}",
            sidecar.display(),
            source_path.display()
        );
        // Remember the document path so a later save clears the same sidecar.
        self.ui_store.view.autosave.track_source(&source_path);
        self.ui_store.recovery_prompt.open(source_path, sidecar);
    }

    /// Startup prompt: Recover the sidecar into the editor, or Discard it.
    pub(crate) fn recovery_prompt_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let spec =
            dialog::DialogSpec::new("crash_recovery", [460.0, 220.0]).with_min_size([400.0, 200.0]);

        // Escape and backdrop clicks are not decisions: `modal` returns false
        // for them, but this prompt stays open until Recover/Discard calls
        // `close()` (which flips `is_open`). The return value is therefore not
        // used to close the prompt.
        let _still_open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            // The X button is deliberately ignored: neither outcome (Recover,
            // Discard) is safe to trigger implicitly, so a dismissal must not
            // count as a decision.
            let _title_close = dialog::title_row(
                ui,
                &format!(
                    "{}  Recover unsaved changes",
                    egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE
                ),
            );
            let mut body_close = false;
            ui.add_space(SPACE_3);
            ui.separator();
            ui.add_space(SPACE_3);

            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.ui_store.recovery_prompt.message)
                        .size(TextRole::Body.size())
                        .color(theme.palette.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(SPACE_5);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Recover: replace the editor text with the sidecar contents
                    // and mark the document dirty so it is not silently saved.
                    let recover = ui.add_sized(
                        [110.0, ROW_L],
                        Button::primary("Recover")
                            .with_icon(egui_phosphor::regular::ARROW_CLOCKWISE),
                    );
                    if recover.clicked() {
                        match self.apply_recovery() {
                            Ok(()) => {
                                body_close = true;
                            },
                            Err(err) => {
                                tracing::warn!("Recovery failed: {}", err);
                                self.ui_store
                                    .toasts
                                    .push(Toast::error(format!("Recovery failed: {err}")));
                                // Keep the prompt open so the file is not lost.
                            },
                        }
                    }

                    // Discard: the user does not want the sidecar; delete it.
                    let discard = ui.add_sized(
                        [110.0, ROW_L],
                        Button::danger("Discard").with_icon(egui_phosphor::regular::TRASH),
                    );
                    if discard.clicked() {
                        self.discard_recovery();
                        body_close = true;
                    }
                });
            });

            body_close
        });
    }

    /// Load the recovery sidecar into the editor and mark the document dirty.
    fn apply_recovery(&mut self) -> Result<(), String> {
        let Some(sidecar) = self.ui_store.recovery_prompt.recovery_path.clone() else {
            return Err("no recovery file recorded".to_string());
        };
        let text = std::fs::read_to_string(&sidecar).map_err(|err| err.to_string())?;

        self.document_store.replace_text(text);
        self.preview_store.pending_rebuild_at =
            Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
        self.preview_store.preview.set_status_info(format!(
            "Recovered unsaved changes from {} • rebuilding",
            sidecar.display()
        ));
        self.ui_store
            .toasts
            .push(Toast::success("Recovered unsaved changes — review and save"));
        // The sidecar stays on disk until a real save or explicit discard, so a
        // crash during review still leaves the edits recoverable.
        self.ui_store.recovery_prompt.close();
        tracing::info!("Recovered unsaved changes from {}", sidecar.display());
        Ok(())
    }

    /// Delete the recovery sidecar and close the prompt.
    fn discard_recovery(&mut self) {
        let sidecar = self.ui_store.recovery_prompt.recovery_path.clone();
        let removed = self
            .ui_store
            .recovery_prompt
            .source_path
            .as_deref()
            .map(clear_recovery)
            .unwrap_or(false);
        if let (Some(sidecar), true) = (sidecar.as_deref(), removed) {
            tracing::info!("Discarded recovery file {}", sidecar.display());
            self.preview_store.preview.set_status_info("Discarded recovered changes");
        }
        self.ui_store.recovery_prompt.close();
    }
}
