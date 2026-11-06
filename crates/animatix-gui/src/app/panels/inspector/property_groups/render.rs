use super::inputs::render_property_input;
use super::*;
use crate::app::design_tokens::spatial::Spatial;

pub(crate) fn render_property_group(
    ui: &mut egui::Ui,
    group: &PropertyGroup,
    actor_label: &str,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
    active_scene: Option<&str>,
) {
    let theme = eparts::theme(ui);
    let group_id = ui.scope_id().with(("prop_group", group.name));
    let mut expanded = ui.data(|d| d.get_temp::<bool>(group_id)).unwrap_or(true);
    let sp = spatial(ui);

    let header = row::Row::new(group.name)
        .height(sp.base.row_m)
        .icon(Some(group.icon))
        .has_children(true)
        .expanded(expanded)
        .right(|ui| {
            ui.add(Badge::new(group.properties.len().to_string()));
        });
    let response = header.show(ui, group_id.with("header"));

    if response.row_clicked || response.chevron_clicked {
        expanded = !expanded;
        ui.data_mut(|d| d.insert_temp(group_id, expanded));
    }

    if expanded {
        // Cache flat widget style once for all rows in this group
        let flat_style = {
            let mut s = (**ui.style()).clone();
            s.visuals.extreme_bg_color = Color32::TRANSPARENT;
            s.visuals.widgets.inactive.bg_fill = theme.palette.surface.widget;
            s.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
            s.visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            s.visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.active.bg_stroke = Stroke::NONE;
            s.visuals.widgets.open.bg_fill = Color32::TRANSPARENT;
            s.visuals.widgets.open.bg_stroke = Stroke::NONE;
            s
        };
        ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_1);
        for entry in &group.properties {
            render_property_row(
                ui,
                actor_label,
                entry,
                commands,
                keyframe_mode,
                current_time_s,
                &flat_style,
                active_scene,
            );
        }
        ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_2);
    }
    ui.add_space(sp.base.space_4);
}

/// Column geometry for one inspector property row.
///
/// [KF dot:14px] [Label: ~42%] [Gap:6px] [Input area: flex] [Gap:4px] [KF btn:14px]
struct RowColumns {
    label_rect: egui::Rect,
    input_rect: egui::Rect,
    kf_btn_rect: egui::Rect,
    kf_dot_center: egui::Pos2,
}

impl RowColumns {
    fn new(row_rect: egui::Rect, available: f32, sp: &Spatial) -> Self {
        let row_height = sp.inspector.row_height;
        let baseline_y = row_rect.center().y;
        let kf_col_right = row_rect.min.x + INSPECTOR_KF_COL_WIDTH;
        let label_width = (available * INSPECTOR_LABEL_WIDTH_FRAC)
            .clamp(INSPECTOR_LABEL_MIN_WIDTH, INSPECTOR_LABEL_MAX_WIDTH);
        let label_col_right = kf_col_right + label_width;
        let input_col_left = label_col_right + INSPECTOR_COL_GAP;
        let kf_btn_right = row_rect.max.x - sp.base.space_2;
        let kf_btn_left = kf_btn_right - INSPECTOR_KF_BTN_WIDTH;
        let input_col_right = kf_btn_left - sp.base.space_2;
        Self {
            label_rect: egui::Rect::from_min_max(
                egui::pos2(kf_col_right + sp.base.space_2, row_rect.min.y),
                egui::pos2(label_col_right, row_rect.max.y),
            ),
            input_rect: egui::Rect::from_min_max(
                egui::pos2(input_col_left, row_rect.min.y),
                egui::pos2(input_col_right, row_rect.max.y),
            ),
            kf_btn_rect: egui::Rect::from_min_size(
                egui::pos2(
                    kf_btn_left,
                    row_rect.min.y + (row_height - INSPECTOR_KF_BTN_WIDTH) * 0.5,
                ),
                Vec2::new(INSPECTOR_KF_BTN_WIDTH, INSPECTOR_KF_BTN_WIDTH),
            ),
            kf_dot_center: egui::pos2(row_rect.min.x + INSPECTOR_KF_COL_WIDTH / 2.0, baseline_y),
        }
    }
}

