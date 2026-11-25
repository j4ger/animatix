//! Inspection deliberately has no document mutation commands or value editors.
use super::property_groups::{PropertyKind, build_property_groups};
use super::{InspectorContext, source};
use crate::app::commands::Command;
use crate::app::i18n::{current_language, empty_state, format, text, with_arg};
use crate::app::panels::panel_frame;
use animatix::timeline::{EntityId, Timeline};
use egui::{RichText, ScrollArea};

pub(crate) fn inspector_panel_ui(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        let theme = eparts::theme(ui);
        if !ctx.source_current {
            ui.label(
                RichText::new(crate::app::i18n::text(ui, "gui-preview-source-outdated"))
                    .color(theme.palette.status.warning),
            );
            ui.small(text(ui, "gui-source-navigation-resume"));
        }
        let Some(timeline) = ctx.timeline else {
            empty_state(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "gui-preview-no-built-scene",
                "gui-inspector-empty-help",
            );
            return;
        };
        ctx.selected_actors.retain(|label| timeline.has_actor(label));
        ScrollArea::vertical()
            .id_salt("readonly_inspector")
            .auto_shrink([false; 2])
            .show(ui, |ui| match ctx.selected_actors.len() {
                0 => scene_summary(ctx, timeline, ui),
                1 => {
                    let label = ctx.selected_actors.iter().next().cloned().unwrap_or_default();
                    if let Some(id) = timeline.get_entity_id(&label) {
                        actor_summary(ctx, timeline, id, &label, ui);
                    }
                },
                _ => selection_summary(ctx, ui),
            });
    });
}

fn scene_summary(ctx: &InspectorContext<'_>, timeline: &Timeline, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    ui.strong(
        ctx.active_scene
            .map(str::to_owned)
            .unwrap_or_else(|| text(ui, "gui-inspector-scene")),
    );
    let mut rows = vec![
        (
            "gui-inspector-canvas",
            format!("{} × {}", ctx.scene_dimensions.width, ctx.scene_dimensions.height),
        ),
        ("gui-inspector-local-duration", format!("{:.3} s", timeline.duration_seconds())),
        ("gui-inspector-objects", timeline.tracks().len().to_string()),
    ];
    if let Some(composition) = ctx.composition {
        rows.push((
            "gui-inspector-composition",
            format(current_language(ui.ctx()), "gui-inspector-scene-count", {
                let mut args = animatix_i18n::FluentArgs::new();
                args.set("count", composition.scenes.len());
                args.set("duration", format!("{:.3}", composition.global_duration_s));
                args
            }),
        ));
    }
    value_rows(ui, rows);
    ui.add_space(crate::app::design_tokens::spatial::SPACE_5);
    ui.label(crate::app::i18n::text(ui, "gui-inspector-select-object"));
}

fn selection_summary(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    ui.strong(with_arg(
        current_language(ui.ctx()),
        "gui-inspector-selection-count",
        "count",
        ctx.selected_actors.len(),
    ));
    ui.small(text(ui, "gui-inspector-selected-help"));
    let mut labels: Vec<_> = ctx.selected_actors.iter().cloned().collect();
    labels.sort();
    for label in labels {
        if ui.button(&label).clicked() {
            ctx.selected_actors.clear();
            ctx.selected_actors.insert(label);
        }
    }
}

fn actor_summary(
    ctx: &mut InspectorContext<'_>,
    timeline: &Timeline,
    id: EntityId,
    label: &str,
    ui: &mut egui::Ui,
) {
    ui.add_space(8.0);
    ui.heading(label);
    let actor_type = timeline.world().actor_type(id);
    ui.label(actor_type);
    ui.add_space(10.0);
    source_card(ctx, timeline, id, label, actor_type, ui);
    ui.add_space(12.0);
    ui.strong(text(ui, "gui-inspector-playhead"));
    let global_s = ctx.preview.playback.current_time_s();
    let local_s = local_time(ctx, global_s);
    let mut rows = vec![("gui-inspector-scene-time", format!("{local_s:.3} s"))];
    if ctx.composition.is_some() {
        rows.push(("gui-inspector-global-time", format!("{global_s:.3} s")));
    }
    let parent = timeline.world().hierarchy(id).parent.and_then(|p| timeline.get_entity_label(p));
    rows.push((
        "gui-inspector-parent",
        parent
            .map(str::to_owned)
            .unwrap_or_else(|| text(ui, "gui-inspector-scene-root")),
    ));
    if let Some(bounds) = timeline.world().local_bounds(id) {
        rows.push((
            "gui-inspector-local-bounds",
            format!(
                "({:.1}, {:.1}) · {:.1} × {:.1}",
                bounds.x0,
                bounds.y0,
                bounds.width(),
                bounds.height()
            ),
        ));
    }
    value_rows(ui, rows);
    ui.small(text(ui, "gui-inspector-samples-help"));
    ui.add_space(10.0);
    let time_ms = (local_s.max(0.0) * 1000.0) as u64;
    for group in build_property_groups(timeline, id, time_ms) {
        egui::CollapsingHeader::new(format!("{}  {}", group.icon, group_label(ui, group.name)))
            .id_salt(("inspect_group", label, group.name))
            .default_open(matches!(group.name, "Transform" | "Text" | "Layout"))
            .show(ui, |ui| {
                for property in group.properties {
                    ui.push_id(&property.name, |ui| {
                        ui.label(RichText::new(&property.name).small());
                        ui.add(
                            egui::Label::new(
                                RichText::new(display_value(&property.kind)).monospace(),
                            )
                            .wrap(),
                        )
                        .on_hover_text(value_type_hint(ui, &property.kind));
                        if property.has_keyframes {
                            let key = if property.has_keyframe_at_current_time {
                                "gui-inspector-key-count-at-time"
                            } else {
                                "gui-inspector-key-count"
                            };
                            ui.small(with_arg(
                                current_language(ui.ctx()),
                                key,
                                "count",
                                property.keyframe_count,
                            ));
                        }
                        ui.add_space(5.0);
                    });
                }
            });
    }
    let assets: Vec<_> = timeline.asset_cache().assets_for(label).collect();
    if !assets.is_empty() {
        ui.separator();
        ui.strong(text(ui, "gui-inspector-assets"));
        for asset in assets {
            ui.add(egui::Label::new(asset).wrap());
        }
    }
}

