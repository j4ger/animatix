use super::super::*;
use crate::app::design_tokens::spatial::Spatial;

pub(super) fn render_vec2_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    x: &f32,
    y: &f32,
) {
    let nx = *x;
    let ny = *y;
    let (a_label, b_label) = vec2_labels(&entry.name);
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                    let half_w = (ui.available_width() - sp.base.space_2) / 2.0;
                    let mut nfx = nx as f64;
                    let rx = NumberField::new(&mut nfx)
                        .prefix(a_label)
                        .speed(0.5)
                        .desired_width(half_w.max(20.0))
                        .show(ui);
                    let mut nfy = ny as f64;
                    let ry = NumberField::new(&mut nfy)
                        .prefix(b_label)
                        .speed(0.5)
                        .desired_width(half_w.max(20.0))
                        .show(ui);
                    let nx_out = nfx as f32;
                    let ny_out = nfy as f32;
                    if rx.drag_started() || ry.drag_started() {
                        commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                    }
                    if rx.drag_stopped() || ry.drag_stopped() {
                        commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                    }
                    if rx.changed() || ry.changed() {
                        commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Vec2([nx_out, ny_out]),
                                create_keyframe: keyframe_mode,
                            })
                            .into(),
                        );
                    }
                },
            );
        },
    );
}

pub(super) fn render_float_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    theme: &eparts::Theme,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    row_height: f32,
    v: &f32,
) {
    let mut nv = *v;
    let is_01 = matches!(
        entry.name.as_str(),
        "opacity" | "fill_opacity" | "stroke_progress" | "sepia" | "volume"
    );
    let is_angle = entry.name == "rotation" || entry.name == "hue_rotate";
    let unit = unit_suffix(&entry.name);
    if is_01 {
        let mut nv_f64 = nv as f64;
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
            |ui| {
                *ui.style_mut() = flat_style.clone();
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                    |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                        let slider_w = ui.available_width() * 0.55;
                        let slider = ui.add_sized(
                            Vec2::new(slider_w.max(40.0), row_height - sp.base.space_2),
                            Slider::new(&mut nv_f64, 0.0..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        nv = nv_f64 as f32;
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("{:.2}", nv))
                                    .font(TextRole::MonoDigits.font_id())
                                    .size(TextRole::Micro.size())
                                    .color(theme.palette.text.primary),
                            )
                            .selectable(false),
                        );
                        if slider.drag_started() {
                            commands
                                .push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                        }
                        if slider.drag_stopped() {
                            commands
                                .push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                        }
                        if slider.changed() {
                            commands.push_back(
                                DocumentCommand::PropertyEdit(PropertyEdit {
                                    time_s: None,
                                    actor: actor_label.to_string(),
                                    property: entry.name.to_string(),
                                    value: GuiPropertyValue::F32(nv),
                                    create_keyframe: keyframe_mode,
                                })
                                .into(),
                            );
                        }
                    },
                );
            },
        );
    } else {
        // Non-0..1 float: use eparts NumberField
        let mut nv = *v as f64;
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
            |ui| {
                *ui.style_mut() = flat_style.clone();
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                    |ui| {
                        let response = NumberField::new(&mut nv)
                            .speed(if is_angle { 0.5 } else { 0.1_f32 })
                            .suffix(if is_angle { "°" } else { unit })
                            .desired_width(ui.available_width())
                            .show(ui);
                        if response.drag_started() {
                            commands
                                .push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                        }
                        if response.drag_stopped() {
                            commands
                                .push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                        }
                        if response.changed() {
                            let out_val = if is_angle { nv.to_radians() } else { nv };
                            commands.push_back(
                                DocumentCommand::PropertyEdit(PropertyEdit {
                                    time_s: None,
                                    actor: actor_label.to_string(),
                                    property: entry.name.to_string(),
                                    value: GuiPropertyValue::F32(out_val as f32),
                                    create_keyframe: keyframe_mode,
                                })
                                .into(),
                            );
                        }
                    },
                );
            },
        );
    }
}

