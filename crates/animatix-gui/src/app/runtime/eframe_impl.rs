use super::helpers::*;
use super::*;

impl eframe::App for AnimatixApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = visuals.panel_fill.to_normalized_gamma_f32();
        [r, g, b, a]
    }

    fn on_exit(&mut self) {
        self.shell.save_persistence();
        if self.shell.ui_store.view.welcome_open {
            clear_app_state();
        } else {
            // A clean shutdown leaves nothing to recover, so drop the sidecar.
            // If the document is still dirty (an abnormal/forced exit), keep it
            // so the next launch can offer the unsaved edits back. A pending
            // recovery prompt also keeps it: the user has not chosen yet.
            if !self.shell.document_store.source.is_dirty() {
                self.shell.clear_recovery_for_current_document();
            }
            save_app_state(&self.shell.document_store.source.document.file_path);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Intercept close when document has unsaved changes
        if self.shell.document_store.source.is_dirty()
            && !self.shell.ui_store.unsaved_changes.is_open
        {
            let close_requested = ui.input(|i| i.viewport().close_requested());
            if close_requested {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.shell.ui_store.unsaved_changes.open_for_close();
            }
        }

        // Record frame tick for performance HUD
        self.shell.preview_store.performance_metrics.record_tick();

        // PF-9: drain shared stage measurements and append the JSONL perf line
        // when the sink is active. Stages execute on this UI thread; the
        // end-to-end rebuild time arrives separately via record_rebuild.
        if let Some(sink) = self.perf_log.as_mut() {
            let stages = animatix::perf::take_measurements();
            let actor_count = self
                .shell
                .document_store
                .source
                .document
                .active_timeline()
                .map(|timeline| timeline.actor_labels().count())
                .unwrap_or(0);
            let dims = self.preview_surface.dimensions();
            sink.record_frame(
                &self.shell.preview_store.performance_metrics,
                stages,
                actor_count,
                [dims.width as f64, dims.height as f64],
            );
        }

        // Update stale flag and GPU memory estimate
        self.shell
            .preview_store
            .performance_metrics
            .set_stale(self.shell.document_store.snapshot_is_stale());

        // Prepare frame (hot reload, playback tick, pending rebuild)
        self.shell.prepare_frame();

        if let Some(err) = self.shell.workspace_store.hot_reload_error.take() {
            self.shell.ui_store.toasts.push(crate::app::components::toast::Toast::warning(
                format!("Hot reload unavailable: {err}"),
            ));
        }

        // Re-resolve the app theme. Periodically re-probe the OS appearance (cheap
        // amortized: every ~2s) so `Auto` follows runtime OS light/dark changes;
        // reapply only when the effective theme actually changes.
        {
            #[cfg(feature = "theme-json")]
            self.update_json_theme(ui.ctx());

            let now = std::time::Instant::now();
            let due = self
                .last_theme_probe
                .map(|t| now.duration_since(t).as_secs_f32() >= 2.0)
                .unwrap_or(true);
            if due {
                self.system_is_dark = detect_system_dark();
                self.last_theme_probe = Some(now);
            }
            let choice = self.shell.ui_store.view.app_theme;
            let signature = (choice, self.system_is_dark);
            if self.applied_theme_signature != Some(signature) {
                let dark = choice.is_dark(self.system_is_dark);
                let theme = choice.resolve(self.system_is_dark);
                eparts::set_theme(ui.ctx(), theme);
                install_theme(ui.ctx(), &theme, dark);
                self.applied_theme_signature = Some(signature);
            }

            // Sync motion preference (reduced-motion toggle)
            let reduce = self.shell.ui_store.view.reduce_motion;
            if self.applied_motion_preference != Some(reduce) {
                let pref = if reduce {
                    eparts::MotionPreference::Reduced
                } else {
                    eparts::MotionPreference::Full
                };
                eparts::set_motion_preference(ui.ctx(), pref);
                self.applied_motion_preference = Some(reduce);
            }

            // Sync density preference
            let d = self.shell.ui_store.view.density;
            if self.applied_density != Some(d) {
                eparts::set_density(ui.ctx(), d);
                self.applied_density = Some(d);
            }
        }

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui.ctx());

        // Sync audio with playback state
        if let Some(audio_engine) = &mut self.audio_engine {
            let playing = self.shell.preview_store.preview.playback.is_playing;
            let segments = self.shell.document_store.source.document.all_audio_segments();
            let time_s = self.shell.preview_store.preview.playback.current_time_s();
            let playback_speed = self.shell.preview_store.preview.playback.playback_speed;
            audio_engine.sync(&segments, time_s, playing, playback_speed);
        }

        // Sync preview surface (render vello, register texture)
        if let Err(error) = self.sync_preview_surface(frame) {
            self.shell.set_render_error(error);
        }

        // Render the UI
        self.shell.ui(ui, self.preview_texture_id);

        // Debug aid for native IME input: raw events plus focus state.
        self.shell.log_input_debug(ui.ctx());

        // Capture window geometry for persistence
        let (size, maximized) = ui.ctx().input(|i| {
            let rect = i.viewport_rect();
            let max = i.viewport().maximized.unwrap_or(false);
            ([rect.size().x, rect.size().y], max)
        });
        self.shell.window_size = size;
        self.shell.window_maximized = maximized;

        // Request repaint if needed
        if self.shell.is_playing()
            || self.shell.preview_store.preview_dirty
            || self.shell.has_pending_rebuild()
            || self.shell.preview_store.rebuild_in_progress
        {
            ui.ctx().request_repaint();
        } else if let Some(delay) = self.shell.autosave_repaint_delay(std::time::Instant::now()) {
            // Wake up exactly when the next autosave is due; without this the
            // app sleeping while idle would never reach the timer.
            ui.ctx().request_repaint_after(delay);
        }

        #[cfg(feature = "dev-screenshots")]
        self.drive_screenshot(ui);
    }
}
