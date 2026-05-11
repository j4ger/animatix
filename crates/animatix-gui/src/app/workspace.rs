use super::*;
use crate::app::components;
use crate::app::theme::*;
use animatix::timeline::{AnimationTrack, PlacementMode, PositionBinding, ShapeType, Timeline, TrackAccessor};
use preview::ActorProps;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    Explorer,
    Layers,
}

/// Describes a property edit made in the inspector panel.
#[derive(Debug, Clone)]
pub(super) struct PropertyEdit {
    pub(super) actor: String,
    pub(super) property: String,
    pub(super) value: PropertyValue,
    /// When true, create a keyframe at current time instead of overwriting defaults.
    pub(super) create_keyframe: bool,
}

/// The typed value of a property edit.
#[derive(Debug, Clone)]
pub(crate) enum PropertyValue {
    Vec2([f32; 2]),
    Float(f32),
    Color([f32; 4]),
    Text(String),
    StringList(Vec<String>),
}

impl From<PropertyValue> for animatix::ast::Expr {
    fn from(pv: PropertyValue) -> Self {
        match pv {
            PropertyValue::Vec2([x, y]) => {
                animatix::ast::Expr::Tuple(vec![
                    animatix::ast::Expr::Num(x as f64),
                    animatix::ast::Expr::Num(y as f64),
                ])
            }
            PropertyValue::Float(v) => animatix::ast::Expr::Num(v as f64),
            PropertyValue::Color([r, g, b, a]) => {
                if (a - 1.0).abs() < 0.001
                    && r.fract() == 0.0
                    && g.fract() == 0.0
                    && b.fract() == 0.0
                {
                    // Opaque integer color — use rgb() shorthand.
                    animatix::ast::Expr::Call(
                        "rgb".into(),
                        vec![
                            animatix::ast::Expr::Num((r * 255.0) as i64 as f64),
                            animatix::ast::Expr::Num((g * 255.0) as i64 as f64),
                            animatix::ast::Expr::Num((b * 255.0) as i64 as f64),
                        ],
                    )
                } else {
                    animatix::ast::Expr::Call(
                        "rgba".into(),
                        vec![
                            animatix::ast::Expr::Num(r as f64),
                            animatix::ast::Expr::Num(g as f64),
                            animatix::ast::Expr::Num(b as f64),
                            animatix::ast::Expr::Num(a as f64),
                        ],
                    )
                }
            }
            PropertyValue::Text(s) => animatix::ast::Expr::Str(s),
            PropertyValue::StringList(items) => {
                animatix::ast::Expr::Tuple(items.into_iter().map(animatix::ast::Expr::Ident).collect())
            }
        }
    }
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
    pub(super) drag_ended: bool,
    pub(super) undo: bool,
    pub(super) redo: bool,
    pub(super) toggle_editor_sync: bool,
    pub(super) toggle_keyframe_mode: bool,
    /// Scroll editor to this 0-indexed line (set by clicking a diagnostic).
    pub(super) scroll_to_line: Option<usize>,
    /// True when an inspector DragValue/Slider drag started this frame.
    pub(super) inspector_input_drag_started: bool,
    /// True when an inspector DragValue/Slider drag ended this frame.
    pub(super) inspector_input_drag_ended: bool,
    /// Toggle the bottom diagnostics panel visibility.
    pub(super) toggle_diagnostics_panel: bool,
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
    /// When true, property edits should create keyframes instead of overwriting defaults.
    pub(super) keyframe_mode: bool,
    /// Actor labels that the user has explicitly collapsed in the layer tree.
    pub(super) collapsed_actors: &'a mut HashSet<String>,
}

/// Uniform panel frame: 8 px padding, transparent fill.
fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

fn render_sidebar_tab_bar(ui: &mut egui::Ui, active_tab: &mut SidebarTab) {
    let tabs = [
        (SidebarTab::Explorer, egui_phosphor::regular::FOLDER, "Explorer"),
        (SidebarTab::Layers, egui_phosphor::regular::STACK, "Layers"),
    ];
    if let Some(new_tab) = widgets::pill_tab_bar(ui, *active_tab, &tabs) {
        *active_tab = new_tab;
    }
}