pub(crate) fn render_property_row(
    ui: &mut egui::Ui,
    actor_label: &str,
    entry: &PropertyEntry,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
    flat_style: &egui::Style,
    active_scene: Option<&str>,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    let row_height = sp.inspector.row_height;
    let available = ui.available_width();
    let (row_rect, row_response) =
        ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

    if row_response.hovered() {
        ui.painter().rect_filled(row_rect, 0.0, theme.palette.surface.hover);
    }

    let cols = RowColumns::new(row_rect, available, &sp);

    paint_keyframe_dot(ui, entry, cols.kf_dot_center, &theme);
    render_property_label(ui, entry, cols.label_rect, &theme);

    // Subtle background on hover for the input area
    if ui.rect_contains_pointer(cols.input_rect) {
        ui.painter().rect_filled(
            cols.input_rect.shrink2(Vec2::new(sp.base.space_2, 0.0)),
            RADIUS_S,
            theme.palette.surface.widget,
        );
    }

    render_keyframe_button(
        ui,
        actor_label,
        entry,
        commands,
        keyframe_mode,
        current_time_s,
        active_scene,
        &theme,
        &cols,
    );

    // Flat widget styling passed from the group renderer (cached once per group).
    render_property_input(
        ui,
        entry,
        cols.input_rect,
        &theme,
        &sp,
        flat_style,
        actor_label,
        commands,
        keyframe_mode,
        row_height,
    );
}

/// Keyframe indicator dot, centered in the KF column.
fn paint_keyframe_dot(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    dot_center: egui::Pos2,
    theme: &eparts::Theme,
) {
    if entry.has_keyframe_at_current_time {
        let dot = egui::Rect::from_center_size(dot_center, Vec2::new(6.0, 6.0));
        ui.painter().rect_filled(dot, 2.0, theme.palette.status.warning);
    } else if entry.has_keyframes {
        let dot = egui::Rect::from_center_size(dot_center, Vec2::new(5.0, 5.0));
        ui.painter().rect_filled(dot, 2.5, theme.palette.text.muted);
    }
}

/// Property label (truncated, vertically centered).
fn render_property_label(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    label_rect: egui::Rect,
    theme: &eparts::Theme,
) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(entry.name.as_str())
                            .size(TextRole::BodyS.size())
                            .color(theme.palette.text.secondary),
                    )
                    .truncate()
                    .selectable(false),
                );
            },
        );
    });
}

/// Keyframe toggle button (far right): diamond, tooltip, click, context menu.
fn render_keyframe_button(
    ui: &mut egui::Ui,
    actor_label: &str,
    entry: &PropertyEntry,
    commands: &mut ActionQueue,
    keyframe_mode: bool,
    current_time_s: f64,
    active_scene: Option<&str>,
    theme: &eparts::Theme,
    cols: &RowColumns,
) {
    let kf_btn_resp = ui.interact(
        cols.kf_btn_rect,
        ui.scope_id().with(("kf_btn", entry.name.as_str())),
        egui::Sense::click(),
    );

    paint_keyframe_diamond(ui, entry, &kf_btn_resp, cols.kf_btn_rect, theme);

    let kf_tip = if entry.has_keyframe_at_current_time {
        "Keyframe at the playhead — click to remove"
    } else if keyframe_mode {
        "Add a keyframe at the playhead (auto-key is on)"
    } else {
        "Add a keyframe at the playhead"
    };
    text_tooltip(ui, kf_btn_resp.id.with("kf_tip"), &kf_btn_resp, kf_tip);

    // Click toggles a keyframe at the playhead. Explicit keying works whether
    // or not auto-key is enabled, so creation is never unavailable.
    if kf_btn_resp.clicked() {
        if entry.has_keyframe_at_current_time {
            commands.push_back(
                Command::DeleteKeyframe {
                    scene: active_scene.map(ToOwned::to_owned),
                    actor: actor_label.to_string(),
                    property: entry.name.to_string(),
                    time_s: current_time_s,
                }
                .into(),
            );
        } else if let Some(value) = entry_to_gui_value(entry) {
            commands.push_back(
                Command::PropertyEdit(PropertyEdit::new(
                    actor_label.to_string(),
                    entry.name.to_string(),
                    value,
                    true,
                    None,
                ))
                .into(),
            );
        }
    }

    // Context menu on filled diamond: delete keyframe or change easing
    if entry.has_keyframe_at_current_time {
        show_keyframe_context_menu(
            &kf_btn_resp,
            actor_label,
            entry,
            commands,
            current_time_s,
            active_scene,
        );
    }
}

