use super::*;
use animatix::timeline::Timeline;
use preview::ActorProps;

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
    pub(super) property_edits: Vec<PropertyEdit>,
    pub(super) undo: bool,
    pub(super) redo: bool,
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
    pub(super) selection: &'a mut selection::SelectionState,
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

    // ─── Actor Property Helpers ──────────────────────────────────────────────

    /// Extract spatial properties of an actor from the timeline at the given time.
    ///
    /// Uses `Timeline::actor_world_affine` to compute the world‑space transform,
    /// which correctly accounts for `position_binding`, parent transforms,
    /// `motion_offset`, and scale — matching the renderer's transform chain.
    ///
    /// Returns `None` if the actor has no explicit size track
    /// (in which case the axis‑aligned fallback should be used for the overlay).
    fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let t = self.timeline?;
        let track = t.get_track(actor)?;
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;

        // The size track stores half‑extents (w/2, h/2).  Double to full size.
        let half = track.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];

        // Compute world‑space affine (position + rotation + scale) via the
        // same transform chain the renderer uses.
        let world_affine = t.actor_world_affine(actor, time_ms, self.scene_dimensions)?;

        // Decompose the affine:  [a, b, c, d, tx, ty]
        //   a = sx·cos(θ),  b = sx·sin(θ)
        //   c = −sy·sin(θ), d = sy·cos(θ)
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);

        // Apply uniform scale to the local size so the overlay corners land
        // at the correct world‑space positions.
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];

        Some(ActorProps { position, size, rotation })
    }

    // ─── Preview UI ──────────────────────────────────────────────────────────

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

            // ── Coordinate mapping helpers ───────────────────────────────
            let scale_x = self.scene_dimensions.width as f64 / desired.x as f64;
            let scale_y = self.scene_dimensions.height as f64 / desired.y as f64;

            let screen_to_scene = |screen: egui::Pos2| -> kurbo::Point {
                kurbo::Point::new(
                    (screen.x - preview_rect.min.x) as f64 * scale_x,
                    (screen.y - preview_rect.min.y) as f64 * scale_y,
                )
            };

            let scene_to_screen = |scene: kurbo::Point| -> egui::Pos2 {
                Pos2::new(
                    (preview_rect.min.x as f64 + scene.x / scale_x) as f32,
                    (preview_rect.min.y as f64 + scene.y / scale_y) as f32,
                )
            };

            // Get current pointer position
            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .filter(|p| preview_rect.contains(*p));

            // ── Drag interaction handling ────────────────────────────────
            let is_dragging = !matches!(self.drag_state, DragState::None);

            // Start new drag
            if response.drag_started() {
                if let (Some(actor), Some(mouse)) = (self.selected_actor.clone(), pointer_pos) {
                    let scene = screen_to_scene(mouse);
                    let props = self.get_actor_props(&actor);

                    if let Some(ref p) = props {
                        // Check scale handles (rotated)
                        let handle_world = preview::world_handle_positions(p);
                        let handle_screen: [Pos2; 8] =
                            std::array::from_fn(|i| scene_to_screen(handle_world[i]));
                        if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen) {
                            let anchor_local = preview::handle_anchor_local(idx, p.size);
                            *self.drag_state = DragState::Scale {
                                actor,
                                handle: idx,
                                start_scene: scene,
                                start_position: p.position,
                                start_size: p.size,
                                start_rotation: p.rotation,
                                anchor_local,
                                constrain_axis: preview::handle_constrains_axis(idx),
                                uniform_ratio: ui.input(|i| i.modifiers.shift),
                            };
                            // Don't process other interactions
                            return;
                        }

                        // Check rotation handle
                        let rot_world = preview::rotation_handle_world(p);
                        let rot_screen = scene_to_screen(rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen) {
                            let center = [p.position[0], p.position[1]];
                            let angle = ((scene.y - center[1] as f64) as f32)
                                .atan2((scene.x - center[0] as f64) as f32);
                            *self.drag_state = DragState::Rotate {
                                actor,
                                start_angle: angle,
                                start_rotation: p.rotation,
                                center,
                            };
                            return;
                        }
                    }

                    // Check actor body for move
                    let hit_body = props
                        .map(|p| {
                            // Point-in-rotated-rect test in local space
                            let local_pt = [
                                (scene.x - p.position[0] as f64) as f32,
                                (scene.y - p.position[1] as f64) as f32,
                            ];
                            let cos = (-p.rotation).cos();
                            let sin = (-p.rotation).sin();
                            let lx = local_pt[0] * cos - local_pt[1] * sin;
                            let ly = local_pt[0] * sin + local_pt[1] * cos;
                            let hw = p.size[0] / 2.0;
                            let hh = p.size[1] / 2.0;
                            lx.abs() <= hw && ly.abs() <= hh
                        })
                        .unwrap_or(false);

                    if hit_body
                        || self
                            .hit_regions
                            .iter()
                            .rev()
                            .any(|(label, bounds)| label == &actor && bounds.contains(scene))
                    {
                        let start_position = if let Some(ref p) = props {
                            p.position
                        } else {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == &actor)
                                .map(|(_, r)| {
                                    [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0]
                                })
                                .unwrap_or([0.0, 0.0])
                        };
                        *self.drag_state = DragState::Move {
                            actor,
                            start_scene: scene,
                            start_position,
                        };
                    }
                }
            }

            // Update ongoing drag
            if !is_dragging {
                // nothing — handled by match arms below
            } else if let Some(mouse) = pointer_pos {
                let scene = screen_to_scene(mouse);
                let shift = ui.input(|i| i.modifiers.shift);

                match self.drag_state.clone() {
                    DragState::Move {
                        actor,
                        start_scene,
                        start_position,
                    } => {
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        let (nx, ny) = if shift {
                            // Constrain to cardinal axes
                            if dx.abs() > dy.abs() {
                                (start_position[0] + dx, start_position[1])
                            } else {
                                (start_position[0], start_position[1] + dy)
                            }
                        } else {
                            (start_position[0] + dx, start_position[1] + dy)
                        };
                        self.actions.property_edits.push(PropertyEdit {
                            actor,
                            property: "position".into(),
                            value: PropertyValue::Vec2([nx, ny]),
                        });
                    }

                    DragState::Scale {
                        actor,
                        handle,
                        start_scene,
                        start_position,
                        start_size,
                        start_rotation,
                        anchor_local,
                        constrain_axis,
                        uniform_ratio,
                    } => {
                        // 1. Compute mouse delta in world space
                        let dx_world = (scene.x - start_scene.x) as f32;
                        let dy_world = (scene.y - start_scene.y) as f32;

                        // 2. Rotate delta into local space
                        let cos = (-start_rotation).cos();
                        let sin = (-start_rotation).sin();
                        let dx_local = dx_world * cos - dy_world * sin;
                        let dy_local = dx_world * sin + dy_world * cos;

                        // 3. Determine sign from handle position in local space
                        let sign = match handle {
                            0 => [-1.0_f32, -1.0], // TL: delta pulls from top-left
                            1 => [1.0, -1.0],  // TR
                            2 => [1.0, 1.0],   // BR
                            3 => [-1.0, 1.0],  // BL
                            4 => [0.0, -1.0],  // top-mid
                            5 => [1.0, 0.0],   // right-mid
                            6 => [0.0, 1.0],   // bottom-mid
                            7 => [-1.0, 0.0],  // left-mid
                            _ => [1.0, 1.0],
                        };

                        let min_size = 10.0;
                        let mut new_w = start_size[0];
                        let mut new_h = start_size[1];

                        if sign[0] != 0.0 {
                            new_w = (start_size[0] + sign[0] * 2.0 * dx_local).max(min_size);
                        }
                        if sign[1] != 0.0 {
                            new_h = (start_size[1] + sign[1] * 2.0 * dy_local).max(min_size);
                        }

                        // Uniform ratio (shift or stored from drag start)
                        let uniform = shift || uniform_ratio;
                        if uniform {
                            if constrain_axis {
                                // Edge midpoint: scale the free axis, then derive the
                                // constrained axis from the original aspect ratio.
                                let ratio = start_size[0] / start_size[1].max(1.0);
                                if sign[0] == 0.0 {
                                    // Dragging top/bottom edge — width derives from height
                                    new_w = (new_h * ratio).max(min_size);
                                } else {
                                    // Dragging left/right edge — height derives from width
                                    new_h = (new_w / ratio).max(min_size);
                                }
                            } else {
                                // Corner: use the dominant axis and scale both proportionally
                                let scale_w = new_w / start_size[0].max(1.0);
                                let scale_h = new_h / start_size[1].max(1.0);
                                let s = scale_w.max(scale_h);
                                new_w = (start_size[0] * s).max(min_size);
                                new_h = (start_size[1] * s).max(min_size);
                            }
                        }

                        // 4. Adjust position to keep anchor fixed in world space
                        let cos_rot = start_rotation.cos();
                        let sin_rot = start_rotation.sin();
                        // anchor_local is in local space based on start_size
                        // but anchor_local in local coords is a fraction of size (e.g. (-0.5*w, -0.5*h))
                        // After scaling, the anchor's local position changes.
                        let old_anchor_local = [anchor_local[0], anchor_local[1]];
                        let new_anchor_local = [
                            old_anchor_local[0] * new_w / start_size[0].max(1.0),
                            old_anchor_local[1] * new_h / start_size[1].max(1.0),
                        ];

                        // Anchor world position (fixed)
                        let anchor_world_x = start_position[0]
                            + old_anchor_local[0] * cos_rot
                            - old_anchor_local[1] * sin_rot;
                        let anchor_world_y = start_position[1]
                            + old_anchor_local[0] * sin_rot
                            + old_anchor_local[1] * cos_rot;

                        // New position
                        let new_pos_x = anchor_world_x
                            - new_anchor_local[0] * cos_rot
                            + new_anchor_local[1] * sin_rot;
                        let new_pos_y = anchor_world_y
                            - new_anchor_local[0] * sin_rot
                            - new_anchor_local[1] * cos_rot;

                        self.actions.property_edits.push(PropertyEdit {
                            actor: actor.clone(),
                            property: "size".into(),
                            value: PropertyValue::Vec2([new_w, new_h]),
                        });
                        self.actions.property_edits.push(PropertyEdit {
                            actor,
                            property: "position".into(),
                            value: PropertyValue::Vec2([new_pos_x, new_pos_y]),
                        });
                    }

                    DragState::Rotate {
                        actor,
                        start_angle,
                        start_rotation,
                        center,
                    } => {
                        let angle = ((scene.y - center[1] as f64) as f32)
                            .atan2((scene.x - center[0] as f64) as f32);
                        let mut delta = angle - start_angle;
                        // Normalize delta to [-π, π]
                        while delta > std::f32::consts::PI {
                            delta -= 2.0 * std::f32::consts::PI;
                        }
                        while delta < -std::f32::consts::PI {
                            delta += 2.0 * std::f32::consts::PI;
                        }
                        let mut new_rot = start_rotation + delta;
                        if shift {
                            let step = std::f32::consts::PI / 12.0; // 15°
                            new_rot = (new_rot / step).round() * step;
                        }
                        self.actions.property_edits.push(PropertyEdit {
                            actor,
                            property: "rotation".into(),
                            value: PropertyValue::Float(new_rot),
                        });
                    }
                    DragState::None => {}
                }
            }

            // End drag
            if is_dragging
                && (response.drag_stopped()
                    || (!response.dragged() && is_dragging))
            {
                *self.drag_state = DragState::None;
            }

            // ── Hover preview ───────────────────────────────────────────
            selection::update_hover(
                self.selection,
                self.hit_regions,
                pointer_pos,
                screen_to_scene,
                is_dragging,
            );

            // ── Right-click context menu ────────────────────────────────
            if response.secondary_clicked() && !is_dragging {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    selection::handle_right_click(
                        self.selection,
                        self.hit_regions,
                        click_pos,
                        screen_to_scene,
                    );
                }
            }

            // Draw context menu if open
            if self.selection.context_menu_open {
                let (selected, close) = selection::draw_context_menu(
                    ui,
                    self.selection,
                    self.selected_actor,
                );
                if let Some(actor) = selected {
                    self.actions.select_actor = Some(actor);
                }
                if close {
                    self.selection.context_menu_open = false;
                }
            }

            // ── Click-to-select with cycling ────────────────────────────
            if response.clicked()
                && !is_dragging
                && !self.selection.context_menu_open
            {
                if let Some(click_pos) = response.interact_pointer_pos() {
                    let selected = selection::handle_click(
                        self.selection,
                        self.hit_regions,
                        click_pos,
                        screen_to_scene,
                    );
                    if let Some(actor) = selected {
                        self.actions.select_actor = Some(actor);
                    } else {
                        *self.selected_actor = None;
                    }
                }
            }

            // ── Cursor feedback ─────────────────────────────────────────
            if !is_dragging && !self.selection.context_menu_open {
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
                    } else if self.selection.hovered_actor.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            } else if !self.selection.context_menu_open {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            // ── Draw hover highlight ────────────────────────────────────
            if let Some(hovered) = self.selection.hovered_actor.as_ref() {
                if self.selected_actor.as_ref() != Some(hovered) {
                    if let Some(hover_rect) = preview::selection_screen_rect(
                        hovered,
                        self.hit_regions,
                        preview_rect,
                        self.scene_dimensions,
                        desired,
                    ) {
                        selection::draw_hover_highlight(ui.painter(), hovered, hover_rect);
                    }
                }
            }

            // ── Draw cycle indicator ────────────────────────────────────
            if let Some(mouse) = pointer_pos {
                selection::draw_cycle_indicator(
                    ui.painter(),
                    mouse,
                    self.selection.cycle_index,
                    self.selection.click_candidates.len(),
                );
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

            // ── Draw selection overlay with handles (AFTER preview texture) ──
            if let Some(actor) = self.selected_actor.as_ref() {
                let props = self.get_actor_props(actor);
                let fallback = preview::selection_screen_rect(
                    actor,
                    self.hit_regions,
                    preview_rect,
                    self.scene_dimensions,
                    desired,
                );
                preview::draw_selection_overlay(
                    ui.painter(),
                    props.as_ref(),
                    fallback,
                    is_dragging,
                    preview_rect,
                    self.scene_dimensions,
                    desired,
                );
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