fn group_label(ui: &egui::Ui, group: &str) -> String {
    let message = match group {
        "Transform" => "gui-inspector-group-transform",
        "Layout" => "gui-inspector-group-layout",
        "Style" => "gui-inspector-group-style",
        "Shape" => "gui-inspector-group-shape",
        "Text" => "gui-inspector-group-text",
        "Plot & Coordinates" => "gui-inspector-group-plot-coordinates",
        "Annotation" => "gui-inspector-group-annotation",
        "Media" => "gui-inspector-group-media",
        "Audio" => "gui-inspector-group-audio",
        "Visibility" => "gui-inspector-group-visibility",
        "Other" => "gui-inspector-group-other",
        "Geometry" => "gui-inspector-group-geometry",
        "Filter" => "gui-inspector-group-filter",
        _ => return group.to_owned(),
    };
    text(ui, message)
}

fn source_card(
    ctx: &mut InspectorContext<'_>,
    timeline: &Timeline,
    id: EntityId,
    label: &str,
    ty: &str,
    ui: &mut egui::Ui,
) {
    ui.strong(text(ui, "gui-inspector-source"));
    let location = if ctx.source_current {
        cached_source_location(ctx, timeline, id, label, ty, ui)
    } else {
        None
    };
    if let Some(span) = location {
        ui.add(egui::Label::new(ctx.source_path.display().to_string()).wrap());
        if ui
            .link(with_arg(
                current_language(ui.ctx()),
                "gui-inspector-go-to-declaration",
                "position",
                format!("{}:{}", span.start_line, span.start_col),
            ))
            .clicked()
        {
            ctx.commands
                .push_back(Command::ScrollToLine(span.start_line - 1, span.start_col - 1).into());
        }
    } else {
        ui.small(text(ui, "gui-inspector-no-declaration"));
        ui.small(text(ui, "gui-inspector-origins"));
    }
}

pub(super) fn local_time(ctx: &InspectorContext<'_>, global_s: f64) -> f64 {
    ctx.composition
        .zip(ctx.active_scene)
        .and_then(|(comp, scene)| comp.scene_start_times.get(scene))
        .map_or(global_s, |start| (global_s - start).max(0.0))
}

fn value_rows(ui: &mut egui::Ui, rows: Vec<(&str, String)>) {
    use egui::{Label, TextWrapMode, WidgetText};
    let gap = eparts::spatial(ui).space_3;
    let mut key_width = 0.0_f32;
    let mut value_width = 0.0_f32;
    for (key, value) in &rows {
        let key = WidgetText::from(RichText::new(text(ui, key)).small().weak());
        let value = WidgetText::from(RichText::new(value).monospace());
        key_width = key_width.max(
            key.into_galley(
                ui,
                Some(TextWrapMode::Extend),
                f32::INFINITY,
                egui::FontSelection::Default,
            )
            .size()
            .x,
        );
        value_width = value_width.max(
            value
                .into_galley(
                    ui,
                    Some(TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::FontSelection::Default,
                )
                .size()
                .x,
        );
    }
    // Switch the whole group so neighbouring rows never drift to different columns.
    let columns = key_width + gap + value_width <= ui.available_width();
    for (key, value) in rows {
        let key = Label::new(RichText::new(text(ui, key)).small().weak());
        let value = Label::new(RichText::new(value).monospace());
        if columns {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(
                    egui::vec2(key_width, 0.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(key_width);
                        ui.add(key.extend());
                    },
                );
                ui.add(value.extend());
            });
        } else {
            ui.add(key.wrap());
            ui.add(value.wrap());
            ui.add_space(eparts::spatial(ui).space_1);
        }
    }
}