impl WorkspaceViewer<'_> {
    pub(super) fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let tab_id = ui.id().with("sidebar_tab");
            let mut active_tab = ui
                .data(|d| d.get_temp::<SidebarTab>(tab_id))
                .unwrap_or(SidebarTab::Explorer);

            render_sidebar_tab_bar(ui, &mut active_tab);
            ui.add_space(6.0);

            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::top_down(egui::Align::Min),
                |ui| match active_tab {
                    SidebarTab::Explorer => self.explorer_content_ui(ui),
                    SidebarTab::Layers => self.layers_content_ui(ui),
                },
            );

            ui.data_mut(|d| d.insert_temp(tab_id, active_tab));
        });
    }

    fn explorer_content_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            for entry in self.file_tree {
                let is_selected = !entry.is_dir && entry.path == self.current_file;
                let is_expanded = entry.is_dir && self.expanded_dirs.contains(&entry.path);
                let has_children = entry.is_dir;

                let (icon, label_color) = if entry.is_dir {
                    let folder_icon = if is_expanded {
                        egui_phosphor::regular::FOLDER_OPEN
                    } else {
                        egui_phosphor::regular::FOLDER
                    };
                    (Some(folder_icon), None)
                } else {
                    let is_amx = entry.path.extension().and_then(|e| e.to_str()) == Some("amx");
                    let file_icon = if is_amx {
                        egui_phosphor::regular::FILM_STRIP
                    } else {
                        egui_phosphor::regular::FILE
                    };
                    let color = if is_amx { Some(ACCENT_BLUE) } else { None };
                    (Some(file_icon), color)
                };

                let row_id = ui.id().with(entry.path.display().to_string());
                let path = entry.path.clone();
                let is_dir = entry.is_dir;
                let clicked = widgets::tree_row(
                    ui,
                    row_id,
                    entry.depth,
                    has_children,
                    is_expanded,
                    is_selected,
                    icon,
                    &entry.name,
                    label_color,
                    || {
                        self.actions.toggle_expand_dir = Some(path.clone());
                    },
                );

                if clicked {
                    if is_dir {
                        self.actions.toggle_expand_dir = Some(path);
                    } else {
                        self.actions.open_file = Some(path);
                    }
                }
            }
        });
    }

    fn layers_content_ui(&mut self, ui: &mut egui::Ui) {
        let Some(timeline) = self.timeline else {
            ui.vertical_centered(|ui| {
                ui.add_space(36.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(egui_phosphor::regular::FILM_STRIP)
                            .size(28.0)
                            .color(Color32::from_rgb(90, 96, 110)),
                    )
                    .selectable(false),
                );
                ui.add_space(10.0);
                ui.add(
                    egui::Label::new(
                        RichText::new("No timeline loaded")
                            .size(12.0)
                            .color(Color32::from_rgb(150, 158, 175)),
                    )
                    .selectable(false),
                );
            });
            return;
        };

        let root_nodes = timeline.root_actor_labels();
        if root_nodes.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(36.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(egui_phosphor::regular::FILM_STRIP)
                            .size(28.0)
                            .color(Color32::from_rgb(90, 96, 110)),
                    )
                    .selectable(false),
                );
                ui.add_space(10.0);
                ui.add(
                    egui::Label::new(
                        RichText::new("No actors in scene")
                            .size(12.0)
                            .color(Color32::from_rgb(150, 158, 175)),
                    )
                    .selectable(false),
                );
            });
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for root_label in root_nodes {
                    render_actor_tree(
                        ui,
                        timeline,
                        root_label,
                        self.selected_actor,
                        self.collapsed_actors,
                        0,
                    );
                }
            });
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            self.editor.set_diagnostics(self.diagnostics);
            let response = self.editor.show(ui);
            if response.changed() || self.editor.text() != self.source_dirty.as_str() {
                *self.source_dirty = self.editor.text().to_string();
                self.actions.editor_changed = true;
            }
            if let Some(time_s) = self.editor.pending_scrub_to_time.take() {
                self.actions.scrub_to = Some(time_s);
                if !self.preview.is_playing {
                    self.actions.toggle_playback = true;
                }
            }
            if let Some(line) = self.actions.scroll_to_line.take() {
                self.editor.scroll_to_line(line);
            }
        });
    }

    // ─── Actor Property Helpers ──────────────────────────────────────────────

    /// Check whether the actor is currently layout-managed by a parent container
    /// (Row / Col / Grid / Stack).  Layout-managed actors have their position
    /// computed by the layout engine, so drag-to-move is replaced by drag-to-reorder.
    fn is_layout_managed(&self, actor: &str) -> bool {
        let Some(t) = self.timeline else { return false };
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        preview::is_layout_managed(actor, t, time_ms)
    }

    fn find_layout_container(
        &self,
        actor_label: &str,
    ) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline?;
        for (container_label, metadata) in &timeline.container_metadata {
            if let Some((idx, _)) = metadata
                .layout_children
                .iter()
                .enumerate()
                .find(|(_, child)| child.label == actor_label)
            {
                return Some((container_label.clone(), metadata.layout_type, idx));
            }
        }
        None
    }

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

    pub(super) fn preview_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
        ui.vertical(|ui| {
            // Minimal header
            let header_avail = ui.available_width();
            let header_h = ROW_S;
            let (header_rect, _) = ui.allocate_exact_size(Vec2::new(header_avail, header_h), egui::Sense::hover());
            let baseline_y = header_rect.center().y;

            ui.painter().text(
                egui::pos2(header_rect.min.x, baseline_y),
                egui::Align2::LEFT_CENTER,
                "Preview",
                egui::FontId::new(12.0, egui::FontFamily::Proportional),
                Color32::from_rgb(150, 158, 175),
            );

            // Status badge (right-aligned)
            let (badge_label, badge_fill, badge_text) = if self.preview.is_playing {
                ("Playing", Color32::from_rgb(46, 106, 80), Color32::from_rgb(216, 249, 235))
            } else {
                ("Paused", Color32::from_rgb(40, 44, 52), Color32::from_rgb(150, 158, 175))
            };
            let badge_w = badge_label.len() as f32 * 7.0 + 16.0;
            let badge_rect = egui::Rect::from_min_size(
                egui::pos2(header_rect.max.x - badge_w, header_rect.min.y + 2.0),
                Vec2::new(badge_w, header_h - 4.0),
            );
            ui.painter().rect_filled(badge_rect, 6.0, badge_fill);
            ui.painter().text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                badge_label,
                egui::FontId::new(11.0, egui::FontFamily::Proportional),
                badge_text,
            );

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

            // Get current pointer position (clamped to preview rect for scene-space work)
            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .filter(|p| preview_rect.contains(*p));

            // Unclamped pointer position for handle hit-testing — handles may
            // extend beyond the scene boundary and must remain interactive.
            let raw_pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos());

            // ── Drag interaction handling ────────────────────────────────
            let is_dragging = !matches!(self.drag_state, DragState::None);

            // Detect drag start — either from egui (pointer inside widget rect)
            // or manually when pointer is over a handle outside the widget rect.
            let drag_started = response.drag_started()
                || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));

            // Start new drag
            if drag_started {
                if let (Some(actor), Some(mouse)) = (self.selected_actor.clone(), raw_pointer_pos) {
                    let scene = screen_to_scene(mouse);
                    let props = self.get_actor_props(&actor);

                    if let Some(ref p) = props {
                        // Check scale handles (rotated)
                        let handle_world = preview::world_handle_positions(p);
                        let handle_screen: [Pos2; 8] =
                            std::array::from_fn(|i| scene_to_screen(handle_world[i]));
                        if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen) {
                            let anchor_local = preview::handle_anchor_local(idx, p.size);
                            let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                            let (resize_mode, start_scale) = self
                                .timeline
                                .and_then(|t| t.get_track(&actor))
                                .map(|tr| {
                                    let mode = match tr.kind {
                                        animatix::timeline::ActorKindId::Text
                                        | animatix::timeline::ActorKindId::Math
                                        | animatix::timeline::ActorKindId::Code
                                        | animatix::timeline::ActorKindId::Graph
                                        | animatix::timeline::ActorKindId::CartesianPlot
                                        | animatix::timeline::ActorKindId::PolarPlot
                                        | animatix::timeline::ActorKindId::ParametricPlot
                                        | animatix::timeline::ActorKindId::ImplicitPlot => {
                                            preview::ResizeMode::Scale
                                        }
                                        _ => preview::ResizeMode::Size,
                                    };
                                    (mode, tr.scale.get(time_ms, 1.0))
                                })
                                .unwrap_or((preview::ResizeMode::Size, 1.0));
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
                                resize_mode,
                                start_scale,
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
                        // Layout-managed actors reorder within their parent container.
                        if self.is_layout_managed(&actor) {
                            if let Some((container, layout_type, source_index)) =
                                self.find_layout_container(&actor)
                            {
                                *self.drag_state = DragState::Reorder {
                                    actor,
                                    container,
                                    source_index,
                                    target_index: source_index,
                                    start_mouse: scene,
                                    layout_type,
                                };
                                return;
                            }
                            return;
                        }

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
            } else if let Some(mouse) = raw_pointer_pos {
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

                        // Determine which source property to edit based on the
                        // actor's position binding.
                        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                        let binding = self
                            .timeline
                            .and_then(|t| t.get_track(&actor))
                            .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                            .unwrap_or(PositionBinding::Absolute);

                        match binding {
                            PositionBinding::SceneAnchor { anchor, .. } => {
                                // Actor is anchored to a scene point.  Keep the
                                // anchor, update the pixel offset so the actor
                                // ends up at the dragged world position.
                                let anchor_pt = animatix::timeline::scene_anchor_point(
                                    anchor,
                                    self.scene_dimensions,
                                );
                                let new_offset = [
                                    nx - anchor_pt.x as f32,
                                    ny - anchor_pt.y as f32,
                                ];
                                self.actions.property_edits.push(PropertyEdit {
                                    actor,
                                    property: "offset".into(),
                                    value: PropertyValue::Vec2(new_offset),
                                    create_keyframe: self.keyframe_mode,
                                });
                            }
                            PositionBinding::ScenePercent { .. } => {
                                // Percent-based: convert world position back to
                                // percentages of scene dimensions.
                                let w = self.scene_dimensions.width.max(1) as f32;
                                let h = self.scene_dimensions.height.max(1) as f32;
                                self.actions.property_edits.push(PropertyEdit {
                                    actor,
                                    property: "at".into(),
                                    value: PropertyValue::Vec2([nx / w, ny / h]),
                                create_keyframe: self.keyframe_mode,
                                });
                            }
                            _ => {
                                // Absolute / ContainerDefault / ContainerPercent /
                                // no binding → edit `at` (which the source writer
                                // maps to the `at:` property).
                                self.actions.property_edits.push(PropertyEdit {
                                    actor,
                                    property: "position".into(),
                                    value: PropertyValue::Vec2([nx, ny]),
                                create_keyframe: self.keyframe_mode,
                                });
                            }
                        }
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
                        resize_mode,
                        start_scale,
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
                            new_w = (start_size[0] + sign[0] * dx_local).max(min_size);
                        }
                        if sign[1] != 0.0 {
                            new_h = (start_size[1] + sign[1] * dy_local).max(min_size);
                        }

                        // For actors with auto-measured bounds (text, plots), all
                        // handle drags act as uniform scale of the entire block.
                        let force_uniform = resize_mode == preview::ResizeMode::Scale;
                        let uniform = shift || uniform_ratio || force_uniform;
                        if uniform {
                            // Use the dominant axis and scale both proportionally
                            let scale_w = new_w / start_size[0].max(1.0);
                            let scale_h = new_h / start_size[1].max(1.0);
                            let s = if constrain_axis && !force_uniform {
                                // Edge midpoint: scale the free axis, derive the other
                                // from the original aspect ratio.
                                let ratio = start_size[0] / start_size[1].max(1.0);
                                if sign[0] == 0.0 {
                                    // Dragging top/bottom — width derives from height
                                    scale_h
                                } else {
                                    // Dragging left/right — height derives from width
                                    scale_w
                                }
                            } else {
                                scale_w.max(scale_h)
                            };
                            new_w = (start_size[0] * s).max(min_size);
                            new_h = (start_size[1] * s).max(min_size);
                        }

                        // 4. Adjust position to keep anchor fixed in world space
                        let cos_rot = start_rotation.cos();
                        let sin_rot = start_rotation.sin();
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

                        // Emit the appropriate property edit
                        if resize_mode == preview::ResizeMode::Scale {
                            let ratio = new_w / start_size[0].max(1.0);
                            let new_scale = (start_scale * ratio).max(0.01);
                            self.actions.property_edits.push(PropertyEdit {
                                actor: actor.clone(),
                                property: "scale".into(),
                                value: PropertyValue::Float(new_scale),
                                create_keyframe: self.keyframe_mode,
                            });
                        } else {
                            self.actions.property_edits.push(PropertyEdit {
                                actor: actor.clone(),
                                property: "size".into(),
                                value: PropertyValue::Vec2([new_w, new_h]),
                                create_keyframe: self.keyframe_mode,
                            });
                        }

                        // Route the position adjustment through the same
                        // binding-aware logic as move-drag.
                        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                        let binding = self
                            .timeline
                            .and_then(|t| t.get_track(&actor))
                            .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                            .unwrap_or(PositionBinding::Absolute);

                        match binding {
                            PositionBinding::SceneAnchor { anchor, .. } => {
                                let anchor_pt = animatix::timeline::scene_anchor_point(
                                    anchor,
                                    self.scene_dimensions,
                                );
                                let new_offset = [
                                    new_pos_x - anchor_pt.x as f32,
                                    new_pos_y - anchor_pt.y as f32,
                                ];
                                self.actions.property_edits.push(PropertyEdit {
                                    actor,
                                    property: "offset".into(),
                                    value: PropertyValue::Vec2(new_offset),
                                    create_keyframe: self.keyframe_mode,
                                });
                            }
                            PositionBinding::ScenePercent { .. } => {
                                let w = self.scene_dimensions.width.max(1) as f32;
                                let h = self.scene_dimensions.height.max(1) as f32;
                                self.actions.property_edits.push(PropertyEdit {
                                    actor,
                                    property: "at".into(),
                                    value: PropertyValue::Vec2([new_pos_x / w, new_pos_y / h]),
                                create_keyframe: self.keyframe_mode,
                                });
                            }
                            _ => {
                                self.actions.property_edits.push(PropertyEdit {
                                    actor,
                                    property: "position".into(),
                                    value: PropertyValue::Vec2([new_pos_x, new_pos_y]),
                                create_keyframe: self.keyframe_mode,
                                });
                            }
                        }
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
                        create_keyframe: self.keyframe_mode,
                        });
                    }
                    DragState::Reorder {
                        actor,
                        container,
                        source_index: _,
                        target_index: _,
                        start_mouse: _,
                        layout_type,
                    } => {
                        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                        if let Some(timeline) = self.timeline {
                            let order = timeline.get_child_order(&container, time_ms);
                            let siblings: Vec<String> = order.into_iter().filter(|l| l != &actor).collect();
                            let positions: Vec<f32> = siblings
                                .iter()
                                .map(|label| {
                                    self.hit_regions
                                        .iter()
                                        .find(|(l, _)| l == label)
                                        .map(|(_, bounds)| {
                                            if layout_type == animatix::timeline::LayoutType::Row {
                                                (bounds.x0 + bounds.x1) as f32 / 2.0
                                            } else {
                                                (bounds.y0 + bounds.y1) as f32 / 2.0
                                            }
                                        })
                                        .or_else(|| {
                                            self.get_actor_props(label).map(|p| {
                                                if layout_type == animatix::timeline::LayoutType::Row {
                                                    p.position[0]
                                                } else {
                                                    p.position[1]
                                                }
                                            })
                                        })
                                        .unwrap_or(if layout_type == animatix::timeline::LayoutType::Row {
                                            scene.x as f32
                                        } else {
                                            scene.y as f32
                                        })
                                })
                                .collect();

                            let mouse_coord = if layout_type == animatix::timeline::LayoutType::Row {
                                scene.x as f32
                            } else {
                                scene.y as f32
                            };

                            let mut insert_at = positions.len();
                            for (idx, coord) in positions.iter().enumerate() {
                                if mouse_coord < *coord {
                                    insert_at = idx;
                                    break;
                                }
                            }
                            if let DragState::Reorder { target_index, .. } = &mut *self.drag_state {
                                *target_index = insert_at;
                            }
                        }
                    }
                    DragState::None => {}
                }
            }

            // End drag — signal via actions so handle_actions can process
            // the final frame's property edits while drag_state is still active.
            let pointer_released = ui.input(|i| i.pointer.any_released());
            if is_dragging
                && (response.drag_stopped()
                    || pointer_released
                    || (!ui.input(|i| i.pointer.any_down()) && is_dragging))
            {
                if let DragState::Reorder {
                    actor,
                    container,
                    source_index,
                    target_index,
                    ..
                } = self.drag_state.clone()
                {
                    if source_index != target_index {
                        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                        if let Some(timeline) = self.timeline {
                            let mut new_order = timeline.get_child_order(&container, time_ms);
                            if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                                let item = new_order.remove(pos);
                                let insert_at = target_index.min(new_order.len());
                                new_order.insert(insert_at, item);
                                self.actions.property_edits.push(PropertyEdit {
                                    actor: container,
                                    property: "child_order".into(),
                                    value: PropertyValue::StringList(new_order),
                                    create_keyframe: self.keyframe_mode,
                                });
                            }
                        }
                    }
                }
                self.actions.drag_ended = true;
            }

            // ── Hover preview ───────────────────────────────────────────
            // Disable hover when context menu is open to avoid stray tooltips
            // and highlights behind the menu.
            if !self.selection.context_menu_open {
                selection::update_hover(
                    self.selection,
                    self.hit_regions,
                    pointer_pos,
                    screen_to_scene,
                    is_dragging,
                );
            } else {
                self.selection.hovered_actor = None;
            }

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
            let mut menu_item_clicked = false;
            if self.selection.context_menu_open {
                let (selected, close, _rect) = selection::draw_context_menu(
                    ui,
                    self.selection,
                    self.selected_actor,
                );
                menu_item_clicked = close;
                if let Some(actor) = selected {
                    self.actions.select_actor = Some(actor);
                }
                if close {
                    self.selection.context_menu_open = false;
                }
            }

            // Any primary click that didn't hit a menu item closes the menu
            // (click on empty preview space, menu background, or outside bounds).
            let mut suppress_click = false;
            if self.selection.context_menu_open && !menu_item_clicked {
                if ui.input(|i| i.pointer.primary_clicked()) {
                    self.selection.context_menu_open = false;
                    suppress_click = true;
                    *self.selected_actor = None;
                }
            }

            // ── Click-to-select with cycling ────────────────────────────
            if response.clicked()
                && !is_dragging
                && !self.selection.context_menu_open
                && !suppress_click
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
                if let Some(mouse) = raw_pointer_pos {
                    let scene = screen_to_scene(mouse);

                    // Check scale/rotation handles first (higher priority than body)
                    let over_handle = self.selected_actor.as_ref().and_then(|a| {
                        let props = self.get_actor_props(a)?;
                        let handle_world = preview::world_handle_positions(&props);
                        let handle_screen: [Pos2; 8] =
                            std::array::from_fn(|i| scene_to_screen(handle_world[i]));
                        if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen) {
                            Some(idx)
                        } else {
                            let rot_world = preview::rotation_handle_world(&props);
                            let rot_screen = scene_to_screen(rot_world);
                            if preview::hit_test_rotation_handle(mouse, rot_screen) {
                                Some(8usize) // sentinel for rotation handle
                            } else {
                                None
                            }
                        }
                    });

                    if let Some(handle_idx) = over_handle {
                        let icon = match handle_idx {
                            0 => egui::CursorIcon::ResizeNwSe,  // TL
                            1 => egui::CursorIcon::ResizeNeSw,  // TR
                            2 => egui::CursorIcon::ResizeNwSe,  // BR
                            3 => egui::CursorIcon::ResizeNeSw,  // BL
                            4 => egui::CursorIcon::ResizeVertical,   // top-mid
                            5 => egui::CursorIcon::ResizeHorizontal, // right-mid
                            6 => egui::CursorIcon::ResizeVertical,   // bottom-mid
                            7 => egui::CursorIcon::ResizeHorizontal, // left-mid
                            _ => egui::CursorIcon::Crosshair,        // rotation
                        };
                        ui.ctx().set_cursor_icon(icon);
                    } else {
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
                }
            } else if !self.selection.context_menu_open {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }

            // ── Draw hover highlight ────────────────────────────────────
            // Skip when context menu is open to avoid stray highlights / tooltips
            // behind the menu.
            if !self.selection.context_menu_open {
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
                if let DragState::Reorder {
                    actor: drag_actor,
                    container,
                    target_index,
                    layout_type,
                    ..
                } = self.drag_state.clone()
                {
                    if &drag_actor == actor {
                        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                        if let Some(timeline) = self.timeline {
                            let order = timeline.get_child_order(&container, time_ms);
                            let siblings: Vec<(String, [f32; 2])> = order
                                .into_iter()
                                .filter(|label| label != actor)
                                .filter_map(|label| {
                                    self.get_actor_props(&label)
                                        .map(|p| (label, p.position))
                                })
                                .collect();
                            if let Some(props) = props.as_ref() {
                                preview::draw_reorder_overlay(
                                    ui.painter(),
                                    props,
                                    target_index,
                                    &siblings,
                                    preview_rect,
                                    self.scene_dimensions,
                                    desired,
                                    layout_type == animatix::timeline::LayoutType::Row,
                                );
                            }
                        }
                    }
                }
            }

            // NOTE: errors are shown in the diagnostics banner above the canvas,
            // not as an overlay, to avoid duplicating the same message.
        });
        });
    }

    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let current_time_s = self.preview.current_time_s;
            inspector::inspector_ui(ui, self.timeline, self.selected_actor, current_time_s, self.actions, self.keyframe_mode);
        });
    }
}

