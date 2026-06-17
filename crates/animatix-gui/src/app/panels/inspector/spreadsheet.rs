//! Property Spreadsheet View — tabular overview of all actors and their properties.
//!
//! Renders an egui [`Grid`] where:
//! - Rows = actor labels (sorted alphabetically)
//! - Columns = properties (position, size, rotation, opacity, color, etc.)
//! - Cells = current value at playhead time
//!
//! Clicking an actor label selects that actor. Right-clicking a cell offers
//! a context menu to add a keyframe at the current time.

use std::collections::HashSet;

use egui::{Color32, Pos2, RichText, Vec2};

use animatix::timeline::{AnimationTrack, SceneDimensions, Timeline, lookup_property};

use super::PropertyViewMode;
use crate::app::commands::{
    ActionQueue, ActorCommand, Command, DocumentCommand, PropertyEdit,
    PropertyValue as GuiPropertyValue, ShellAction,
};
use crate::app::components::layout;
use crate::app::design_tokens::semantic::accent::{
    PRIMARY as semantic_accent_primary, selection as semantic_accent_selection,
};
use crate::app::design_tokens::semantic::status::WARNING as semantic_status_warning;
use crate::app::design_tokens::semantic::surface::{
    HOVER as semantic_surface_hover, SURFACE as semantic_surface_surface,
};
use crate::app::design_tokens::semantic::text::{
    MUTED as semantic_text_muted, PRIMARY as semantic_text_primary,
    SECONDARY as semantic_text_secondary,
};
use crate::app::design_tokens::spatial::{
    ROW_M as spatial_row_m, ROW_S as spatial_row_s, SPACE_2 as spatial_space_s,
    SPACE_3 as spatial_space_m, SPACE_5 as spatial_space_xl,
};
use crate::app::design_tokens::typography::TextRole;

/// The list of spreadsheet columns (property names).
///
/// These are the canonical names from the property registry. Only properties
/// that are commonly used and visually meaningful are included here.
const SPREADSHEET_PROPERTIES: &[&str] = &[
    "position",
    "size",
    "rotation",
    "scale",
    "opacity",
    "color",
    "stroke_width",
    "stroke_color",
    "stroke_progress",
    "fill_opacity",
];

// ─── Public entry point ──────────────────────────────────────────────────

