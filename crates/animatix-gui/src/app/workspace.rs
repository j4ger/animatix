use super::*;
use animatix::timeline::Timeline;
use kurbo::Point;

const DIAGNOSTICS_PER_PHASE_LIMIT: usize = 3;

#[derive(Default)]
pub(super) struct UiActions {
    pub(super) open_file: Option<PathBuf>,
    pub(super) toggle_expand_dir: Option<PathBuf>,
    pub(super) show_inspector: bool,
    pub(super) save: bool,
    pub(super) reload: bool,
    pub(super) rebuild: bool,
    pub(super) toggle_playback: bool,
    pub(super) scrub_to: Option<f64>,
    pub(super) editor_changed: bool,
    pub(super) request_repaint: bool,
    pub(super) prev_keyframe: bool,
    pub(super) next_keyframe: bool,
    pub(super) select_actor: Option<String>,
}

pub(super) struct WorkspaceViewer<'a> {
    pub(super) current_file: &'a Path,
    pub(super) workspace_root: &'a Path,
    pub(super) expanded_dirs: &'a mut HashSet<PathBuf>,
    pub(super) file_tree: &'a [FileTreeEntry],
    pub(super) timeline_markers: Vec<f64>,
    pub(super) editor: &'a mut EditorBuffer,
    pub(super) preview: &'a mut PreviewPaneState,
    pub(super) diagnostics: &'a [Diagnostic],
    pub(super) preview_texture_id: Option<egui::TextureId>,
    pub(super) actions: &'a mut UiActions,
    pub(super) source_dirty: &'a mut String,
    pub(super) scene_dimensions: SceneDimensions,
    pub(super) timeline: Option<&'a Timeline>,
    pub(super) selected_actor: &'a mut Option<String>,
    pub(super) hit_regions: &'a [(String, kurbo::Rect)],
}

impl TabViewer for WorkspaceViewer<'_> {
    type Tab = WorkspaceTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            WorkspaceTab::Explorer => "Explorer".into(),
            WorkspaceTab::Editor => "Editor".into(),
            WorkspaceTab::Preview => "Preview".into(),
            WorkspaceTab::Inspector => "Inspector".into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            WorkspaceTab::Explorer => self.explorer_ui(ui),
            WorkspaceTab::Editor => self.editor_ui(ui),
            WorkspaceTab::Preview => self.preview_ui(ui),
            WorkspaceTab::Inspector => self.inspector_ui(ui),
        }
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        false
    }
}

