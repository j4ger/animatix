//! PreviewContext: shared state and helper methods for the preview canvas.
//!
//! The struct lives here so it can be imported by both the main preview panel
//! UI function (`preview_panel_ui`) and the extracted drag handler.

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Vec2};

use crate::app::commands::{
    ActionQueue, DocumentCommand, PropertyEdit, PropertyValue as GuiPropertyValue, SceneCommand,
};
use crate::app::design_tokens::semantic::accent;
use crate::app::design_tokens::semantic::status;
use crate::app::design_tokens::semantic::surface;
use crate::app::design_tokens::semantic::text;
use crate::app::design_tokens::spatial::preview::{
    HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS, MIN_ZOOM as PREVIEW_MIN_ZOOM,
};
use crate::app::design_tokens::spatial::{RADIUS_M, SPACE_S, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::preview::performance::PerformanceMetrics;
use crate::app::preview::{self, ActorProps, DragState, selection};
use crate::app::{InlineTextEditState, PreviewPaneState};
use animatix::timeline::{ActorKindId, SceneDimensions, Timeline};
use egui::Stroke;

pub(crate) struct PreviewContext<'a> {
    pub scene_dimensions: SceneDimensions,
    pub preview: &'a mut PreviewPaneState,
    pub preview_texture_id: Option<egui::TextureId>,
    pub commands: &'a mut ActionQueue,
    pub drag_state: &'a mut DragState,
    pub selection: &'a mut selection::SelectionState,
    pub selected_actors: &'a mut HashSet<String>,
    pub hit_regions: &'a [(String, kurbo::Rect)],
    pub timeline: Option<&'a Timeline>,
    pub pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub tool_mode: &'a mut preview::ToolMode,
    pub rotation_snap_degrees: f32,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub keyframe_mode: bool,
    /// Performance metrics for the HUD overlay.
    pub performance_metrics: &'a mut PerformanceMetrics,
    /// Show layout debug overlay (container labels, slot outlines, sizes).
    pub debug_layout: bool,
    /// Show padding/gap regions as overlay.
    pub debug_spacing: bool,
}

// ─── Helper methods ─────────────────────────────────────────────────────────