pub(super) fn display_value(kind: &PropertyKind) -> String {
    match kind {
        PropertyKind::Vec2 { x, y } => format!("({x:.2}, {y:.2})"),
        PropertyKind::Float(v) => format!("{v:.3}"),
        PropertyKind::U32(v) => v.to_string(),
        PropertyKind::Bool(v) => v.to_string(),
        PropertyKind::Color(v) => super::property_groups_legend::color_display(*v),
        PropertyKind::Text(v) => {
            if v.is_empty() {
                "\"\"".to_owned()
            } else {
                v.clone()
            }
        },
        PropertyKind::Enum { value, .. } | PropertyKind::EnumOwned { value, .. } => value.clone(),
        PropertyKind::Union { value, .. } | PropertyKind::Sum { value, .. } => format!("{value:?}"),
    }
}

fn value_type_hint(ui: &egui::Ui, kind: &PropertyKind) -> String {
    let language = current_language(ui.ctx());
    match kind {
        PropertyKind::Enum { variants, .. } => {
            with_arg(language, "gui-inspector-value-choices", "choices", variants.join(", "))
        },
        PropertyKind::EnumOwned { variants, .. } => {
            with_arg(language, "gui-inspector-value-choices", "choices", variants.join(", "))
        },
        PropertyKind::Union { variants, .. } => {
            with_arg(language, "gui-inspector-value-types", "count", variants.len())
        },
        PropertyKind::Sum { variants, .. } => with_arg(
            language,
            "gui-inspector-value-variants",
            "variants",
            variants.iter().map(|v| v.name).collect::<Vec<_>>().join(", "),
        ),
        _ => text(ui, "gui-inspector-readonly-help"),
    }
}

#[derive(Clone)]
struct SourceLocationCache {
    fingerprint: u64,
    location: Option<animatix_syntax::ast::Span>,
}

fn cached_source_location(
    ctx: &InspectorContext<'_>,
    timeline: &Timeline,
    id: EntityId,
    label: &str,
    ty: &str,
    ui: &egui::Ui,
) -> Option<animatix_syntax::ast::Span> {
    use std::hash::{Hash, Hasher};
    let runtime_span = timeline.world().decl_span(id);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (
        ctx.source_text,
        ctx.active_scene,
        label,
        ty,
        runtime_span.map(|s| (s.start_line, s.start_col, s.end_line, s.end_col)),
    )
        .hash(&mut hasher);
    let fingerprint = hasher.finish();
    let cache_id = egui::Id::unique("inspector_source_location");
    if let Some(cache) = ui.ctx().data(|data| data.get_temp::<SourceLocationCache>(cache_id))
        && cache.fingerprint == fingerprint
    {
        return cache.location;
    }
    let location = ctx.raw_statements.and_then(|stmts| {
        source::parsed_declaration(
            ctx.source_text,
            stmts,
            ctx.active_scene,
            label,
            ty,
            runtime_span,
        )
    });
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            cache_id,
            SourceLocationCache {
                fingerprint,
                location,
            },
        )
    });
    location
}

#[cfg(test)]
mod summary_layout_tests {
    use super::*;

    #[test]
    fn compact_values_align_in_two_columns_and_stack_when_space_runs_out() {
        fn rendered_text(width: f32) -> Vec<egui::epaint::TextShape> {
            let ctx = egui::Context::default();
            crate::fonts::install_fonts(&ctx);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 300.0),
                )),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                value_rows(
                    ui,
                    vec![
                        ("gui-inspector-canvas", "1,920 × 1,080".into()),
                        ("gui-inspector-local-duration", "20.000 s".into()),
                        ("gui-inspector-objects", "6".into()),
                    ],
                );
            });
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .filter_map(|clipped| match clipped.shape {
                    egui::Shape::Text(text) => Some(text),
                    _ => None,
                })
                .collect()
        }
        for width in [240.0, 118.0] {
            let texts = rendered_text(width);
            let find = |label| texts.iter().find(|text| text.galley.job.text == label).unwrap();
            let keys = ["Canvas", "Local duration", "Objects"].map(find);
            let values = ["1,920 × 1,080", "20.000 s", "6"].map(find);
            assert!(keys.iter().all(|key| key.pos.x == keys[0].pos.x));
            assert!(values.iter().all(|value| value.pos.x == values[0].pos.x));
            assert!(values.iter().all(|value| value.pos.x + value.galley.size().x <= width));
            if width > 200.0 {
                for (key, value) in keys.into_iter().zip(values) {
                    assert!(value.pos.x > key.pos.x + key.galley.size().x);
                    assert!((value.pos.y - key.pos.y).abs() < 8.0);
                }
            } else {
                for (key, value) in keys.into_iter().zip(values) {
                    assert_eq!(value.pos.x, key.pos.x);
                    assert!(value.pos.y >= key.pos.y + key.galley.size().y);
                }
            }
        }
    }
}