/// Render the full property spreadsheet into [`ui`].
///
/// Uses `egui::Grid` with a sticky top-left corner (labels stay visible).
/// The grid scrolls both horizontally and vertically.
pub(crate) fn render_property_spreadsheet(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    current_time_s: f64,
    selected_actors: &mut HashSet<String>,
    commands: &mut ActionQueue,
    scene_dimensions: SceneDimensions,
    property_view_mode: &mut PropertyViewMode,
) {
    // ── View-mode toggle bar ──
    ui.horizontal(|ui| {
        ui.add_space(spatial_space_s);
        let btn = egui::Button::new(
            RichText::new(format!("{} Semantic", egui_phosphor::regular::ROWS))
                .size(TextRole::Micro.size())
                .color(semantic_text_secondary),
        )
        .min_size(Vec2::new(0.0, spatial_row_s));
        if ui.add(btn).on_hover_text("Switch to semantic property view").clicked() {
            *property_view_mode = PropertyViewMode::Semantic;
            return;
        }
        ui.add_space(spatial_space_m);
        ui.add(
            egui::Label::new(
                RichText::new(egui_phosphor::regular::TABLE)
                    .size(TextRole::BodyS.size())
                    .color(semantic_status_warning),
            )
            .selectable(false),
        );
        ui.add(
            egui::Label::new(
                RichText::new("Spreadsheet")
                    .size(TextRole::BodyS.size())
                    .color(semantic_status_warning),
            )
            .selectable(false),
        );
        ui.add_space(spatial_space_s);
    });
    ui.add_space(spatial_space_m);

    let Some(timeline) = timeline else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::TABLE,
            "No timeline",
            "Open a scene to view the spreadsheet",
        );
        return;
    };

    // Add Actor button
    if ui
        .button(
            RichText::new(format!("{} Add", egui_phosphor::regular::PLUS))
                .size(TextRole::Micro.size())
                .color(semantic_accent_primary),
        )
        .on_hover_text("Add a new actor")
        .clicked()
    {
        commands.push_back(
            ActorCommand::CreateActor {
                ty: crate::app::panels::default_actor_type().into(),
                label: crate::app::utils::labels::unique_label(None, "actor"),
                position: [
                    scene_dimensions.width as f32 / 2.0,
                    scene_dimensions.height as f32 / 2.0,
                ],
                props: vec![],
            }
            .into(),
        );
    }

    let actors: Vec<&String> = {
        let mut labels: Vec<&String> = timeline.actor_labels().collect();
        labels.sort();
        labels
    };

    if actors.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(spatial_space_xl * 3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(layout::EMPTY_STATE_ICON_SIZE)
                        .color(semantic_text_muted),
                )
                .selectable(false),
            );
            ui.add_space(spatial_space_m);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(TextRole::Title.size())
                        .color(semantic_text_secondary),
                )
                .selectable(false),
            );
        });
        return;
    }

    let time_ms = (current_time_s * 1000.0) as u64;

    // ── Determine row height and column widths ──
    let row_height = spatial_row_m; // 24px — matches INSPECTOR_ROW_HEIGHT
    let label_col_width = 140.0;
    let value_col_width = 90.0;

    // ── Scrollable outer container ──
    egui::ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
        egui::Grid::new(ui.id().with("spreadsheet_grid"))
            .striped(true)
            .min_col_width(label_col_width)
            .spacing(Vec2::new(spatial_space_s, 0.0))
            .show(ui, |ui| {
                // ── Column headers (top-left corner + property names) ──
                // Top-left corner header
                let corner_rect = ui
                    .allocate_exact_size(
                        Vec2::new(label_col_width, row_height),
                        egui::Sense::hover(),
                    )
                    .0;
                ui.painter().rect_filled(corner_rect, 0.0, semantic_surface_surface);
                ui.painter().text(
                    corner_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    egui_phosphor::regular::TABLE,
                    TextRole::BodyS.font_id(),
                    semantic_text_muted,
                );

                // Property name headers
                for &prop_name in SPREADSHEET_PROPERTIES {
                    let header_rect = ui
                        .allocate_exact_size(
                            Vec2::new(value_col_width, row_height),
                            egui::Sense::hover(),
                        )
                        .0;
                    ui.painter().rect_filled(header_rect, 0.0, semantic_surface_surface);
                    ui.painter().text(
                        header_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        prop_name,
                        TextRole::Micro.font_id(),
                        semantic_text_secondary,
                    );
                }
                ui.end_row();

                // ── Data rows ──
                for &actor_label in &actors {
                    let Some(track) = timeline.get_track(actor_label) else {
                        continue;
                    };

                    let is_selected = selected_actors.contains(actor_label);

                    // ── Actor label cell ──
                    let (label_rect, label_response) = ui.allocate_exact_size(
                        Vec2::new(label_col_width, row_height),
                        egui::Sense::click(),
                    );

                    // Background for selected or hovered row
                    let label_bg = if is_selected {
                        semantic_accent_selection()
                    } else if label_response.hovered() {
                        semantic_surface_hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    if label_bg != Color32::TRANSPARENT {
                        ui.painter().rect_filled(label_rect, 0.0, label_bg);
                    }

                    // Actor icon + label text
                    let icon = crate::app::icons::actor_icon_str(track.kind);
                    let label_color = if is_selected {
                        semantic_accent_primary
                    } else {
                        semantic_text_primary
                    };
                    ui.painter().text(
                        Pos2::new(label_rect.min.x + spatial_space_s, label_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{} {}", icon, actor_label),
                        TextRole::BodyS.font_id(),
                        label_color,
                    );

                    // Click to select
                    if label_response.clicked() {
                        let multi = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
                        if multi {
                            // Toggle selection
                            if is_selected {
                                selected_actors.remove(actor_label);
                            } else {
                                selected_actors.insert(actor_label.clone());
                            }
                        } else {
                            selected_actors.clear();
                            selected_actors.insert(actor_label.clone());
                        }
                    }

                    // ── Value cells ──
                    for &prop_name in SPREADSHEET_PROPERTIES {
                        let (cell_rect, cell_response) = ui.allocate_exact_size(
                            Vec2::new(value_col_width, row_height),
                            egui::Sense::click(),
                        );

                        // Subtle hover
                        if cell_response.hovered() {
                            ui.painter().rect_filled(cell_rect, 0.0, semantic_surface_hover);
                        }

                        // Get the value at current time
                        let value_text = get_property_value_display(track, prop_name, time_ms);
                        let has_animated_track = has_property_track(track, prop_name);
                        let field = lookup_property(prop_name).map(|s| s.field);
                        let has_keyframes = field.is_some()
                            && animatix::timeline::property_has_keyframes(track, field.unwrap());

                        let value_color = if has_keyframes {
                            semantic_status_warning
                        } else if !has_animated_track || value_text == "—" {
                            semantic_text_muted
                        } else {
                            semantic_text_secondary
                        };

                        ui.painter().text(
                            cell_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &value_text,
                            egui::FontId::monospace(TextRole::Micro.size()),
                            value_color,
                        );

                        // Context menu on right-click
                        cell_response.context_menu(|ui| {
                            ui.set_min_width(160.0);
                            ui.strong(format!("{} / {}", actor_label, prop_name));
                            ui.separator();
                            if ui
                                .button(format!("{} Add keyframe", egui_phosphor::regular::DIAMOND))
                                .on_hover_text("Add a keyframe at current time with current value")
                                .clicked()
                            {
                                if let Some(gui_val) =
                                    get_property_gui_value(track, prop_name, time_ms)
                                {
                                    commands.push_back(
                                        DocumentCommand::PropertyEdit(PropertyEdit {
                                            time_s: None,
                                            actor: actor_label.to_string(),
                                            property: prop_name.to_string(),
                                            value: gui_val,
                                            create_keyframe: true,
                                        })
                                        .into(),
                                    );
                                }
                                ui.close();
                            }
                            if ui
                                .button(format!(
                                    "{} Open in Inspector",
                                    egui_phosphor::regular::ARROW_RIGHT
                                ))
                                .clicked()
                            {
                                selected_actors.clear();
                                selected_actors.insert(actor_label.clone());
                                ui.close();
                            }
                        });
                    }
                    ui.end_row();
                }
            });
    });
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Get a display string for a property's value at the given time.
fn get_property_value_display(track: &AnimationTrack, prop_name: &str, time_ms: u64) -> String {
    match prop_name {
        "position" => track
            .position
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("({:.0}, {:.0})", v[0], v[1])
            })
            .unwrap_or_else(|| "—".into()),
        "size" => {
            track
                .size
                .as_ref()
                .map(|t| {
                    let v = t.evaluate_copy(time_ms);
                    // Size is stored as half-extents; show full dimensions
                    let w = v[0] * 2.0;
                    let h = v[1] * 2.0;
                    format!("({:.0}, {:.0})", w, h)
                })
                .unwrap_or_else(|| "—".into())
        },
        "rotation" => track
            .rotation
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("{:.1}°", v.to_degrees())
            })
            .unwrap_or_else(|| "—".into()),
        "scale" => track
            .scale
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("{:.2}×", v)
            })
            .unwrap_or_else(|| "—".into()),
        "opacity" => track
            .opacity
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("{:.2}", v)
            })
            .unwrap_or_else(|| "—".into()),
        "color" => track
            .color
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format_rgba(v)
            })
            .unwrap_or_else(|| "—".into()),
        "stroke_width" => track
            .stroke_width
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("{:.1}", v)
            })
            .unwrap_or_else(|| "—".into()),
        "stroke_color" => track
            .stroke_color
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format_rgba(v)
            })
            .unwrap_or_else(|| "—".into()),
        "stroke_progress" => track
            .stroke_progress
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("{:.2}", v)
            })
            .unwrap_or_else(|| "—".into()),
        "fill_opacity" => track
            .fill_opacity
            .as_ref()
            .map(|t| {
                let v = t.evaluate_copy(time_ms);
                format!("{:.2}", v)
            })
            .unwrap_or_else(|| "—".into()),
        _ => "—".into(),
    }
}