/// Draw the keyframe diamond. It is always visible: it is the explicit
/// "add/remove a key at the playhead" affordance, independent of auto-key.
fn paint_keyframe_diamond(
    ui: &mut egui::Ui,
    entry: &PropertyEntry,
    kf_btn_resp: &egui::Response,
    kf_btn_rect: egui::Rect,
    theme: &eparts::Theme,
) {
    let kf_color = if entry.has_keyframe_at_current_time {
        theme.palette.status.warning
    } else if entry.has_keyframes {
        if kf_btn_resp.hovered() {
            theme.palette.status.warning
        } else {
            theme.palette.text.muted
        }
    } else if kf_btn_resp.hovered() {
        theme.palette.text.secondary
    } else {
        theme.palette.text.disabled
    };
    if kf_color == Color32::TRANSPARENT {
        return;
    }
    let center = kf_btn_rect.center();
    let size = if entry.has_keyframe_at_current_time {
        5.5
    } else {
        4.5
    };
    let half = size * 0.5;
    let points = vec![
        center + Vec2::new(0.0, -half),
        center + Vec2::new(half, 0.0),
        center + Vec2::new(0.0, half),
        center + Vec2::new(-half, 0.0),
    ];
    if entry.has_keyframe_at_current_time {
        // Jewel ambient halo
        ui.painter().circle_filled(
            center,
            half * 1.8,
            Color32::from_rgba_unmultiplied(
                theme.palette.status.warning.r(),
                theme.palette.status.warning.g(),
                theme.palette.status.warning.b(),
                40,
            ),
        );
        ui.painter().add(egui::Shape::convex_polygon(points, kf_color, Stroke::NONE));
    } else {
        if kf_btn_resp.hovered() {
            ui.painter().circle_filled(center, half * 1.5, theme.palette.surface.hover);
        }
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            Color32::TRANSPARENT,
            Stroke::new(STROKE_WIDTH, kf_color),
        ));
    }
}

/// Context menu on a filled diamond: delete the keyframe or change its easing.
fn show_keyframe_context_menu(
    kf_btn_resp: &egui::Response,
    actor_label: &str,
    entry: &PropertyEntry,
    commands: &mut ActionQueue,
    current_time_s: f64,
    active_scene: Option<&str>,
) {
    kf_btn_resp.context_menu(|ui| {
        ui.set_min_width(140.0);
        ui.strong(format!("Keyframe @ {:.2}s", current_time_s));
        ui.separator();
        if ui
            .add(Button::danger("Delete").with_icon(egui_phosphor::regular::TRASH))
            .clicked()
        {
            commands.push_back(
                Command::DeleteKeyframe {
                    scene: active_scene.map(ToOwned::to_owned),
                    actor: actor_label.to_string(),
                    property: entry.name.to_string(),
                    time_s: current_time_s,
                }
                .into(),
            );
            ui.close();
        }
        ui.menu_button(format!("{} Easing", egui_phosphor::regular::WAVEFORM), |ui| {
            for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                let variant = animatix_syntax::easing::parse_easing_name(id_str)
                    .unwrap_or(animatix_syntax::easing::Easing::Linear);
                if ui.stable_selectable_label(false, display_name).clicked() {
                    commands.push_back(
                        Command::SetKeyframeEasing {
                            scene: active_scene.map(ToOwned::to_owned),
                            actor: actor_label.to_string(),
                            property: entry.name.to_string(),
                            time_s: current_time_s,
                            easing: variant,
                        }
                        .into(),
                    );
                    ui.close();
                }
            }
        });
    });
}
