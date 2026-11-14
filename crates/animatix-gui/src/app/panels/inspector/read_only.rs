//! Inspection deliberately has no document mutation commands or value editors.
use super::property_groups::{PropertyKind, build_property_groups};
use super::{InspectorContext, source};
use crate::app::commands::Command;
use crate::app::components::layout;
use crate::app::panels::panel_frame;
use animatix::timeline::{EntityId, Timeline};
use egui::{RichText, ScrollArea};

pub(crate) fn inspector_panel_ui(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        let theme = eparts::theme(ui);
        ui.horizontal(|ui| {
            ui.heading("Inspect");
            ui.label(RichText::new("READ ONLY").small().color(theme.palette.text.muted));
        });
        ui.label(
            RichText::new("Observe here. Change the source.")
                .small()
                .color(theme.palette.text.secondary),
        );
        ui.separator();
        if !ctx.source_current {
            ui.label(
                RichText::new("Last built scene · source is out of date")
                    .color(theme.palette.status.warning),
            );
            ui.small("Source links resume after a successful rebuild.");
        }
        let Some(timeline) = ctx.timeline else {
            layout::empty_state(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "No built scene",
                "Open an .amx file to inspect its preview.",
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
    ui.strong(ctx.active_scene.unwrap_or("Scene"));
    value_row(
        ui,
        "Canvas",
        format!("{} × {}", ctx.scene_dimensions.width, ctx.scene_dimensions.height),
    );
    value_row(ui, "Local duration", format!("{:.3} s", timeline.duration_seconds()));
    value_row(ui, "Objects", timeline.tracks().len().to_string());
    if let Some(composition) = ctx.composition {
        value_row(
            ui,
            "Composition",
            format!("{} scenes · {:.3} s", composition.scenes.len(), composition.global_duration_s),
        );
    }
    ui.add_space(16.0);
    ui.label("Select an object in the preview or outline.");
    ui.small(
        "Selection never moves or modifies an object. Use the timeline to inspect another moment.",
    );
}

fn selection_summary(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    ui.strong(format!("{} objects selected", ctx.selected_actors.len()));
    ui.small("Choose one to inspect its source and sampled values.");
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
    ui.strong("At the playhead");
    let global_s = ctx.preview.playback.current_time_s();
    let local_s = local_time(ctx, global_s);
    value_row(ui, "Scene time", format!("{local_s:.3} s"));
    if ctx.composition.is_some() {
        value_row(ui, "Global time", format!("{global_s:.3} s"));
    }
    ui.small("Sampled tracks and defaults, not a post-layout property dump. Expressions and layout can change the final rendered result.");
    let parent = timeline.world().hierarchy(id).parent.and_then(|p| timeline.get_entity_label(p));
    value_row(ui, "Parent", parent.unwrap_or("Scene root").to_owned());
    if let Some(bounds) = timeline.world().local_bounds(id) {
        value_row(
            ui,
            "Last measured local bounds",
            format!(
                "({:.1}, {:.1}) · {:.1} × {:.1}",
                bounds.x0,
                bounds.y0,
                bounds.width(),
                bounds.height()
            ),
        );
    }
    ui.add_space(10.0);
    let time_ms = (local_s.max(0.0) * 1000.0) as u64;
    for group in build_property_groups(timeline, id, time_ms) {
        egui::CollapsingHeader::new(format!("{}  {}", group.icon, group.name))
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
                        .on_hover_text(value_type_hint(&property.kind));
                        if property.has_keyframes {
                            ui.small(format!(
                                "{} keys{}",
                                property.keyframe_count,
                                if property.has_keyframe_at_current_time {
                                    " · key at this time"
                                } else {
                                    ""
                                }
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
        ui.strong("Referenced assets");
        for asset in assets {
            ui.add(egui::Label::new(asset).wrap());
        }
    }
}

fn source_card(
    ctx: &mut InspectorContext<'_>,
    timeline: &Timeline,
    id: EntityId,
    label: &str,
    ty: &str,
    ui: &mut egui::Ui,
) {
    ui.strong("Source");
    let location = if ctx.source_current {
        cached_source_location(ctx, timeline, id, label, ty, ui)
    } else {
        None
    };
    if let Some(span) = location {
        ui.add(egui::Label::new(ctx.source_path.display().to_string()).wrap());
        if ui
            .link(format!("Go to declaration · {}:{}", span.start_line, span.start_col))
            .clicked()
        {
            ctx.commands
                .push_back(Command::ScrollToLine(span.start_line - 1, span.start_col - 1).into());
        }
    } else {
        ui.small("No verified direct declaration in this source snapshot.");
        ui.small("Generated, imported and component objects may have multiple origins. Navigation is intentionally unavailable rather than guessed.");
    }
}

pub(super) fn local_time(ctx: &InspectorContext<'_>, global_s: f64) -> f64 {
    ctx.composition
        .zip(ctx.active_scene)
        .and_then(|(comp, scene)| comp.scene_start_times.get(scene))
        .map_or(global_s, |start| (global_s - start).max(0.0))
}

fn value_row(ui: &mut egui::Ui, name: &str, value: String) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(name).small().weak());
        ui.label(value);
    });
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

fn value_type_hint(kind: &PropertyKind) -> String {
    match kind {
        PropertyKind::Enum { variants, .. } => format!("Choices: {}", variants.join(", ")),
        PropertyKind::EnumOwned { variants, .. } => format!("Choices: {}", variants.join(", ")),
        PropertyKind::Union { variants, .. } => format!("{} accepted value types", variants.len()),
        PropertyKind::Sum { variants, .. } => {
            format!("Variants: {}", variants.iter().map(|v| v.name).collect::<Vec<_>>().join(", "))
        },
        _ => "Read-only sampled value. Edit the source to change it.".to_owned(),
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
    let cache_id = egui::Id::new("inspector_source_location");
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