/// Get a `GuiPropertyValue` for the property at the given time, for emitting edits.
fn get_property_gui_value(
    track: &AnimationTrack,
    prop_name: &str,
    time_ms: u64,
) -> Option<GuiPropertyValue> {
    match prop_name {
        "position" => track.position.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Vec2(v)
        }),
        "size" => track.size.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Vec2([v[0] * 2.0, v[1] * 2.0])
        }),
        "rotation" => track.rotation.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Float(v)
        }),
        "scale" => track.scale.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Float(v)
        }),
        "opacity" => track.opacity.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Float(v)
        }),
        "color" => track.color.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Color(v)
        }),
        "stroke_width" => track.stroke_width.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Float(v)
        }),
        "stroke_color" => track.stroke_color.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Color(v)
        }),
        "stroke_progress" => track.stroke_progress.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Float(v)
        }),
        "fill_opacity" => track.fill_opacity.as_ref().map(|t| {
            let v = t.evaluate_copy(time_ms);
            GuiPropertyValue::Float(v)
        }),
        _ => None,
    }
}

/// Check whether an actor has an actual property track for the given property name.
fn has_property_track(track: &AnimationTrack, prop_name: &str) -> bool {
    match prop_name {
        "position" => track.position.is_some(),
        "size" => track.size.is_some(),
        "rotation" => track.rotation.is_some(),
        "scale" => track.scale.is_some(),
        "opacity" => track.opacity.is_some(),
        "color" => track.color.is_some(),
        "stroke_width" => track.stroke_width.is_some(),
        "stroke_color" => track.stroke_color.is_some(),
        "stroke_progress" => track.stroke_progress.is_some(),
        "fill_opacity" => track.fill_opacity.is_some(),
        _ => false,
    }
}

/// Format an RGBA color array as a hex string.
fn format_rgba(rgba: [f32; 4]) -> String {
    let r = (rgba[0] * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = (rgba[1] * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = (rgba[2] * 255.0).round().clamp(0.0, 255.0) as u8;
    if rgba[3] >= 0.999 {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        let a = (rgba[3] * 255.0).round().clamp(0.0, 255.0) as u8;
        format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
    }
}