impl WorkspaceViewer<'_> {
    fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label(RichText::new("Workspace").strong());
            ui.label(
                RichText::new(self.workspace_root.display().to_string())
                    .monospace()
                    .small(),
            );
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                for entry in self.file_tree {
                    let is_selected = !entry.is_dir && entry.path == self.current_file;
                    let label = if entry.is_dir {
                        let is_expanded = self.expanded_dirs.contains(&entry.path);
                        let icon = if is_expanded { "📂" } else { "📁" };
                        format!("{} {}", icon, entry.name)
                    } else {
                        let is_amx = entry.path.extension().and_then(|e| e.to_str()) == Some("amx");
                        let icon = if is_amx { "🎬" } else { "📄" };
                        format!("{} {}", icon, entry.name)
                    };

                    let height = 20.0;
                    let (rect, response) = ui.allocate_at_least(
                        Vec2::new(ui.available_width(), height),
                        egui::Sense::click(),
                    );

                    ui.painter().rect_filled(
                        rect.expand(0.5),
                        2.0,
                        match (is_selected, response.hovered()) {
                            (true, _) => Color32::from_rgb(63, 81, 181),
                            (_, true) => Color32::from_rgb(50, 50, 60),
                            _ => Color32::TRANSPARENT,
                        },
                    );

                    let text_rect = Rect::from_min_max(
                        Pos2::new(rect.min.x + entry.depth as f32 * EXPLORER_INDENT_PX, rect.min.y),
                        Pos2::new(rect.max.x, rect.max.y),
                    );
                    let is_amx = !entry.is_dir && entry.path.extension().and_then(|e| e.to_str()) == Some("amx");
                    let text_color = if is_amx {
                        Color32::from_rgb(137, 200, 235)
                    } else {
                        Color32::from_rgb(200, 200, 200)
                    };
                    ui.painter().text(
                        text_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::TextStyle::Small.resolve(ui.style()),
                        text_color,
                    );

                    if response.clicked() {
                        if entry.is_dir {
                            self.actions.toggle_expand_dir = Some(entry.path.clone());
                        } else {
                            self.actions.open_file = Some(entry.path.clone());
                        }
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(4.0);
                ui.collapsing("Action Registry", |ui| {
                    ui.label(
                        RichText::new("Shipped built-in actions from the runtime registry.")
                            .small()
                            .weak(),
                    );
                    for signature in get_action_signatures() {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!("{} · {}", signature.category, signature.name))
                                .strong()
                                .small(),
                        );
                        ui.label(RichText::new(signature.description).small());
                        if !signature.modifiers.is_empty() {
                            let modifier_list = signature
                                .modifiers
                                .iter()
                                .map(|modifier| modifier.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            ui.label(
                                RichText::new(format!("Modifiers: {modifier_list}"))
                                    .small()
                                    .weak(),
                            );
                        }
                    }
                });
            });
        });
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        self.current_file
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Untitled"),
                    )
                    .strong(),
                );
                ui.label(
                    RichText::new(self.current_file.display().to_string())
                        .monospace()
                        .small()
                        .weak(),
                );
            });
            ui.separator();

            let response = self.editor.show(ui);
            if response.changed() {
                *self.source_dirty = self.editor.text().to_string();
                self.actions.editor_changed = true;
            }
        });
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preview").strong());
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if self.preview.is_playing {
                        badge(
                            ui,
                            "Playing",
                            Color32::from_rgb(46, 106, 80),
                            Color32::from_rgb(216, 249, 235),
                        );
                    } else {
                        badge(
                            ui,
                            "Paused",
                            Color32::from_rgb(60, 64, 76),
                            Color32::from_rgb(228, 232, 243),
                        );
                    }
                });
            });
            ui.label(RichText::new(&self.preview.status).small().weak());
            if !self.diagnostics.is_empty() {
                ui.add_space(4.0);
                if let Some(message) = diagnostics_banner_message(self.diagnostics) {
                    ui.colored_label(
                        diagnostics_summary_color(self.diagnostics),
                        RichText::new(message).small().strong(),
                    );
                    ui.add_space(2.0);
                }
                ui.colored_label(
                    diagnostics_summary_color(self.diagnostics),
                    RichText::new(diagnostics_phase_summary(self.diagnostics)).small(),
                );
                render_diagnostics_by_phase(ui, self.diagnostics);
            }
            ui.separator();

            let available = ui.available_size_before_wrap();
            let reserved_height = PREVIEW_NON_CANVAS_HEIGHT.min((available.y - 80.0).max(0.0));
            let image_height = ((available.y - reserved_height).max(180.0))
                .min((available.y * PREVIEW_MAX_HEIGHT_RATIO).max(180.0));
        let desired = fit_preview(
            self.scene_dimensions,
            Vec2::new(available.x.max(200.0), image_height),
        );

            let (preview_rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
            ui.painter().rect_stroke(
                preview_rect,
                6.0,
                Stroke::new(1.0, Color32::from_rgb(58, 63, 74)),
                egui::StrokeKind::Outside,
            );
            ui.painter()
                .rect_filled(preview_rect, 6.0, Color32::from_rgb(18, 20, 24));

            // Click-to-select: test click against actor hit regions
            if response.clicked() && !self.hit_regions.is_empty() {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    // Map click position in preview rect to scene coordinates
                    let scale_x = self.scene_dimensions.width as f64 / desired.x as f64;
                    let scale_y = self.scene_dimensions.height as f64 / desired.y as f64;
                    let scene_x = (click_pos.x - preview_rect.min.x) as f64 * scale_x;
                    let scene_y = (click_pos.y - preview_rect.min.y) as f64 * scale_y;
                    let scene_point = Point::new(scene_x, scene_y);

                    // Iterate in reverse: last-drawn (children) are on top
                    for (label, bounds) in self.hit_regions.iter().rev() {
                        if bounds.contains(scene_point) {
                            self.actions.select_actor = Some(label.clone());
                            break;
                        }
                    }
                }
            }

            match self.preview_texture_id {
                Some(texture_id) => {
                    ui.put(preview_rect, egui::Image::new((texture_id, desired)));
                }
                None => {
                    ui.painter().text(
                        preview_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Preview initializing…",
                        egui::TextStyle::Body.resolve(ui.style()),
                        Color32::from_rgb(150, 155, 168),
                    );
                }
            }

            let has_error = self.preview.error.is_some();
            let transport_height = PREVIEW_TRANSPORT_HEIGHT + if has_error { 26.0 } else { 0.0 };
            ui.add_space((ui.available_height() - transport_height).max(0.0));

            egui::Frame::new()
                .fill(Color32::from_rgb(22, 25, 31))
                .stroke(Stroke::new(1.0, Color32::from_rgb(52, 58, 68)))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "t = {:.2}s / {:.2}s",
                                        self.preview.current_time_s, self.preview.duration_s
                                    ))
                                    .strong(),
                                );
                                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "{} × {}",
                        self.scene_dimensions.width,
                        self.scene_dimensions.height,
                    ))
                    .small()
                    .weak(),
                );
                            });

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Rebuild").clicked() {
                        self.actions.rebuild = true;
                    }
                    ui.add_space(4.0);
                    if ui.button(">>").on_hover_text_at_pointer("Next keyframe (.)")
                        .clicked()
                    {
                        self.actions.next_keyframe = true;
                    }
                    if ui.button("<<").on_hover_text_at_pointer("Previous keyframe (,)")
                        .clicked()
                    {
                        self.actions.prev_keyframe = true;
                    }
                    ui.add_space(4.0);
                    if ui
                        .button(if self.preview.is_playing { "Pause" } else { "Play" })
                        .clicked()
                    {
                        self.actions.toggle_playback = true;
                    }
                });
                        });

                        if let Some(error) = &self.preview.error {
                            ui.add_space(2.0);
                            ui.colored_label(Color32::from_rgb(255, 136, 136), error);
                        }

                        ui.add_space(4.0);
                        let mut scrub = self.preview.current_time_s;
                        if paint_timeline_scrubber(
                            ui,
                            &mut scrub,
                            self.preview.duration_s,
                            &self.timeline_markers,
                            self.preview.is_playing,
                        ) {
                            self.actions.scrub_to = Some(scrub);
                        }
                    });
                });
        });
    }

    fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        let current_time_s = self.preview.current_time_s;
        inspector::inspector_ui(ui, self.timeline, self.selected_actor, current_time_s);
    }
}

fn render_diagnostics_by_phase(ui: &mut egui::Ui, diagnostics: &[Diagnostic]) {
    for summary in diagnostics_summary_by_phase(diagnostics) {
        egui::CollapsingHeader::new(summary.label())
            .default_open(true)
            .show(ui, |ui| {
                let phase_diagnostics: Vec<_> = diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.phase == summary.phase)
                    .collect();

                for diagnostic in phase_diagnostics.iter().take(DIAGNOSTICS_PER_PHASE_LIMIT) {
                    ui.label(RichText::new(format_diagnostic(diagnostic)).small());
                }

                if phase_diagnostics.len() > DIAGNOSTICS_PER_PHASE_LIMIT {
                    ui.label(
                        RichText::new(format!(
                            "… and {} more {} diagnostics",
                            phase_diagnostics.len() - DIAGNOSTICS_PER_PHASE_LIMIT,
                            summary.phase
                        ))
                        .small()
                        .weak(),
                    );
                }
            });
    }
}