// ─── Layer Tree ─────────────────────────────────────────────────────────────

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actor: &mut Option<String>,
    collapsed_actors: &mut HashSet<String>,
    depth: usize,
) {
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actor.as_deref() == Some(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = !track.children.is_empty();
    let is_expanded = has_children && !collapsed_actors.contains(label);

    let (icon, display_label, label_color) = if is_anonymous {
        (
            Some(egui_phosphor::regular::GHOST),
            "anon",
            Some(TEXT_MUTED),
        )
    } else {
        let shape_icon = track
            .shape_type
            .as_ref()
            .map(|pt| shape_icon(pt.evaluate(0)));
        (shape_icon, label, None)
    };

    // Use the unique actor label (not display_label) for the row id
    // so that multiple anonymous actors don't share the same id.
    let row_id = ui.id().with(label);
    let label_owned = label.to_string();
    let response = components::Row::new(display_label)
        .indent(depth as f32 * 14.0)
        .selected(is_selected)
        .icon(icon)
        .label_color(label_color.unwrap_or(TEXT_SECONDARY))
        .has_children(has_children)
        .expanded(is_expanded)
        .show(ui, row_id);

    if response.chevron_clicked {
        if collapsed_actors.contains(&label_owned) {
            collapsed_actors.remove(&label_owned);
        } else {
            collapsed_actors.insert(label_owned.clone());
        }
    }

    if response.row_clicked {
        *selected_actor = Some(label.to_string());
    }

    // Children
    if is_expanded {
        for child_label in &track.children {
            render_actor_tree(
                ui,
                timeline,
                child_label,
                selected_actor,
                collapsed_actors,
                depth + 1,
            );
        }
    }
}

fn shape_icon(shape: ShapeType) -> &'static str {
    match shape {
        ShapeType::Rect => egui_phosphor::regular::SQUARE,
        ShapeType::Circle => egui_phosphor::regular::CIRCLE,
        ShapeType::Line => egui_phosphor::regular::MINUS,
        ShapeType::Ellipse => egui_phosphor::regular::CIRCLE_NOTCH,
        ShapeType::Arc => egui_phosphor::regular::ARROWS_CLOCKWISE,
        ShapeType::Polygon => egui_phosphor::regular::HEXAGON,
        ShapeType::Path => egui_phosphor::regular::PEN,
        ShapeType::Arrow => egui_phosphor::regular::ARROW_RIGHT,
        ShapeType::Graph => egui_phosphor::regular::CHART_BAR,
        ShapeType::Plot => egui_phosphor::regular::DOTS_THREE_OUTLINE,
    }
}
