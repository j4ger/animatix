use super::super::*;
use crate::app::design_tokens::spatial::Spatial;

pub(super) fn render_sum_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    variants: &'static [SumVariant],
    value: &PropertyValue,
) {
    let current_variant = match value {
        PropertyValue::Variant { name, .. } => name.as_str(),
        _ => "",
    };
    let mut selected =
        variants.iter().position(|variant| variant.name == current_variant).unwrap_or(0);
    let previous = selected;
    let mut buf = match value {
        PropertyValue::Variant { value: inner, .. } => match inner.as_ref() {
            PropertyValue::String(s) => s.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    };
    let mut label_changed = false;
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
                    for (index, variant) in variants.iter().enumerate() {
                        ui.selectable_value(&mut selected, index, variant.name);
                    }
                    if variants.get(selected).is_some_and(|v| v.value_type == ValueType::String) {
                        let tf_resp = TextField::new(&mut buf)
                            .desired_width(ui.available_width().max(60.0))
                            .show(ui);
                        label_changed = tf_resp.changed;
                    }
                },
            );
        },
    );
    if selected != previous {
        let edit_value = gui_value_for_sum_variant(&variants[selected], buf.clone());
        commands.push_back(
            Command::PropertyEdit(PropertyEdit::new(
                actor_label.to_string(),
                entry.name.to_string(),
                edit_value,
                keyframe_mode,
                None,
            ))
            .into(),
        );
    } else if label_changed {
        commands.push_back(
            Command::PropertyEdit(PropertyEdit::new(
                actor_label.to_string(),
                entry.name.to_string(),
                GuiPropertyValue::String(buf),
                keyframe_mode,
                None,
            ))
            .into(),
        );
    }
}

pub(super) fn render_union_input(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    input_rect: egui::Rect,
    theme: &eparts::Theme,
    sp: &Spatial,
    flat_style: &egui::Style,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    variants: &'static [ValueType],
    value: &PropertyValue,
) {
    if variants.contains(&ValueType::Bool) && variants.contains(&ValueType::String) {
        let (mut selected, mut buf) = match value {
            PropertyValue::Bool(true) => (0, String::new()),
            PropertyValue::Bool(false) => (1, String::new()),
            PropertyValue::String(label) => (2, label.clone()),
            _ => (0, String::new()),
        };
        let previous = selected;
        let mut label_changed = false;
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
            |ui| {
                *ui.style_mut() = flat_style.clone();
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                    |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
                        ui.selectable_value(&mut selected, 0, "Auto");
                        ui.selectable_value(&mut selected, 1, "Hidden");
                        ui.selectable_value(&mut selected, 2, "Label");
                        if selected == 2 {
                            let tf_resp = TextField::new(&mut buf)
                                .desired_width(ui.available_width().max(60.0))
                                .show(ui);
                            label_changed = tf_resp.changed;
                        }
                    },
                );
            },
        );
        if selected != previous {
            let edit_value = match selected {
                0 => GuiPropertyValue::Bool(true),
                1 => GuiPropertyValue::Bool(false),
                _ => GuiPropertyValue::String(buf.clone()),
            };
            commands.push_back(
                Command::PropertyEdit(PropertyEdit::new(
                    actor_label.to_string(),
                    entry.name.to_string(),
                    edit_value,
                    keyframe_mode,
                    None,
                ))
                .into(),
            );
        } else if label_changed {
            commands.push_back(
                Command::PropertyEdit(PropertyEdit::new(
                    actor_label.to_string(),
                    entry.name.to_string(),
                    GuiPropertyValue::String(buf),
                    keyframe_mode,
                    None,
                ))
                .into(),
            );
        }
    } else {
        let text = match value {
            PropertyValue::Bool(v) => v.to_string(),
            PropertyValue::String(v) => v.clone(),
            PropertyValue::F32(v) => format!("{v:.2}"),
            PropertyValue::U32(v) => v.to_string(),
            other => format!("{other:?}"),
        };
        ui.add(
            egui::Label::new(
                egui::RichText::new(text.as_str())
                    .size(TextRole::Body.size())
                    .color(theme.palette.text.muted),
            )
            .selectable(false),
        );
    }
}

pub(super) fn render_text_input(
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
    text: &String,
) {
    let mut buf = text.clone();
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0))),
        |ui| {
            *ui.style_mut() = flat_style.clone();
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    if entry.name == "shape_type" {
                        use std::sync::OnceLock;
                        static SHAPE_VARIANTS: OnceLock<Vec<&'static str>> = OnceLock::new();
                        let variants = SHAPE_VARIANTS.get_or_init(|| {
                            vec![
                                ShapeType::Rect.as_str(),
                                ShapeType::Ellipse.as_str(),
                                ShapeType::Line.as_str(),
                                ShapeType::Polygon.as_str(),
                                ShapeType::Path.as_str(),
                            ]
                        });
                        // Map current text to Option<usize> index for Select
                        let mut sel_idx = variants.iter().position(|v| *v == text);
                        ui.add_sized(
                            Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                            Select::new(
                                ui.scope_id().with(("enum", entry.name.as_str())),
                                &mut sel_idx,
                                &variants[..],
                            ),
                        );
                        if let Some(idx) = sel_idx {
                            let chosen = variants[idx];
                            if chosen != text {
                                commands.push_back(
                                    Command::PropertyEdit(PropertyEdit::new(
                                        actor_label.to_string(),
                                        entry.name.to_string(),
                                        GuiPropertyValue::String(chosen.to_string()),
                                        keyframe_mode,
                                        None,
                                    ))
                                    .into(),
                                );
                            }
                        }
                    } else if entry.name == "font_family" {
                        let font_ctx = crate::fonts::system_font_context();
                        let families = animatix_text::available_font_families(font_ctx);
                        let mut sel_idx = families.iter().position(|f| f == text);
                        ui.add_sized(
                            Vec2::new(ui.available_width(), row_height - sp.base.space_2),
                            Select::new(
                                ui.scope_id().with(("font", entry.name.as_str())),
                                &mut sel_idx,
                                &families[..],
                            )
                            .placeholder("Select font…")
                            .searchable(true),
                        );
                        if let Some(idx) = sel_idx {
                            let chosen = &families[idx];
                            if chosen != text {
                                commands.push_back(
                                    Command::PropertyEdit(PropertyEdit::new(
                                        actor_label.to_string(),
                                        entry.name.to_string(),
                                        GuiPropertyValue::String(chosen.clone()),
                                        keyframe_mode,
                                        None,
                                    ))
                                    .into(),
                                );
                            }
                        }
                    } else if entry.name == "text_content"
                        || entry.name == "text"
                        || entry.name == "source"
                    {
                        let tf_resp =
                            TextField::new(&mut buf).desired_width(ui.available_width()).show(ui);
                        if tf_resp.changed {
                            commands.push_back(
                                Command::PropertyEdit(PropertyEdit::new(
                                    actor_label.to_string(),
                                    entry.name.to_string(),
                                    GuiPropertyValue::String(buf),
                                    keyframe_mode,
                                    None,
                                ))
                                .into(),
                            );
                        }
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text.as_str())
                                    .size(TextRole::Body.size())
                                    .color(theme.palette.text.muted),
                            )
                            .selectable(false),
                        );
                    }
                },
            );
        },
    );
}
