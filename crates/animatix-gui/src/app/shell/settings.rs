use egui::RichText;

use crate::app::components::layout;
use crate::app::design_tokens::typography::TextRole;
use crate::app::{GuiShell, components};

const SETTINGS_INPUT_WIDTH: f32 = 120.0;

impl GuiShell {
    pub(crate) fn settings_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec = components::dialog::DialogSpec::new("Settings", [420.0, 520.0])
            .with_min_size([380.0, 400.0])
            .with_max_size([600.0, 700.0])
            .with_resizable(true);

        let open = components::dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = components::dialog::title_row(ui, "Settings");
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_3);

            // ── Preview ──
            layout::section_header(ui, egui_phosphor::regular::GRID_FOUR, "Preview", None);
            ui.add_space(sp.base.space_2);

            {
                let mut v = self.preview_store.preview.overlay.grid_size as f64;
                eparts::widget::Form::new("preview_form")
                    .label_width(SETTINGS_INPUT_WIDTH)
                    .show(ui, |f| {
                        f.field("Grid size", |ui| {
                            eparts::NumberField::new(&mut v)
                                .range(1.0..=200.0)
                                .speed(1.0)
                                .suffix(" px")
                                .show(ui);
                        });
                    });
                self.preview_store.preview.overlay.grid_size = v as f32;
            }
            ui.add_space(sp.base.space_3);

            // ── Appearance (IDE light/dark/auto) ──
            layout::section_header(ui, egui_phosphor::regular::SUN, "Appearance", None);
            ui.add_space(sp.base.space_2);

            {
                let mut theme_idx = match self.ui_store.view.app_theme {
                    eparts::AppThemeChoice::Auto => Some(0),
                    eparts::AppThemeChoice::Light => Some(1),
                    eparts::AppThemeChoice::Dark => Some(2),
                };
                let themes = ["Auto (system)", "Light", "Dark"];

                eparts::widget::Form::new("appearance_form")
                    .label_width(SETTINGS_INPUT_WIDTH)
                    .show(ui, |f| {
                        f.field("Theme", |ui| {
                            ui.add(
                                eparts::widget::Select::new(
                                    "app_theme_select",
                                    &mut theme_idx,
                                    &themes,
                                )
                                .placeholder("Select theme"),
                            );
                        });
                    });

                self.ui_store.view.app_theme = match theme_idx {
                    Some(0) => eparts::AppThemeChoice::Auto,
                    Some(1) => eparts::AppThemeChoice::Light,
                    Some(2) => eparts::AppThemeChoice::Dark,
                    _ => self.ui_store.view.app_theme,
                };

                ui.add_space(sp.base.space_2);

                eparts::widget::Form::new("motion_form").label_width(SETTINGS_INPUT_WIDTH).show(
                    ui,
                    |f| {
                        f.field("Reduce motion", |ui| {
                            ui.checkbox(&mut self.ui_store.view.reduce_motion, "Snap animations");
                        });
                    },
                );

                ui.add_space(sp.base.space_2);

                {
                    let mut density_idx = match self.ui_store.view.density {
                        eparts::Density::Default => Some(0usize),
                        eparts::Density::Compact => Some(1),
                    };
                    let densities = ["Default", "Compact"];

                    eparts::widget::Form::new("density_form")
                        .label_width(SETTINGS_INPUT_WIDTH)
                        .show(ui, |f| {
                            f.field("Density", |ui| {
                                ui.add(
                                    eparts::widget::Select::new(
                                        "density_select",
                                        &mut density_idx,
                                        &densities,
                                    )
                                    .placeholder("Select density"),
                                );
                            });
                        });

                    self.ui_store.view.density = match density_idx {
                        Some(1) => eparts::Density::Compact,
                        _ => eparts::Density::Default,
                    };
                }
            }
            ui.add_space(sp.base.space_3);

            // ── Colorscheme ──
            layout::section_header(ui, egui_phosphor::regular::PALETTE, "Colorscheme", None);
            ui.add_space(sp.base.space_2);

            let current_scheme = self
                .document_store
                .source
                .document
                .timeline
                .as_ref()
                .map(|t| t.colorscheme_name())
                .unwrap_or("default-dark")
                .to_string();
            let schemes = [
                ("default-dark", "Default Dark"),
                ("default-light", "Default Light"),
                ("editorial-dark", "Editorial Dark"),
            ];
            layout::labeled_row(
                ui,
                RichText::new("Theme").size(TextRole::BodyS.size()).color(theme.text.secondary),
                SETTINGS_INPUT_WIDTH,
                |ui| {
                    egui::ComboBox::from_id_salt(ui.id().with("colorscheme"))
                        .selected_text(
                            schemes
                                .iter()
                                .find(|(id, _)| *id == current_scheme)
                                .map(|(_, name)| *name)
                                .unwrap_or(&current_scheme),
                        )
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (id, name) in schemes {
                                if ui.selectable_label(id == current_scheme, name).clicked()
                                    && id != current_scheme
                                {
                                    if let Some(ref mut stmts) =
                                        self.document_store.source.document.raw_statements
                                    {
                                        let edit =
                                            crate::source_edit::SourceEdit::SetConfigProperty {
                                                key: "colorscheme".into(),
                                                value: animatix_syntax::ast::Expr::Str(id.into()),
                                            };
                                        if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                                            let (new_source, source_index) = (
                                                animatix_syntax::to_source::stmts_to_source(stmts),
                                                animatix_syntax::source_index::SourceIndex::build(
                                                    stmts,
                                                ),
                                            );
                                            self.document_store
                                                .commit_source(new_source, source_index);
                                            self.preview_store.pending_rebuild_at = Some(
                                                std::time::Instant::now()
                                                    + std::time::Duration::from_millis(
                                                        self.ui_store.rebuild_debounce_ms,
                                                    ),
                                            );
                                            self.preview_store.preview.status =
                                                format!("Colorscheme changed to {}", name);
                                        }
                                    }
                                }
                            }
                        });
                },
            );
            ui.add_space(sp.base.space_3);

            // ── Input ──
            layout::section_header(ui, egui_phosphor::regular::CURSOR_CLICK, "Input", None);
            ui.add_space(sp.base.space_2);

            {
                let mut nudge_step_px = self.ui_store.nudge_step_px as f64;
                let mut nudge_step_shift_px = self.ui_store.nudge_step_shift_px as f64;
                let mut rotation_snap_degrees = self.ui_store.rotation_snap_degrees as f64;

                eparts::widget::Form::new("input_form").label_width(SETTINGS_INPUT_WIDTH).show(
                    ui,
                    |f| {
                        f.field("Nudge step", |ui| {
                            eparts::NumberField::new(&mut nudge_step_px)
                                .range(0.1..=50.0)
                                .speed(0.5)
                                .suffix(" px")
                                .show(ui);
                        });
                        f.field("Nudge step (Shift)", |ui| {
                            eparts::NumberField::new(&mut nudge_step_shift_px)
                                .range(1.0..=200.0)
                                .speed(0.5)
                                .suffix(" px")
                                .show(ui);
                        });
                        f.field("Rotation snap", |ui| {
                            eparts::NumberField::new(&mut rotation_snap_degrees)
                                .range(1.0..=90.0)
                                .speed(1.0)
                                .suffix("°")
                                .show(ui);
                        });
                    },
                );

                self.ui_store.nudge_step_px = nudge_step_px as f32;
                self.ui_store.nudge_step_shift_px = nudge_step_shift_px as f32;
                self.ui_store.rotation_snap_degrees = rotation_snap_degrees as f32;
            }
            ui.add_space(sp.base.space_3);

            // ── Playback ──
            layout::section_header(ui, egui_phosphor::regular::PLAY, "Playback", None);
            ui.add_space(sp.base.space_2);

            {
                let mut scrub_step_s = self.ui_store.scrub_step_s;
                eparts::widget::Form::new("playback_form")
                    .label_width(SETTINGS_INPUT_WIDTH)
                    .show(ui, |f| {
                        f.field("Scrub step", |ui| {
                            eparts::NumberField::new(&mut scrub_step_s)
                                .range(0.01..=1.0)
                                .speed(0.01)
                                .suffix(" s")
                                .show(ui);
                        });
                    });
                self.ui_store.scrub_step_s = scrub_step_s;
            }
            ui.add_space(sp.base.space_3);

            // ── Editor ──
            layout::section_header(ui, egui_phosphor::regular::PENCIL, "Editor", None);
            ui.add_space(sp.base.space_2);

            {
                let mut rebuild_debounce_ms = self.ui_store.rebuild_debounce_ms as f64;
                let mut undo_limit = self.document_store.history.undo_limit as f64;
                let mut snap_fps = self.ui_store.snap_fps as f64;
                let mut keyframe_merge_window_ms = self.ui_store.keyframe_merge_window_s * 1000.0;

                eparts::widget::Form::new("editor_form").label_width(SETTINGS_INPUT_WIDTH).show(
                    ui,
                    |f| {
                        f.field("Rebuild debounce", |ui| {
                            eparts::NumberField::new(&mut rebuild_debounce_ms)
                                .range(0.0..=1000.0)
                                .speed(10.0)
                                .suffix(" ms")
                                .show(ui);
                        });
                        f.field("Undo limit", |ui| {
                            eparts::NumberField::new(&mut undo_limit)
                                .range(10.0..=1000.0)
                                .speed(10.0)
                                .suffix(" entries")
                                .show(ui);
                        });
                        f.field("Snap FPS", |ui| {
                            eparts::NumberField::new(&mut snap_fps)
                                .range(1.0..=240.0)
                                .speed(1.0)
                                .suffix(" fps")
                                .show(ui);
                        });
                        f.field("Keyframe merge window", |ui| {
                            eparts::NumberField::new(&mut keyframe_merge_window_ms)
                                .range(0.0..=500.0)
                                .speed(1.0)
                                .suffix(" ms")
                                .show(ui);
                        });
                    },
                );

                self.ui_store.rebuild_debounce_ms = rebuild_debounce_ms as u64;
                self.document_store.history.undo_limit = undo_limit as usize;
                self.ui_store.snap_fps = snap_fps as f32;
                self.ui_store.keyframe_merge_window_s =
                    (keyframe_merge_window_ms / 1000.0).max(0.0);
            }
            close
        });

        if !open {
            self.ui_store.view.settings_open = false;
        }
    }
}