pub(super) fn render_u32_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    v: &u32,
) {
    let mut nv = *v as f64;
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    let response = NumberField::new(&mut nv)
                        .speed(0.1_f32)
                        .desired_width(ui.available_width())
                        .show(ui);
                    if response.drag_started() {
                        commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragStarted));
                    }
                    if response.drag_stopped() {
                        commands.push_back(ShellAction::Drag(DragEvent::InspectorInputDragEnded));
                    }
                    if response.changed() {
                        commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::F32(nv as u32 as f32),
                                create_keyframe: keyframe_mode,
                            })
                            .into(),
                        );
                    }
                },
            );
        },
    );
}

pub(super) fn render_bool_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    value: &bool,
) {
    let mut checked = *value;
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    if ui.add(Checkbox::new(&mut checked)).changed() {
                        commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Bool(checked),
                                create_keyframe: keyframe_mode,
                            })
                            .into(),
                        );
                    }
                },
            );
        },
    );
}

pub(super) fn render_color_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    theme: &eparts::Theme,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    rgba: &[f32; 4],
) {
    let mut color = Color32::from_rgba_premultiplied(
        (rgba[0] * 255.0) as u8,
        (rgba[1] * 255.0) as u8,
        (rgba[2] * 255.0) as u8,
        (rgba[3] * 255.0) as u8,
    );
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
                    let response =
                        ColorPicker::new(ui.id().with(("color", entry.name.as_str())), &mut color)
                            .swatches(&[
                                theme.palette.accent.primary,
                                theme.palette.status.success,
                                theme.palette.status.warning,
                                theme.palette.status.error,
                                theme.palette.text.primary,
                                theme.palette.text.muted,
                                theme.palette.surface.surface,
                                theme.palette.surface.widget,
                            ])
                            .show(ui);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(color_to_hex_rgba(color))
                                .font(TextRole::MonoDigits.font_id())
                                .size(TextRole::Micro.size())
                                .color(theme.palette.text.muted),
                        )
                        .selectable(false),
                    );
                    if response.changed {
                        let [r, g, b, a] = color.to_array();
                        commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor_label.to_string(),
                                property: entry.name.to_string(),
                                value: GuiPropertyValue::Color([
                                    r as f32 / 255.0,
                                    g as f32 / 255.0,
                                    b as f32 / 255.0,
                                    a as f32 / 255.0,
                                ]),
                                create_keyframe: keyframe_mode,
                            })
                            .into(),
                        );
                    }
                },
            );
        },
    );
}

pub(super) fn render_enum_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    row_height: f32,
    variants: &&'static [&'static str],
    value: &String,
) {
    let mut selected = variants.iter().position(|variant| *variant == value);
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.add_sized(
                Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                Select::new(ui.id().with(("enum", entry.name.as_str())), &mut selected, variants),
            );
        },
    );
    if let Some(selected) = selected {
        let chosen = variants[selected];
        if chosen != value {
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor_label.to_string(),
                    property: entry.name.to_string(),
                    value: GuiPropertyValue::String(chosen.to_string()),
                    create_keyframe: keyframe_mode,
                })
                .into(),
            );
        }
    }
}

pub(super) fn render_enum_owned_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    row_height: f32,
    variants: &Vec<String>,
    value: &String,
) {
    let mut selected = variants.iter().position(|variant| variant == value);
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            let selected_ref = &mut selected;
            ui.add_sized(
                Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                Select::new(
                    ui.id().with(("enum_owned", entry.name.as_str())),
                    selected_ref,
                    variants.as_slice(),
                ),
            );
        },
    );
    if let Some(selected) = selected {
        let chosen = variants[selected].clone();
        if chosen != *value {
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor_label.to_string(),
                    property: entry.name.to_string(),
                    value: GuiPropertyValue::String(chosen),
                    create_keyframe: keyframe_mode,
                })
                .into(),
            );
        }
    }
}
