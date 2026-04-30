use super::*;
use animatix::timeline::Timeline;
use preview::DragState;

/// Returns all actors at the given scene point, ordered from topmost (last rendered) to bottommost.
fn actors_at_point(hit_regions: &[(String, kurbo::Rect)], point: kurbo::Point) -> Vec<String> {
    hit_regions
        .iter()
        .rev()
        .filter(|(_, bounds)| bounds.contains(point))
        .map(|(label, _)| label.clone())
        .collect()
}

/// Describes a property edit made in the inspector panel.
#[derive(Debug, Clone)]
pub(super) struct PropertyEdit {
    pub(super) actor: String,
    pub(super) property: String,
    pub(super) value: PropertyValue,
}

/// The typed value of a property edit.
#[derive(Debug, Clone)]
pub(crate) enum PropertyValue {
    Vec2([f32; 2]),
    Float(f32),
    Color([f32; 4]),
    Text(String),
}

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
    pub(super) property_edit: Option<PropertyEdit>,
}

pub(super) struct WorkspaceViewer<'a> {
    pub(super) current_file: &'a Path,
    pub(super) workspace_root: &'a Path,
    pub(super) expanded_dirs: &'a mut HashSet<PathBuf>,
    pub(super) file_tree: &'a [FileTreeEntry],
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
    pub(super) drag_state: &'a mut DragState,
    pub(super) hovered_actor: &'a mut Option<String>,
    pub(super) click_candidates: &'a mut Vec<String>,
    pub(super) cycle_index: &'a mut usize,
    pub(super) last_click_scene: &'a mut Option<kurbo::Point>,
    pub(super) context_menu_open: &'a mut bool,
    pub(super) context_menu_pos: &'a mut Option<Pos2>,
    pub(super) context_menu_actors: &'a mut Vec<String>,
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
            // Minimal header
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preview").strong().size(12.0).color(Color32::from_rgb(150, 158, 175)));
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
                            Color32::from_rgb(40, 44, 52),
                            Color32::from_rgb(150, 158, 175),
                        );
                    }
                });
            });

            // Diagnostics banner (compact)
            if !self.diagnostics.is_empty() {
                if let Some(message) = diagnostics_banner_message(self.diagnostics) {
                    ui.add_space(2.0);
                    ui.colored_label(
                        diagnostics_summary_color(self.diagnostics),
                        RichText::new(message).small().strong(),
                    );
                }
            }

            ui.add_space(4.0);

            // Canvas — takes all available space
            let available = ui.available_size_before_wrap();
            let desired = fit_preview(
                self.scene_dimensions,
                Vec2::new(available.x.max(200.0), available.y.max(180.0)),
            );

            let (preview_rect, response) =
                ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
            ui.painter().rect_stroke(
                preview_rect,
                6.0,
                Stroke::new(1.0, Color32::from_rgb(40, 44, 52)),
                egui::StrokeKind::Outside,
            );
            ui.painter()
                .rect_filled(preview_rect, 6.0, Color32::from_rgb(12, 14, 18));

            // ── Drag interaction handling ───────────────────────────────
            let scale_x = self.scene_dimensions.width as f64 / desired.x as f64;
            let scale_y = self.scene_dimensions.height as f64 / desired.y as f64;

            // Helper: screen position → scene coordinates
            let screen_to_scene = |screen: egui::Pos2| -> kurbo::Point {
                kurbo::Point::new(
                    (screen.x - preview_rect.min.x) as f64 * scale_x,
                    (screen.y - preview_rect.min.y) as f64 * scale_y,
                )
            };

            // Get current pointer position (works even when pointer leaves widget)
            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .filter(|p| preview_rect.contains(*p));

            // Start new drag
            if response.drag_started() {
                if let (Some(actor), Some(mouse)) = (self.selected_actor.clone(), pointer_pos) {
                    let scene = screen_to_scene(mouse);

                    // Check scale handles first (highest priority)
                    if let Some(sel_rect) = preview::selection_screen_rect(
                        &actor,
                        self.hit_regions,
                        preview_rect,
                        self.scene_dimensions,
                        desired,
                    ) {
                        let handle_positions = preview::scale_handle_positions(sel_rect);
                        for (i, handle_pos) in handle_positions.iter().enumerate() {
                            if mouse.distance(*handle_pos) <= 8.0 {
                                let start_size = self
                                    .hit_regions
                                    .iter()
                                    .find(|(l, _)| l == &actor)
                                    .map(|(_, r)| {
                                        [(r.x1 - r.x0) as f32, (r.y1 - r.y0) as f32]
                                    })
                                    .unwrap_or([100.0, 100.0]);
                                *self.drag_state = DragState::Scale {
                                    actor,
                                    handle: i,
                                    start_scene: scene,
                                    start_size,
                                };
                                return;
                            }
                        }

                        // Check rotation handle
                        let top_center = egui::Pos2::new(sel_rect.center().x, sel_rect.top());
                        let rot_center = egui::Pos2::new(
                            top_center.x,
                            top_center.y - preview::ROTATION_OFFSET,
                        );
                        if mouse.distance(rot_center) <= 10.0 {
                            let start_rotation = self
                                .timeline
                                .and_then(|t| t.get_track(&actor))
                                .and_then(|tr| tr.rotation.as_ref())
                                .map(|pt| pt.evaluate(0))
                                .unwrap_or(0.0);
                            *self.drag_state = DragState::Rotate {
                                actor,
                                start_scene: scene,
                                start_rotation,
                            };
                            return;
                        }
                    }

                    // Check actor body (move)
                    for (label, bounds) in self.hit_regions.iter().rev() {
                        if label == &actor && bounds.contains(scene) {
                            let start_position = self
                                .timeline
                                .and_then(|t| t.get_track(&actor))
                                .and_then(|tr| tr.position.as_ref())
                                .map(|pt| pt.evaluate(0))
                                .unwrap_or([0.0, 0.0]);
                            *self.drag_state = DragState::Move {
                                actor,
                                start_scene: scene,
                                start_position,
                            };
                            return;
                        }
                    }
                }
            }

            // Update ongoing drag
            if let DragState::None = *self.drag_state {
            } else if let Some(mouse) = pointer_pos {
                let scene = screen_to_scene(mouse);
                let shift = ui.input(|i| i.modifiers.shift);

                match self.drag_state.clone() {
                    DragState::Move { actor, start_scene, start_position } => {
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        self.actions.property_edit = Some(PropertyEdit {
                            actor,
                            property: "position".into(),
                            value: PropertyValue::Vec2([
                                start_position[0] + dx,
                                start_position[1] + dy,
                            ]),
                        });
                    }
                    DragState::Scale { actor, handle, start_scene: _, start_size } => {
                        if let Some(sel_rect) = preview::selection_screen_rect(
                            &actor,
                            self.hit_regions,
                            preview_rect,
                            self.scene_dimensions,
                            desired,
                        ) {
                            let opposite = preview::scale_handle_positions(sel_rect)
                                [(handle + 2) % 8];
                            let opposite_scene = screen_to_scene(opposite);
                            let dx = (scene.x - opposite_scene.x).abs() as f32;
                            let dy = (scene.y - opposite_scene.y).abs() as f32;
                            let min_size = 10.0;
                            let (new_w, new_h) = if shift {
                                let scale_factor = ((dx / start_size[0].max(1.0))
                                    + (dy / start_size[1].max(1.0)))
                                    / 2.0;
                                (
                                    (start_size[0] * scale_factor).max(min_size),
                                    (start_size[1] * scale_factor).max(min_size),
                                )
                            } else {
                                (dx.max(min_size), dy.max(min_size))
                            };
                            self.actions.property_edit = Some(PropertyEdit {
                                actor,
                                property: "size".into(),
                                value: PropertyValue::Vec2([new_w, new_h]),
                            });
                        }
                    }
                    DragState::Rotate { actor, start_scene: _, start_rotation } => {
                        if let Some(sel_rect) = preview::selection_screen_rect(
                            &actor,
                            self.hit_regions,
                            preview_rect,
                            self.scene_dimensions,
                            desired,
                        ) {
                            let center = sel_rect.center();
                            let center_scene = screen_to_scene(center);
                            let angle = ((scene.y - center_scene.y) as f32)
                                .atan2((scene.x - center_scene.x) as f32);
                            let angle = if shift {
                                let step = std::f32::consts::PI / 12.0;
                                (angle / step).round() * step
                            } else {
                                angle
                            };
                            let delta = angle - start_rotation;
                            let new_rotation = start_rotation + delta;
                            self.actions.property_edit = Some(PropertyEdit {
                                actor,
                                property: "rotation".into(),
                                value: PropertyValue::Float(new_rotation),
                            });
                        }
                    }
                    DragState::None => {}
                }
            }

            // End drag
            if matches!(self.drag_state, DragState::None) == false
                && (response.drag_stopped()
                    || (!response.dragged()
                        && !matches!(self.drag_state, DragState::None)))
            {
                *self.drag_state = DragState::None;
            }

            // ── Hover preview ───────────────────────────────────────────
            if matches!(self.drag_state, DragState::None) {
                if let Some(mouse) = pointer_pos {
                    let scene_point = screen_to_scene(mouse);
                    let candidates = actors_at_point(self.hit_regions, scene_point);
                    *self.hovered_actor = candidates.first().cloned();
                } else {
                    *self.hovered_actor = None;
                }
            }

            // ── Right-click context menu ────────────────────────────────
            if response.secondary_clicked() && matches!(self.drag_state, DragState::None) {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    let scene_point = screen_to_scene(click_pos);
                    let candidates = actors_at_point(self.hit_regions, scene_point);
                    if !candidates.is_empty() {
                        *self.context_menu_open = true;
                        *self.context_menu_pos = Some(click_pos);
                        *self.context_menu_actors = candidates;
                    }
                }
            }

            // Draw context menu if open
            if *self.context_menu_open {
                let menu_pos = self.context_menu_pos.unwrap_or_default();
                let actors = self.context_menu_actors.clone();
                let mut selected_from_menu = None;
                let mut close_menu = false;

                egui::Area::new(egui::Id::new("selection_context_menu"))
                    .fixed_pos(menu_pos)
                    .order(egui::Order::Foreground)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(30, 33, 40))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(60, 65, 75)))
                            .corner_radius(4.0)
                            .inner_margin(4.0)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("Select actor:")
                                        .small()
                                        .color(Color32::from_rgb(150, 158, 175)),
                                );
                                ui.separator();
                                for (i, actor) in actors.iter().enumerate() {
                                    let is_selected =
                                        self.selected_actor.as_ref() == Some(actor);
                                    let text = if is_selected {
                                        RichText::new(format!("● {}", actor))
                                            .color(Color32::from_rgb(84, 110, 255))
                                    } else {
                                        RichText::new(format!("  {}", actor))
                                            .color(Color32::from_rgb(200, 200, 210))
                                    };
                                    let btn = egui::Button::new(text)
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE);
                                    if ui.add_sized(
                                        [ui.available_width(), 20.0],
                                        btn,
                                    ).clicked()
                                    {
                                        selected_from_menu = Some(actor.clone());
                                        close_menu = true;
                                    }
                                    // Show index hint
                                    if i < 9 {
                                        ui.painter().text(
                                            egui::pos2(
                                                ui.max_rect().right() - 8.0,
                                                ui.min_rect().center().y,
                                            ),
                                            egui::Align2::RIGHT_CENTER,
                                            format!("{}", i + 1),
                                            egui::TextStyle::Small.resolve(ui.style()),
                                            Color32::from_rgb(100, 100, 110),
                                        );
                                    }
                                }
                                ui.separator();
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 18.0],
                                        egui::Button::new(
                                            RichText::new("Cancel")
                                                .small()
                                                .color(Color32::from_rgb(120, 120, 130)),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE),
                                    )
                                    .clicked()
                                {
                                    close_menu = true;
                                }
                            });
                    });

                if let Some(actor) = selected_from_menu {
                    self.actions.select_actor = Some(actor);
                }
                if close_menu {
                    *self.context_menu_open = false;
                }
            }

            // ── Click-to-select with cycling ────────────────────────────
            if response.clicked() && matches!(self.drag_state, DragState::None) && !*self.context_menu_open {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    let scene_point = screen_to_scene(click_pos);
                    let candidates = actors_at_point(self.hit_regions, scene_point);

                    if candidates.is_empty() {
                        *self.selected_actor = None;
                        *self.click_candidates = Vec::new();
                        *self.cycle_index = 0;
                        *self.last_click_scene = None;
                    } else {
                        // Check if this is a repeat click at the same position
                        let is_same_position = self.last_click_scene.map_or(false, |last| {
                            let dx = (scene_point.x - last.x).abs();
                            let dy = (scene_point.y - last.y).abs();
                            dx < 5.0 && dy < 5.0
                        });

                        if is_same_position && *self.click_candidates == candidates {
                            // Cycle to next candidate
                            *self.cycle_index = (*self.cycle_index + 1) % candidates.len();
                        } else {
                            // New click position, reset cycle
                            *self.click_candidates = candidates;
                            *self.cycle_index = 0;
                        }

                        *self.last_click_scene = Some(scene_point);
                        self.actions.select_actor =
                            Some(self.click_candidates[*self.cycle_index].clone());
                    }
                }
            }

            // ── Cursor feedback ─────────────────────────────────────────
            if matches!(self.drag_state, DragState::None) {
                if let Some(mouse) = pointer_pos {
                    let scene = screen_to_scene(mouse);
                    let is_over_selected = self
                        .selected_actor
                        .as_ref()
                        .and_then(|a| {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == a)
                                .map(|(_, b)| b.contains(scene))
                        })
                        .unwrap_or(false);

                    if is_over_selected {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    } else if self.hovered_actor.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            } else {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            // ── Draw hover highlight ────────────────────────────────────
            if let Some(hovered) = self.hovered_actor.as_ref() {
                // Don't draw hover if it's the same as selected
                if self.selected_actor.as_ref() != Some(hovered) {
                    if let Some(hover_rect) = preview::selection_screen_rect(
                        hovered,
                        self.hit_regions,
                        preview_rect,
                        self.scene_dimensions,
                        desired,
                    ) {
                        // Subtle dashed outline for hover
                        let hover_color = Color32::from_rgba_unmultiplied(84, 110, 255, 80);
                        let dash_len = 4.0;
                        let gap_len = 3.0;
                        let corners = [
                            hover_rect.left_top(),
                            hover_rect.right_top(),
                            hover_rect.right_bottom(),
                            hover_rect.left_bottom(),
                        ];
                        for i in 0..4 {
                            let start = corners[i];
                            let end = corners[(i + 1) % 4];
                            let total = start.distance(end);
                            let mut pos = 0.0;
                            while pos < total {
                                let t0 = pos / total;
                                let t1 = ((pos + dash_len).min(total)) / total;
                                let p0 = Pos2::new(
                                    start.x + (end.x - start.x) * t0,
                                    start.y + (end.y - start.y) * t0,
                                );
                                let p1 = Pos2::new(
                                    start.x + (end.x - start.x) * t1,
                                    start.y + (end.y - start.y) * t1,
                                );
                                ui.painter().line_segment(
                                    [p0, p1],
                                    Stroke::new(1.0, hover_color),
                                );
                                pos += dash_len + gap_len;
                            }
                        }

                        // Tooltip with actor name
                        let tooltip_pos = egui::pos2(
                            hover_rect.center().x,
                            hover_rect.top() - 20.0,
                        );
                        let galley = ui.painter().layout_no_wrap(
                            hovered.clone(),
                            egui::TextStyle::Small.resolve(ui.style()),
                            Color32::WHITE,
                        );
                        let tooltip_size = galley.size();
                        let tooltip_rect = egui::Rect::from_center_size(
                            tooltip_pos,
                            tooltip_size + Vec2::new(8.0, 4.0),
                        );
                        ui.painter().rect_filled(
                            tooltip_rect,
                            3.0,
                            Color32::from_rgba_unmultiplied(30, 33, 40, 220),
                        );
                        ui.painter().rect_stroke(
                            tooltip_rect,
                            3.0,
                            Stroke::new(1.0, Color32::from_rgb(60, 65, 75)),
                            egui::StrokeKind::Outside,
                        );
                        ui.painter().galley(
                            tooltip_rect.left_center() + Vec2::new(4.0, -tooltip_size.y / 2.0),
                            galley,
                            Color32::WHITE,
                        );
                    }
                }
            }

            // ── Draw cycle indicator ────────────────────────────────────
            if self.click_candidates.len() > 1 {
                if let Some(mouse) = pointer_pos {
                    let indicator_text = format!(
                        "{}/{}",
                        *self.cycle_index + 1,
                        self.click_candidates.len()
                    );
                    let indicator_pos = egui::pos2(mouse.x + 16.0, mouse.y - 8.0);
                    let galley = ui.painter().layout_no_wrap(
                        indicator_text,
                        egui::TextStyle::Small.resolve(ui.style()),
                        Color32::WHITE,
                    );
                    let size = galley.size();
                    let rect = egui::Rect::from_center_size(
                        indicator_pos,
                        size + Vec2::new(6.0, 3.0),
                    );
                    ui.painter().rect_filled(
                        rect,
                        3.0,
                        Color32::from_rgba_unmultiplied(84, 110, 255, 200),
                    );
                    ui.painter().galley(
                        rect.left_center() + Vec2::new(3.0, -size.y / 2.0),
                        galley,
                        Color32::WHITE,
                    );
                }
            }

            // ── Draw selection overlay with handles ─────────────────────
            if let Some(actor) = self.selected_actor.clone() {
                let is_dragging = !matches!(self.drag_state, DragState::None);
                if let Some(sel_rect) = preview::selection_screen_rect(
                    &actor,
                    self.hit_regions,
                    preview_rect,
                    self.scene_dimensions,
                    desired,
                ) {
                    preview::draw_selection_overlay(ui.painter(), sel_rect, is_dragging);
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
                        Color32::from_rgb(90, 96, 110),
                    );
                }
            }

            // Error display (compact, overlaid)
            if let Some(error) = &self.preview.error {
                let error_rect = egui::Rect::from_min_max(
                    egui::pos2(preview_rect.min.x + 8.0, preview_rect.max.y - 28.0),
                    egui::pos2(preview_rect.max.x - 8.0, preview_rect.max.y - 8.0),
                );
                ui.painter().rect_filled(error_rect, 4.0, Color32::from_rgba_unmultiplied(40, 10, 10, 200));
                ui.painter().text(
                    error_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    error,
                    egui::TextStyle::Small.resolve(ui.style()),
                    Color32::from_rgb(255, 136, 136),
                );
            }
        });
    }

    fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        let current_time_s = self.preview.current_time_s;
        inspector::inspector_ui(ui, self.timeline, self.selected_actor, current_time_s, self.actions);
    }
}