impl PreviewContext<'_> {
    pub(crate) fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        self.get_actor_props_at_time(actor, time_ms)
    }

    pub(crate) fn get_actor_props_at_time(&self, actor: &str, time_ms: u64) -> Option<ActorProps> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        let half = track.geometry.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        let pivot_offset = self.pivot_offsets.get(actor).copied().unwrap_or([0.0, 0.0]);
        Some(ActorProps {
            position,
            size,
            rotation,
            pivot_offset,
        })
    }

    /// Get the text content property name for a text-type actor.
    /// Returns `Some(property_name)` for Text, Math, Code, Typst actors.
    pub(crate) fn get_text_property(&self, actor: &str) -> Option<&'static str> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        match track.kind {
            ActorKindId::Text => Some("text"),
            ActorKindId::Code => Some("code"),
            ActorKindId::Typst => Some("content"),
            _ => None,
        }
    }

    /// Get the current text content of a text-type actor.
    pub(crate) fn get_text_content(&self, actor: &str) -> Option<String> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        let schema = animatix::timeline::lookup_property("text").unwrap();
        let value = animatix::timeline::read_property_value_or_default(
            track,
            schema,
            time_ms,
        );
        match value {
            animatix::timeline::PropertyValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Start inline text editing for a text-type actor.
    pub(crate) fn start_inline_edit(&mut self, actor: &str, preview_rect: egui::Rect) {
        let Some(property) = self.get_text_property(actor) else {
            return;
        };
        let Some(content) = self.get_text_content(actor) else {
            return;
        };
        let Some(props) = self.get_actor_props(actor) else {
            return;
        };

        let screen_pos = preview::scene_to_screen(
            kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
            preview_rect,
            self.scene_dimensions,
            preview_rect.size(),
            self.preview.viewport.preview_zoom,
            self.preview.viewport.preview_pan,
        );

        // Estimate screen size from actor bounds
        let screen_size =
            if let Some((_, bounds)) = self.hit_regions.iter().find(|(l, _)| l == actor) {
                let tl = preview::scene_to_screen(
                    kurbo::Point::new(bounds.x0, bounds.y0),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                let br = preview::scene_to_screen(
                    kurbo::Point::new(bounds.x1, bounds.y1),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                egui::vec2(br.x - tl.x, br.y - tl.y).max(egui::vec2(100.0, 24.0))
            } else {
                egui::vec2(200.0, 24.0)
            };

        self.preview.inline_edit = Some(InlineTextEditState {
            actor: actor.to_string(),
            property: property.to_string(),
            current_value: content,
            screen_pos,
            screen_size,
        });
    }

    /// Render inline text editor overlay if active.
    pub(crate) fn render_inline_text_editor(
        &mut self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
    ) {
        let Some(edit) = self.preview.inline_edit.as_mut() else {
            return;
        };

        // Position the editor at the actor's screen position
        let editor_w = edit.screen_size.x.max(150.0);
        let editor_h = edit.screen_size.y.max(24.0);
        let editor_rect = egui::Rect::from_center_size(
            edit.screen_pos,
            egui::vec2(editor_w + 16.0, editor_h + 8.0),
        )
        .intersect(preview_rect);

        // Draw background
        ui.painter().rect_filled(editor_rect, RADIUS_M as u8, surface::SURFACE);
        ui.painter().rect_stroke(
            editor_rect,
            RADIUS_M as u8,
            Stroke::new(STROKE_WIDTH, accent::PRIMARY),
            egui::StrokeKind::Outside,
        );

        // Build text edit UI
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(editor_rect.shrink(SPACE_S)));
        child.set_clip_rect(editor_rect);

        let font = match edit.property.as_str() {
            "code" => TextRole::Mono.font_id(),
            "math" => TextRole::Mono.font_id(),
            _ => TextRole::BodyS.font_id(),
        };

        let response = child.add(
            egui::TextEdit::multiline(&mut edit.current_value)
                .font(font)
                .desired_width(editor_w)
                .desired_rows(1),
        );

        // Auto-focus the text edit
        response.request_focus();

        // Commit on Enter (without Shift) or Escape
        let enter_pressed = child.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        let escape_pressed = child.input(|i| i.key_pressed(egui::Key::Escape));
        let lost_focus = response.lost_focus();

        if enter_pressed || (lost_focus && !escape_pressed) {
            // Commit the edit
            let new_value = edit.current_value.clone();
            let actor = edit.actor.clone();
            let property = edit.property.clone();
            self.commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor,
                    property,
                    value: GuiPropertyValue::Text(new_value),
                    create_keyframe: false,
                })
                .into(),
            );
            self.preview.inline_edit = None;
        } else if escape_pressed {
            // Cancel the edit
            self.preview.inline_edit = None;
        }
    }

    pub(crate) fn is_layout_managed(&self, actor: &str) -> bool {
        let Some(timeline) = self.timeline else {
            return false;
        };
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        preview::is_layout_managed(actor, timeline, time_ms)
    }

    pub(crate) fn find_layout_container(
        &self,
        actor: &str,
    ) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline?;
        let container = timeline
            .tracks()
            .iter()
            .find(|(_, track)| track.children.iter().any(|child| child == actor))?
            .0
            .clone();
        let metadata = timeline.container_metadata().get(&container)?;
        let source_index = timeline
            .get_track(&container)?
            .children
            .iter()
            .position(|child| child == actor)?;
        Some((container, metadata.layout_type, source_index))
    }

    pub(crate) fn preview_transform(&self, preview_rect: egui::Rect) -> preview::PreviewTransform {
        preview::PreviewTransform::new(
            self.scene_dimensions,
            preview_rect,
            self.preview.viewport.preview_zoom,
            self.preview.viewport.preview_pan,
        )
    }

    pub(crate) fn clamp_pan(&self, pan: Vec2, preview_rect: egui::Rect) -> Vec2 {
        Self::clamp_pan_value(
            pan,
            preview_rect,
            self.scene_dimensions,
            self.preview.viewport.preview_zoom,
        )
    }

    /// Pure clamping math — extracted so it can be unit-tested.
    pub(crate) fn clamp_pan_value(
        pan: Vec2,
        preview_rect: egui::Rect,
        scene_dimensions: SceneDimensions,
        zoom: f32,
    ) -> Vec2 {
        let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, Vec2::ZERO);
        let (scale, _) = tx.scale();
        let scene_w = scene_dimensions.width as f64;
        let scene_h = scene_dimensions.height as f64;
        let preview_w = preview_rect.width() as f64;
        let preview_h = preview_rect.height() as f64;

        let visible_w = (preview_w * scale).min(scene_w);
        let visible_h = (preview_h * scale).min(scene_h);
        let half_w = visible_w / 2.0;
        let half_h = visible_h / 2.0;

        Vec2::new(
            pan.x.clamp(half_w as f32, (scene_w - half_w) as f32),
            pan.y.clamp(half_h as f32, (scene_h - half_h) as f32),
        )
    }

    pub(crate) fn preview_screen_to_scene(
        &self,
        preview_rect: egui::Rect,
        screen: egui::Pos2,
    ) -> kurbo::Point {
        self.preview_transform(preview_rect).screen_to_scene(screen)
    }

    pub(crate) fn preview_scene_to_screen(
        &self,
        preview_rect: egui::Rect,
        scene: kurbo::Point,
    ) -> egui::Pos2 {
        self.preview_transform(preview_rect).scene_to_screen(scene)
    }

    pub(crate) fn handle_preview_selection(
        &mut self,
        ui: &mut egui::Ui,
        _preview_rect: egui::Rect,
        response: &egui::Response,
    ) {
        if ui.input(|i| i.pointer.middle_down()) {
            return;
        }

        let is_dragging = !matches!(self.drag_state, DragState::None);

        // Filter out locked actors from hit regions for selection purposes
        let unlocked_hit_regions: Vec<(String, kurbo::Rect)> = self
            .hit_regions
            .iter()
            .filter(|(label, _)| {
                !self
                    .timeline
                    .and_then(|t| t.get_track(label))
                    .map(|tr| tr.locked)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        if response.secondary_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                selection::handle_right_click(
                    self.selection,
                    &unlocked_hit_regions,
                    click_pos,
                    move |screen| {
                        let tx = preview::PreviewTransform::new(
                            scene_dimensions,
                            _preview_rect,
                            zoom,
                            pan,
                        );
                        tx.screen_to_scene(screen)
                    },
                );
            }
        }

        let mut menu_item_clicked = false;
        if self.selection.context_menu_open {
            let (selected, close, _rect) =
                selection::draw_context_menu(ui, self.selection, self.selected_actors);
            menu_item_clicked = close;
            if let Some(actor) = selected {
                self.selected_actors.clear();
                self.selected_actors.insert(actor);
            }
            if close {
                self.selection.context_menu_open = false;
            }
        }

        let mut suppress_click = false;
        if self.selection.context_menu_open
            && !menu_item_clicked
            && ui.input(|i| i.pointer.primary_clicked())
        {
            self.selection.context_menu_open = false;
            suppress_click = true;
            self.selected_actors.clear();
        }

        // ── End of context-menu / click handling ──
        if response.clicked()
            && !is_dragging
            && !self.selection.context_menu_open
            && !suppress_click
        {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let modifiers = ui.ctx().input(|i| i.modifiers);
                selection::handle_click(
                    self.selection,
                    self.selected_actors,
                    &unlocked_hit_regions,
                    click_pos,
                    move |screen| {
                        let tx = preview::PreviewTransform::new(
                            scene_dimensions,
                            _preview_rect,
                            zoom,
                            pan,
                        );
                        tx.screen_to_scene(screen)
                    },
                    &modifiers,
                );

                if let Some(comp) = self.composition {
                    if let Some(actor) = self.selected_actors.iter().next().cloned() {
                        let active_has_actor = self.active_scene.is_some_and(|scene| {
                            comp.scenes.get(scene).is_some_and(|s| s.timeline.has_actor(&actor))
                        });
                        if !active_has_actor {
                            for (scene_name, scene) in &comp.scenes {
                                if scene.timeline.has_actor(&actor) {
                                    self.commands.push_back(
                                        SceneCommand::SelectScene(scene_name.clone()).into(),
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Double-click: start inline text editing for text actors ──
        if response.double_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let scene_point = {
                    let tx =
                        preview::PreviewTransform::new(scene_dimensions, _preview_rect, zoom, pan);
                    tx.screen_to_scene(click_pos)
                };

                // Find a text-type actor at the click position (skip locked actors)
                let text_actor = self
                    .hit_regions
                    .iter()
                    .filter(|(_, bounds)| {
                        // Check if click is within bounds
                        scene_point.x >= bounds.x0
                            && scene_point.x <= bounds.x1
                            && scene_point.y >= bounds.y0
                            && scene_point.y <= bounds.y1
                    })
                    .filter(|(label, _)| {
                        // Skip locked actors
                        !self
                            .timeline
                            .and_then(|t| t.get_track(label))
                            .map(|tr| tr.locked)
                            .unwrap_or(false)
                    })
                    .filter(|(label, _)| self.get_text_property(label).is_some())
                    .map(|(label, _)| label.clone())
                    .next();

                if let Some(actor) = text_actor {
                    self.selected_actors.clear();
                    self.selected_actors.insert(actor.clone());
                    self.start_inline_edit(&actor, _preview_rect);
                }
            }
        }
    }

    pub(crate) fn render_preview_cursor_feedback(&self, ui: &egui::Ui, preview_rect: egui::Rect) {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
        let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;

        if !is_dragging && !self.selection.context_menu_open {
            if let Some(mouse) = raw_pointer_pos {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);

                let over_handle = self.selected_actors.iter().next().and_then(|a| {
                    let props = self.get_actor_props(a)?;
                    let pivot_world_pt = preview::pivot_world(&props);
                    let pivot_screen = self.preview_scene_to_screen(
                        preview_rect,
                        kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                    );
                    if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                        return Some(9usize);
                    }
                    let handle_world = preview::world_handle_positions(&props);
                    let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
                        self.preview_scene_to_screen(preview_rect, handle_world[i])
                    });
                    if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen, hit_radius) {
                        Some(idx)
                    } else {
                        let rot_world = preview::rotation_handle_world(&props);
                        let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen, hit_radius) {
                            Some(8usize)
                        } else {
                            None
                        }
                    }
                });

                if let Some(handle_idx) = over_handle {
                    let (icon, tooltip) = match handle_idx {
                        0 => (egui::CursorIcon::ResizeNwSe, "Scale from top-left"),
                        1 => (egui::CursorIcon::ResizeNeSw, "Scale from top-right"),
                        2 => (egui::CursorIcon::ResizeNwSe, "Scale from bottom-right"),
                        3 => (egui::CursorIcon::ResizeNeSw, "Scale from bottom-left"),
                        4 => (egui::CursorIcon::ResizeVertical, "Scale height"),
                        5 => (egui::CursorIcon::ResizeHorizontal, "Scale width"),
                        6 => (egui::CursorIcon::ResizeVertical, "Scale height"),
                        7 => (egui::CursorIcon::ResizeHorizontal, "Scale width"),
                        8 => (egui::CursorIcon::Crosshair, "Rotate"),
                        9 => (egui::CursorIcon::Move, "Move pivot"),
                        _ => (egui::CursorIcon::Default, ""),
                    };
                    ui.ctx().set_cursor_icon(icon);
                    egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        egui::Id::new("handle_tooltip"),
                        egui::PopupAnchor::Pointer,
                    )
                    .show(|ui| {
                        ui.label(
                            egui::RichText::new(tooltip).size(
                                crate::app::design_tokens::typography::TextRole::BodyS.size(),
                            ),
                        );
                    });
                } else {
                    let is_over_selected = self
                        .selected_actors
                        .iter()
                        .next()
                        .and_then(|a| {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == a)
                                .map(|(_, b)| b.contains(scene))
                        })
                        .unwrap_or(false);
                    if is_over_selected {
                        let cursor = match *self.tool_mode {
                            preview::ToolMode::Move => egui::CursorIcon::Grab,
                            preview::ToolMode::Scale => egui::CursorIcon::ResizeNwSe,
                            preview::ToolMode::Rotate => egui::CursorIcon::Crosshair,
                            preview::ToolMode::Vertex => egui::CursorIcon::Crosshair,
                            preview::ToolMode::Pivot => egui::CursorIcon::Move,
                            preview::ToolMode::Select => egui::CursorIcon::Grab,
                        };
                        ui.ctx().set_cursor_icon(cursor);
                    } else if self.selection.hovered_actor.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            }
        } else if !self.selection.context_menu_open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    pub(crate) fn render_preview_content(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        match self.preview_texture_id {
            Some(texture_id) => {
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let scene_w = self.scene_dimensions.width.max(1) as f32;
                let scene_h = self.scene_dimensions.height.max(1) as f32;
                let tx =
                    preview::PreviewTransform::new(self.scene_dimensions, preview_rect, zoom, pan);
                let display_rect = tx.display_rect();

                if (zoom - 1.0).abs() > 0.001 || pan != Vec2::new(scene_w / 2.0, scene_h / 2.0) {
                    let half_inv_zx = 0.5 / zoom.max(PREVIEW_MIN_ZOOM);
                    let half_inv_zy = 0.5 / zoom.max(PREVIEW_MIN_ZOOM);
                    let uv_cx = (pan.x / scene_w).clamp(0.0, 1.0);
                    let uv_cy = (pan.y / scene_h).clamp(0.0, 1.0);
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2(
                            (uv_cx - half_inv_zx).clamp(0.0, 1.0),
                            (uv_cy - half_inv_zy).clamp(0.0, 1.0),
                        ),
                        egui::pos2(
                            (uv_cx + half_inv_zx).clamp(0.0, 1.0),
                            (uv_cy + half_inv_zy).clamp(0.0, 1.0),
                        ),
                    );
                    ui.put(
                        display_rect,
                        egui::Image::new((texture_id, display_rect.size())).uv(uv_rect),
                    );
                } else {
                    ui.put(display_rect, egui::Image::new((texture_id, display_rect.size())));
                }
            },
            None => {
                ui.painter().text(
                    preview_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Preview initializing…",
                    egui::TextStyle::Body.resolve(ui.style()),
                    text::MUTED,
                );
            },
        }
    }

    pub(crate) fn render_preview_overlays(&mut self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        if self.selection.context_menu_open {
            return;
        }

        if let Some(mouse) =
            ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p))
        {
            if !self
                .selected_actors
                .contains(self.selection.hovered_actor.as_deref().unwrap_or(""))
            {
                selection::draw_cycle_indicator(
                    ui.painter(),
                    mouse,
                    self.selection.cycle_index,
                    self.selection.click_candidates.len(),
                );
            }
        }

        if self.preview.overlay.show_hover_highlight {
            if let Some(hovered) = self.selection.hovered_actor.as_ref() {
                if !self.selected_actors.contains(hovered) {
                    if let Some(hover_rect) = preview::selection_screen_rect(
                        &HashSet::from([hovered.clone()]),
                        self.hit_regions,
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    ) {
                        selection::draw_hover_highlight(ui.painter(), hovered, hover_rect);
                    }
                }
            }
        }

        if self.preview.overlay.show_snap_guides {
            if let DragState::Move { primary, .. } | DragState::Scale { actor: primary, .. } =
                &self.drag_state
            {
                self.draw_snap_guides(ui, preview_rect, primary);
            }
        }

        if self.preview.overlay.show_snap_guides {
            if let Some(ref label) = self.preview.snap.snap_hud_label {
                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                    let hud_pos = mouse + Vec2::new(12.0, -24.0);
                    let galley = ui.painter().layout_no_wrap(
                        label.clone(),
                        TextRole::BodyS.font_id(),
                        status::SUCCESS,
                    );
                    let padding = Vec2::new(8.0, 4.0);
                    let bg_rect = egui::Rect::from_min_size(hud_pos, galley.size() + padding * 2.0);
                    ui.painter().rect_filled(
                        bg_rect,
                        3.0,
                        crate::app::design_tokens::semantic::canvas::snap_guide_label_bg(),
                    );
                    ui.painter().rect_stroke(
                        bg_rect,
                        3.0,
                        egui::Stroke::new(
                            STROKE_WIDTH,
                            crate::app::design_tokens::semantic::canvas::snap_guide_line(),
                        ),
                        egui::StrokeKind::Outside,
                    );
                    ui.painter().galley(hud_pos + padding, galley, status::SUCCESS);
                }
            }
        }

        // ── Performance HUD ──
        if self.preview.overlay.show_performance_hud {
            preview::overlay::render_performance_hud(
                ui.painter(),
                preview_rect,
                self.performance_metrics,
            );
        }
    }

    pub(crate) fn draw_snap_guides(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        primary: &str,
    ) {
        let primary_props = self.get_actor_props(primary);
        let primary_rect = if let Some(p) = primary_props {
            let hw = p.size[0] / 2.0;
            let hh = p.size[1] / 2.0;
            let corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for corner in &corners {
                let world = preview::local_to_world(*corner, p.position, p.rotation);
                let screen = preview::scene_to_screen(
                    world,
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                min_x = min_x.min(screen.x);
                min_y = min_y.min(screen.y);
                max_x = max_x.max(screen.x);
                max_y = max_y.max(screen.y);
            }
            egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
        } else {
            self.hit_regions
                .iter()
                .find(|(l, _)| l == primary)
                .map(|(_, bounds)| {
                    let tl = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x0, bounds.y0),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    );
                    let br = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x1, bounds.y1),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    );
                    egui::Rect::from_min_max(tl, br)
                })
                .unwrap_or(preview_rect)
        };

        let threshold = 8.0;
        let guide_color = crate::app::design_tokens::semantic::accent::subtle();
        let guide_stroke = egui::Stroke::new(STROKE_WIDTH, guide_color);

        for (label, bounds) in self.hit_regions {
            if label == primary || self.selected_actors.contains(label) {
                continue;
            }
            let tl = preview::scene_to_screen(
                kurbo::Point::new(bounds.x0, bounds.y0),
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
                self.preview.viewport.preview_zoom,
                self.preview.viewport.preview_pan,
            );
            let br = preview::scene_to_screen(
                kurbo::Point::new(bounds.x1, bounds.y1),
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
                self.preview.viewport.preview_zoom,
                self.preview.viewport.preview_pan,
            );
            let other_rect = egui::Rect::from_min_max(tl, br);

            let px = [
                primary_rect.min.x,
                primary_rect.max.x,
                primary_rect.center().x,
            ];
            let py = [
                primary_rect.min.y,
                primary_rect.max.y,
                primary_rect.center().y,
            ];
            let ox = [other_rect.min.x, other_rect.max.x, other_rect.center().x];
            let oy = [other_rect.min.y, other_rect.max.y, other_rect.center().y];

            for &px in &px {
                for &ox in &ox {
                    if (px - ox).abs() < threshold {
                        ui.painter().line_segment(
                            [
                                egui::pos2(px, preview_rect.min.y),
                                egui::pos2(px, preview_rect.max.y),
                            ],
                            guide_stroke,
                        );
                    }
                }
            }
            for &py in &py {
                for &oy in &oy {
                    if (py - oy).abs() < threshold {
                        ui.painter().line_segment(
                            [
                                egui::pos2(preview_rect.min.x, py),
                                egui::pos2(preview_rect.max.x, py),
                            ],
                            guide_stroke,
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn render_motion_paths(&mut self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        if !self.preview.overlay.show_motion_paths {
            return;
        }

        let timeline = match self.timeline {
            Some(t) => t,
            None => return,
        };

        for actor in self.selected_actors.iter() {
            let track = match timeline.get_track(actor) {
                Some(t) => t,
                None => continue,
            };
            let pos_track = match &track.geometry.position {
                Some(pt) => pt,
                None => continue,
            };
            if pos_track.keyframes().len() < 2 {
                continue;
            }

            // Collect keyframe positions
            let mut kf_points: Vec<(u64, [f32; 2])> = Vec::new();
            for (&time_ms, (val, _)) in pos_track.keyframes() {
                kf_points.push((time_ms, *val));
            }
            kf_points.sort_by_key(|(t, _)| *t);

            // Draw path lines
            let path_color = accent::PRIMARY.gamma_multiply(0.6);
            let path_stroke = egui::Stroke::new(1.5, path_color);
            for i in 0..kf_points.len().saturating_sub(1) {
                let p1_screen = preview::scene_to_screen(
                    kurbo::Point::new(kf_points[i].1[0] as f64, kf_points[i].1[1] as f64),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                let p2_screen = preview::scene_to_screen(
                    kurbo::Point::new(kf_points[i + 1].1[0] as f64, kf_points[i + 1].1[1] as f64),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                ui.painter().line_segment([p1_screen, p2_screen], path_stroke);
            }

            // Draw keyframe dots
            for (time_ms, pos) in &kf_points {
                let screen = preview::scene_to_screen(
                    kurbo::Point::new(pos[0] as f64, pos[1] as f64),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                let current_time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
                let is_current = *time_ms == current_time_ms;
                let dot_color = if is_current {
                    status::WARNING
                } else {
                    accent::PRIMARY
                };
                let dot_radius = if is_current { 5.0 } else { 3.5 };
                ui.painter().circle_filled(screen, dot_radius, dot_color);
                if is_current {
                    ui.painter().circle_stroke(
                        screen,
                        dot_radius + 2.0,
                        egui::Stroke::new(1.0, status::WARNING),
                    );
                }

                // Time label
                let time_label = format!("{:.1}s", *time_ms as f64 / 1000.0);
                ui.painter().text(
                    egui::pos2(screen.x, screen.y - dot_radius - 4.0),
                    egui::Align2::CENTER_BOTTOM,
                    time_label,
                    TextRole::Micro.font_id(),
                    text::MUTED,
                );
            }
        }
    }

    pub(crate) fn render_preview_selection_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        is_dragging: bool,
    ) {
        if self.selected_actors.len() > 1 {
            let mut screen_rects = Vec::new();
            for actor in self.selected_actors.iter() {
                if let Some(props) = self.get_actor_props(actor) {
                    let hw = props.size[0] / 2.0;
                    let hh = props.size[1] / 2.0;
                    let local_corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
                    let mut min_x = f32::INFINITY;
                    let mut min_y = f32::INFINITY;
                    let mut max_x = f32::NEG_INFINITY;
                    let mut max_y = f32::NEG_INFINITY;
                    for corner in &local_corners {
                        let world =
                            preview::local_to_world(*corner, props.position, props.rotation);
                        let screen = preview::scene_to_screen(
                            world,
                            preview_rect,
                            self.scene_dimensions,
                            preview_rect.size(),
                            self.preview.viewport.preview_zoom,
                            self.preview.viewport.preview_pan,
                        );
                        min_x = min_x.min(screen.x);
                        min_y = min_y.min(screen.y);
                        max_x = max_x.max(screen.x);
                        max_y = max_y.max(screen.y);
                    }
                    screen_rects.push(egui::Rect::from_min_max(
                        egui::pos2(min_x, min_y),
                        egui::pos2(max_x, max_y),
                    ));
                } else if let Some((_, bounds)) = self.hit_regions.iter().find(|(l, _)| l == actor)
                {
                    let top_left = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x0, bounds.y0),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    );
                    let br = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x1, bounds.y1),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    );
                    screen_rects.push(egui::Rect::from_min_max(top_left, br));
                }
            }
            preview::draw_multi_selection_overlay(
                ui.painter(),
                &screen_rects,
                is_dragging,
                ui.ctx().pixels_per_point(),
            );
            return;
        }

        for actor in self.selected_actors.iter() {
            let props = self.get_actor_props(actor);
            let fallback = self.hit_regions.iter().find(|(l, _)| l == actor).map(|(_, bounds)| {
                let tl = preview::scene_to_screen(
                    kurbo::Point::new(bounds.x0, bounds.y0),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                let br = preview::scene_to_screen(
                    kurbo::Point::new(bounds.x1, bounds.y1),
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                egui::Rect::from_min_max(tl, br)
            });
            preview::draw_selection_overlay(
                ui.painter(),
                props.as_ref(),
                fallback,
                is_dragging,
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
                ui.ctx().pixels_per_point(),
                self.preview.viewport.preview_zoom,
                self.preview.viewport.preview_pan,
            );

            let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
            let points = self
                .timeline
                .and_then(|t| t.get_track(actor))
                .and_then(|tr| tr.shape.points.as_ref().map(|pt| pt.evaluate(time_ms)))
                .filter(|pts| !pts.is_empty());
            if let (Some(ref p), Some(pts)) = (props, points) {
                let active_vertex = match &self.drag_state {
                    DragState::EditVertices {
                        actor: drag_actor,
                        vertex,
                        ..
                    } => {
                        if drag_actor == actor {
                            Some(*vertex)
                        } else {
                            None
                        }
                    },
                    _ => None,
                };
                preview::draw_vertex_handles(
                    ui.painter(),
                    p,
                    &pts,
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    active_vertex,
                    ui.ctx().pixels_per_point(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
            }

            if is_dragging {
                let measurement_color = accent::PRIMARY;
                let text_color = text::PRIMARY;
                let font = egui::FontId::monospace(TextRole::Micro.size());
                match &self.drag_state {
                    DragState::Move {
                        primary,
                        actors: _,
                        start_scene,
                    } => {
                        if let Some(props) = self.get_actor_props(primary) {
                            let start_screen = preview::scene_to_screen(
                                kurbo::Point::new(start_scene.x, start_scene.y),
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                            );
                            let current_screen = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64,
                                    props.position[1] as f64,
                                ),
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                            );
                            let y = (start_screen.y + current_screen.y) / 2.0;
                            ui.painter().line_segment(
                                [
                                    Pos2::new(start_screen.x.min(current_screen.x), y),
                                    Pos2::new(start_screen.x.max(current_screen.x), y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, measurement_color),
                            );
                            ui.painter().text(
                                Pos2::new((start_screen.x + current_screen.x) / 2.0, y - 8.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!("Δx: {:+.0}", props.position[0] - start_scene.x as f32),
                                font.clone(),
                                text_color,
                            );
                            let x = (start_screen.x + current_screen.x) / 2.0;
                            ui.painter().line_segment(
                                [
                                    Pos2::new(x, start_screen.y.min(current_screen.y)),
                                    Pos2::new(x, start_screen.y.max(current_screen.y)),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, measurement_color),
                            );
                            ui.painter().text(
                                Pos2::new(x + 4.0, (start_screen.y + current_screen.y) / 2.0),
                                egui::Align2::LEFT_CENTER,
                                format!("Δy: {:+.0}", props.position[1] - start_scene.y as f32),
                                font.clone(),
                                text_color,
                            );
                        }
                    },
                    DragState::Scale {
                        actor, start_size, ..
                    } => {
                        if let Some(props) = self.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64,
                                    props.position[1] as f64,
                                ),
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                            );
                            let br = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64 + props.size[0] as f64 / 2.0,
                                    props.position[1] as f64 + props.size[1] as f64 / 2.0,
                                ),
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                            );
                            ui.painter().text(
                                Pos2::new(screen_pos.x, br.y + 12.0),
                                egui::Align2::CENTER_TOP,
                                format!("w: {:.0} → {:.0}", start_size[0], props.size[0]),
                                font.clone(),
                                text_color,
                            );
                            ui.painter().text(
                                Pos2::new(br.x + 4.0, screen_pos.y),
                                egui::Align2::LEFT_CENTER,
                                format!("h: {:.0} → {:.0}", start_size[1], props.size[1]),
                                font.clone(),
                                text_color,
                            );
                        }
                    },
                    DragState::Rotate {
                        actor,
                        start_rotation,
                        ..
                    } => {
                        if let Some(props) = self.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64,
                                    props.position[1] as f64,
                                ),
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                            );
                            ui.painter().text(
                                Pos2::new(screen_pos.x, screen_pos.y - props.size[1] / 2.0 - 16.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!(
                                    "{:.0}° → {:.0}°",
                                    start_rotation.to_degrees(),
                                    props.rotation.to_degrees()
                                ),
                                font.clone(),
                                text_color,
                            );
                        }
                    },
                    _ => {},
                }
            }

            if !is_dragging {
                if let Some(timeline) = self.timeline {
                    let current_time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
                    let keyframe_times = timeline.keyframe_times_s();
                    let mut prev_time_ms: Option<u64> = None;
                    let mut next_time_ms: Option<u64> = None;
                    for &time_s in &keyframe_times {
                        let time_ms = (time_s * 1000.0) as u64;
                        if time_ms < current_time_ms {
                            prev_time_ms = Some(time_ms);
                        } else if time_ms > current_time_ms && next_time_ms.is_none() {
                            next_time_ms = Some(time_ms);
                        }
                    }
                    if let Some(prev_ms) = prev_time_ms {
                        if let Some(prev_props) = self.get_actor_props_at_time(actor, prev_ms) {
                            preview::draw_ghost_overlay(
                                ui.painter(),
                                &prev_props,
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                                crate::app::design_tokens::semantic::canvas::ghost_prev(),
                            );
                        }
                    }
                    if let Some(next_ms) = next_time_ms {
                        if let Some(next_props) = self.get_actor_props_at_time(actor, next_ms) {
                            preview::draw_ghost_overlay(
                                ui.painter(),
                                &next_props,
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                                crate::app::design_tokens::semantic::canvas::ghost_next(),
                            );
                        }
                    }
                }
            }

            if let DragState::Reorder {
                actor: drag_actor,
                container,
                target_index,
                layout_type,
                ..
            } = self.drag_state.clone()
            {
                if &drag_actor == actor {
                    let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<(String, [f32; 2])> = order
                            .into_iter()
                            .filter(|label| label != actor)
                            .filter_map(|label| {
                                self.get_actor_props(&label).map(|p| (label, p.position))
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
                                preview_rect.size(),
                                layout_type == animatix::timeline::LayoutType::Row,
                                self.preview.viewport.preview_zoom,
                                self.preview.viewport.preview_pan,
                            );
                        }
                    }
                }
            }
        }

        if let (Some(start), Some(current)) =
            (self.selection.marquee_start, self.selection.marquee_current)
        {
            let marquee_rect = egui::Rect::from_two_pos(start, current);
            ui.painter().rect_filled(
                marquee_rect,
                0.0,
                crate::app::design_tokens::semantic::accent::faint(),
            );
            ui.painter().rect_stroke(
                marquee_rect,
                0.0,
                egui::Stroke::new(
                    STROKE_WIDTH,
                    crate::app::design_tokens::semantic::accent::subtle(),
                ),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// Render layout debug overlay showing container bounds, slot outlines, and sizes.
    pub(crate) fn render_layout_debug(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        let Some(timeline) = self.timeline else {
            return;
        };
        if !self.debug_layout {
            return;
        }
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        crate::app::preview::overlay::render_layout_debug(
            ui.painter(),
            timeline,
            time_ms,
            preview_rect,
            self.scene_dimensions,
            self.preview.viewport.preview_zoom,
            self.preview.viewport.preview_pan,
            self.debug_spacing,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Rect;

    #[test]
    fn test_clamp_pan_center() {
        // Scene 1920×1080, preview 960×540, zoom=1 → full scene visible, must pan to center (960, 540)
        let scene = SceneDimensions {
            width: 1920,
            height: 1080,
        };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(500.0, 300.0), preview, scene, 1.0);
        // Both axes are clamped to the exact center since all scene is visible
        assert_eq!(result.x, 960.0);
        assert_eq!(result.y, 540.0);
    }

    #[test]
    fn test_clamp_pan_beyond_bounds() {
        // Scene 1920×1080, preview 960×540, zoom=2 → half scene visible
        // visible_w = min(960*1.0, 1920) = 960, half_w = 480
        // range: [480, 1920-480] = [480, 1440]
        let scene = SceneDimensions {
            width: 1920,
            height: 1080,
        };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
        // Pan way beyond right edge
        let result =
            PreviewContext::clamp_pan_value(Vec2::new(2000.0, 1000.0), preview, scene, 2.0);
        assert_eq!(result.x, 1440.0);
        assert_eq!(result.y, 810.0);
        // Pan way beyond left/top edge
        let result =
            PreviewContext::clamp_pan_value(Vec2::new(-100.0, -100.0), preview, scene, 2.0);
        assert_eq!(result.x, 480.0);
        assert_eq!(result.y, 270.0);
    }

    #[test]
    fn test_clamp_pan_zero_size_preview() {
        // Minimal preview rect (1×1 minimum via scale logic)
        let scene = SceneDimensions {
            width: 1920,
            height: 1080,
        };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1.0, 1.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(500.0, 300.0), preview, scene, 1.0);
        // visible_w = min(1 * huge_scale, 1920) = 1920 → half_w = 960 → range [960, 960]
        assert_eq!(result.x, 960.0);
        assert_eq!(result.y, 540.0);
    }

    #[test]
    fn test_clamp_pan_extreme_zoom() {
        // Scene 1920×1080, preview 960×540, zoom=10 → tiny viewport in scene space
        // scale = (base_scale=2.0) / 10 = 0.2
        // visible_w = min(960 * 0.2, 1920) = min(192, 1920) = 192, half_w = 96
        // range: [96, 1920-96] = [96, 1824]
        let scene = SceneDimensions {
            width: 1920,
            height: 1080,
        };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(100.0, 100.0), preview, scene, 10.0);
        assert_eq!(result.x, 100.0); // 100 is in [96, 1824]
        assert_eq!(result.y, 100.0); // 100 is in [54, 1026]
    }

    #[test]
    fn test_clamp_pan_scene_smaller_than_preview() {
        // Scene 100×50, preview 500×500, zoom=1 → scene is tiny compared to preview
        // px_per_scene_x = 500/100 = 5, px_per_scene_y = 500/50 = 10
        // px_per_scene = min(5, 10) = 5
        // base_scale = 1/5 = 0.2
        // scale = 0.2 / 1 = 0.2
        // visible_w = min(500*0.2, 100) = min(100, 100) = 100, half_w = 50
        // range: [50, 100-50] = [50, 50]
        // visible_h = min(500*0.2, 50) = min(100, 50) = 50, half_h = 25
        // range: [25, 50-25] = [25, 25]
        let scene = SceneDimensions {
            width: 100,
            height: 50,
        };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 500.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(0.0, 0.0), preview, scene, 1.0);
        assert_eq!(result.x, 50.0);
        assert_eq!(result.y, 25.0);
    }
}
