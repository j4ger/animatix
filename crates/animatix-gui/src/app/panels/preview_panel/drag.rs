//! Library drag-and-drop over the preview.

use super::coords::*;
use super::*;

pub(super) fn library_drag_drop_ui(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
) {
    let Some(payload) = crate::app::panels::library_drag(ui.ctx()) else {
        return;
    };

    let pointer = ui.ctx().input(|i| i.pointer.latest_pos());
    let over_preview = pointer.is_some_and(|p| preview_rect.contains(p));

    if over_preview {
        // Drop highlight: accent outline plus a translucent fill so the target
        // region reads clearly against the scene.
        let theme = eparts::theme(ui);
        ui.painter().rect_filled(preview_rect, RADIUS_L, theme.palette.accent.faint);
        ui.painter().rect_stroke(
            preview_rect,
            RADIUS_L,
            egui::Stroke::new(2.0, theme.palette.accent.primary),
            egui::StrokeKind::Inside,
        );
        if let Some(mouse) = pointer {
            ui.painter().text(
                mouse + Vec2::new(12.0, -12.0),
                egui::Align2::LEFT_CENTER,
                &payload.ty,
                TextRole::BodyS.font_id(),
                theme.palette.text.primary,
            );
        }
    }

    let released = ui.input(|i| i.pointer.any_released());
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escape {
        crate::app::panels::clear_library_drag(ui.ctx());
        return;
    }
    if !released {
        return;
    }

    if over_preview {
        if let Some(mouse) = pointer {
            let scene = preview_screen_to_scene(
                ctx.scene_dimensions,
                preview_rect,
                mouse,
                ctx.preview.viewport.preview_zoom,
                ctx.preview.viewport.preview_pan,
            );
            let label = crate::app::utils::labels::unique_label(
                ctx.timeline,
                &crate::app::panels::library_drag_label_base(&payload),
            );
            ctx.commands.push_back(
                ActorCommand::CreateActor {
                    ty: payload.ty.clone(),
                    label,
                    position: [scene.x as f32, scene.y as f32],
                    props: payload.props.clone(),
                }
                .into(),
            );
        }
    }
    // Always clear on release so a drag that ends off-canvas leaves no payload.
    crate::app::panels::clear_library_drag(ui.ctx());
}

// ─── Main preview_panel_ui function ─────────────────────────────────────────
