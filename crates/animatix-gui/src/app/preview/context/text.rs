//! Inline text editing helpers and overlay.

use egui::Stroke;

use crate::app::InlineTextEditState;
use crate::app::commands::{DocumentCommand, PropertyEdit, PropertyValue as GuiPropertyValue};
use crate::app::design_tokens::spatial::{RADIUS_M, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::preview;

use super::PreviewContext;

impl PreviewContext<'_> {
    /// Get the text content property name for a text-type actor.
    /// Returns `Some(property_name)` for Text, Math, Code, Typst actors;
    /// extension primitives that emit text paths default to `"text"`.
    pub(crate) fn get_text_property(&self, actor: &str) -> Option<&'static str> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        crate::app::primitives::actor_gizmo_affordances(track, timeline)
            .into_iter()
            .find_map(|a| match a {
                crate::app::primitives::GizmoAffordance::InlineText { property } => Some(property),
                _ => None,
            })
    }

    /// Get the current text content of a text-type actor.
    pub(crate) fn get_text_content(&self, actor: &str, property: &str) -> Option<String> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        let schema = animatix::timeline::lookup_property(property)?;
        // Shape-aware read: the World field bundle resolves whichever tier owns
        // the property instead of a shape-blind track accessor.
        let value = timeline
            .world()
            .read_property_value(track.id, schema.field, time_ms)
            .unwrap_or_else(|| {
                (schema.default_value)(
                    &animatix::timeline::caps_for_type(timeline.world().actor_type(track.id))
                        .unwrap_or_default(),
                )
            });
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
        let Some(content) = self.get_text_content(actor, property) else {
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

        let theme = eparts::theme(ui);

        // Draw background
        ui.painter()
            .rect_filled(editor_rect, RADIUS_M as u8, theme.palette.surface.surface);
        ui.painter().rect_stroke(
            editor_rect,
            RADIUS_M as u8,
            Stroke::new(STROKE_WIDTH, theme.palette.accent.primary),
            egui::StrokeKind::Outside,
        );

        // Build text edit UI
        let sp = spatial(ui);
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(editor_rect.shrink(sp.base.space_2)));
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
                    value: GuiPropertyValue::String(new_value),
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
}
