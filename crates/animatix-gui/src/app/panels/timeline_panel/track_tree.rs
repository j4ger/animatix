//! Timeline tree hierarchy rendering: actor selection and row expansion.

use crate::app::i18n::{render, text};
use std::collections::HashSet;

use animatix::timeline::Timeline;
use egui::{Align2, Color32, Pos2, Rect, Sense, Vec2};

use crate::app::design_tokens::spatial::Spatial;
use crate::app::design_tokens::typography::TextRole;
use crate::app::utils::text::elide_to_width;

/// Build a flat list of (actor_label, depth) in tree order (depth-first).
pub(crate) fn build_actor_tree(
    timeline: &Timeline,
    collapsed: &HashSet<String>,
) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    for &root_id in timeline.world().root_entities() {
        let Some(root) = timeline.get_entity_label(root_id) else {
            continue;
        };
        add_actor_and_children(timeline, root, 0, collapsed, &mut result);
    }
    result
}

fn add_actor_and_children(
    timeline: &Timeline,
    label: &str,
    depth: usize,
    collapsed: &HashSet<String>,
    result: &mut Vec<(String, usize)>,
) {
    result.push((label.to_string(), depth));
    if !collapsed.contains(label) {
        for child in timeline.child_labels(label) {
            add_actor_and_children(timeline, child, depth + 1, collapsed, result);
        }
    }
}

/// Layout and tree state parameters for an actor row header.
pub(crate) struct ActorRowHeaderLayout<'a> {
    pub actor_label: &'a str,
    pub depth: usize,
    pub track_idx: usize,
    pub at_top: f32,
    pub at_bot: f32,
    pub scroll_rect: Rect,
    pub bar_origin_x: f32,
    pub is_selected: bool,
    pub is_collapsed: bool,
    pub has_children: bool,
    pub prop_expanded: bool,
}

impl ActorRowHeaderLayout<'_> {
    /// Full row rect across every column.
    fn track_rect(&self) -> Rect {
        Rect::from_min_max(
            Pos2::new(self.scroll_rect.left(), self.at_top),
            Pos2::new(self.scroll_rect.right(), self.at_bot),
        )
    }

    /// Label column: left edge through the timeline bar origin.
    fn label_rect(&self) -> Rect {
        Rect::from_min_max(
            Pos2::new(self.scroll_rect.left(), self.at_top),
            Pos2::new(self.bar_origin_x, self.at_bot),
        )
    }
}

/// Alternating row fill, selection highlight, and label column background.
fn paint_actor_row_background(
    painter: &egui::Painter,
    layout: &ActorRowHeaderLayout<'_>,
    theme: eparts::Theme,
) {
    let track_rect = layout.track_rect();
    if layout.track_idx.is_multiple_of(2) {
        painter.rect_filled(track_rect, 0.0, theme.components.list.odd.bg);
    }
    if layout.is_selected {
        painter.rect_filled(track_rect, 0.0, theme.palette.accent.selection);
        let accent = Rect::from_min_size(track_rect.min, Vec2::new(2.0, track_rect.height()));
        painter.rect_filled(accent, 0.0, theme.palette.accent.primary);
    }
    painter.rect_filled(layout.label_rect(), 0.0, theme.palette.surface.base);
}

/// Collapse/expand chevron for an actor that has children.
fn render_actor_chevron(
    ui: &egui::Ui,
    layout: &ActorRowHeaderLayout<'_>,
    chevron_x: f32,
    collapsed_actors: &mut HashSet<String>,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let actor_label = layout.actor_label;
    let chevron_icon = if layout.is_collapsed {
        egui_phosphor::regular::CARET_RIGHT
    } else {
        egui_phosphor::regular::CARET_DOWN
    };
    let chevron_rect = Rect::from_min_size(
        Pos2::new(chevron_x, layout.at_top + 2.0),
        Vec2::new(14.0, sp.timeline.track_row_height - 4.0),
    );
    let chevron_resp = ui.interact(
        chevron_rect,
        ui.scope_id().with(("actor_chevron", actor_label)),
        Sense::click(),
    );
    painter.text(
        chevron_rect.center(),
        Align2::CENTER_CENTER,
        chevron_icon,
        TextRole::BodyS.font_id(),
        if chevron_resp.hovered() {
            theme.palette.text.primary
        } else {
            theme.palette.text.muted
        },
    );
    if chevron_resp.clicked() {
        if layout.is_collapsed {
            collapsed_actors.remove(actor_label);
        } else {
            collapsed_actors.insert(actor_label.to_string());
        }
    }
}

/// Property-lane expand toggle (LIST icon) next to the chevron slot.
fn render_property_expand_toggle(
    ui: &egui::Ui,
    layout: &ActorRowHeaderLayout<'_>,
    prop_toggle_x: f32,
    expanded_properties: &mut HashSet<String>,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let actor_label = layout.actor_label;
    let prop_toggle_rect = Rect::from_min_size(
        Pos2::new(prop_toggle_x, layout.at_top + 2.0),
        Vec2::new(14.0, sp.timeline.track_row_height - 4.0),
    );
    let prop_toggle_resp = ui.interact(
        prop_toggle_rect,
        ui.scope_id().with(("prop_toggle", actor_label)),
        Sense::click(),
    );
    painter.text(
        prop_toggle_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::LIST,
        TextRole::Micro.font_id(),
        if layout.prop_expanded {
            theme.palette.accent.primary
        } else {
            theme.palette.text.muted
        },
    );
    let prop_toggle_resp = prop_toggle_resp.on_hover_text(text(
        ui,
        if layout.prop_expanded {
            "gui-timeline-hide-properties"
        } else {
            "gui-timeline-show-properties"
        },
    ));
    if prop_toggle_resp.clicked() {
        if layout.prop_expanded {
            expanded_properties.remove(actor_label);
        } else {
            expanded_properties.insert(actor_label.to_string());
        }
    }
}

/// Elided actor label plus click/shift-click selection.
fn render_actor_label(
    ui: &egui::Ui,
    layout: &ActorRowHeaderLayout<'_>,
    label_x: f32,
    selected_actors: &mut HashSet<String>,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let actor_label = layout.actor_label;
    let track_rect = layout.track_rect();
    let label_max_w = (layout.bar_origin_x - sp.base.space_2 - label_x).max(24.0);
    let label_font = TextRole::BodyS.font_id();
    let label_text = elide_to_width(actor_label, label_max_w, |candidate| {
        painter
            .layout_no_wrap(candidate.to_owned(), label_font.clone(), Color32::WHITE)
            .size()
            .x
    });
    painter.text(
        Pos2::new(label_x, track_rect.center().y),
        Align2::LEFT_CENTER,
        &label_text,
        label_font,
        if layout.is_selected {
            theme.palette.text.primary
        } else {
            theme.palette.text.secondary
        },
    );

    // The left controls have their own hit targets. Covering the whole gutter
    // with the later label interaction prevents either control receiving clicks.
    let mut label_hit = layout.label_rect();
    label_hit.min.x = label_x.clamp(label_hit.left(), label_hit.right());
    if label_hit.width() <= 0.0 {
        return;
    }
    let label_resp = ui.interact(
        label_hit,
        ui.scope_id().with(("actor_label", layout.track_idx)),
        Sense::click(),
    );
    if label_resp.clicked() {
        let modifiers = ui.ctx().input(|i| i.modifiers);
        let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
        if multi {
            if selected_actors.contains(actor_label) {
                selected_actors.remove(actor_label);
            } else {
                selected_actors.insert(actor_label.to_string());
            }
        } else {
            selected_actors.clear();
            selected_actors.insert(actor_label.to_string());
        }
        // Only scroll if the clicked row is not fully visible.
        let viewport = ui.clip_rect();
        let row_visible = layout.at_top >= viewport.top() && layout.at_bot <= viewport.bottom();
        if !row_visible {
            ui.scroll_to_rect(track_rect, Some(egui::Align::Center));
        }
    }
}

/// Render the left header column of an actor track row and its selection affordance.
pub(crate) fn render_actor_row_header(
    ui: &mut egui::Ui,
    layout: &ActorRowHeaderLayout<'_>,
    collapsed_actors: &mut HashSet<String>,
    expanded_properties: &mut HashSet<String>,
    selected_actors: &mut HashSet<String>,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    paint_actor_row_background(ui.painter(), layout, theme);

    let gutter = layout.bar_origin_x - layout.scroll_rect.left();
    let indent = (layout.depth as f32 * 14.0).min((gutter - 54.0).max(0.0));
    let chevron_x = layout.scroll_rect.left() + sp.base.space_2 + indent;
    if layout.has_children {
        render_actor_chevron(ui, layout, chevron_x, collapsed_actors, theme, sp);
    }

    let prop_toggle_x = chevron_x + if layout.has_children { 18.0 } else { 4.0 };
    render_property_expand_toggle(ui, layout, prop_toggle_x, expanded_properties, theme, sp);

    let label_x = prop_toggle_x + 16.0 + sp.base.space_2;
    render_actor_label(ui, layout, label_x, selected_actors, theme, sp);
}

/// Render the left header column of an expanded property lane row (colored dot, indented property name).
pub(crate) fn render_property_row_header(
    ui: &mut egui::Ui,
    prop_name: &str,
    depth: usize,
    track_idx: usize,
    prop_top: f32,
    prop_bot: f32,
    scroll_rect: Rect,
    bar_origin_x: f32,
    group_col: Color32,
    theme: eparts::Theme,
    sp: &Spatial,
    curve: Option<bool>,
) -> bool {
    let painter = ui.painter().clone();
    let prop_rect = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), prop_top),
        Pos2::new(scroll_rect.right(), prop_bot),
    );

    // Alternating row (offset from main track)
    if track_idx % 2 == 1 {
        painter.rect_filled(prop_rect, 0.0, theme.components.list.odd.bg);
    }

    // Label column background
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(scroll_rect.left(), prop_top),
            Pos2::new(bar_origin_x, prop_bot),
        ),
        0.0,
        theme.palette.surface.base,
    );

    let label_right = bar_origin_x - if curve.is_some() { 24.0 } else { 3.0 };
    let label_area = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), prop_top),
        Pos2::new(label_right, prop_bot),
    );
    let label_painter = painter.with_clip_rect(label_area.intersect(ui.clip_rect()));
    let depth_x = scroll_rect.left() + sp.base.space_2 + depth as f32 * 14.0 + 18.0;
    let dot_x = depth_x.min((label_right - 24.0).max(scroll_rect.left() + 2.0));
    label_painter.circle_filled(Pos2::new(dot_x + 3.0, prop_rect.center().y), 3.0, group_col);
    let font = TextRole::Micro.font_id();
    let label_x = dot_x + 10.0;
    let label = elide_to_width(prop_name, (label_right - label_x).max(0.0), |s| {
        label_painter
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    });
    label_painter.text(
        Pos2::new(label_x, prop_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        theme.palette.text.muted,
    );
    curve.is_some_and(|open| {
        let button_rect = curve_button_rect(bar_origin_x, prop_rect);
        if !ui.is_rect_visible(button_rect) {
            return false;
        }
        let tooltip = if open {
            render(ui, "gui-timeline-hide-curve", &[("property", prop_name.to_string())])
        } else {
            render(ui, "gui-timeline-show-curve", &[("property", prop_name.to_string())])
        };
        let response = ui.interact(
            button_rect,
            ui.scope_id().with(("property_curve", track_idx, prop_name)),
            Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &tooltip)
        });
        if open || response.hovered() || response.has_focus() {
            painter.rect_filled(
                button_rect.shrink(1.0),
                3.0,
                if open {
                    theme.components.tab.active.bg
                } else {
                    theme.components.tab.hover.bg
                },
            );
        }
        painter.text(
            button_rect.center(),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::CHART_LINE,
            TextRole::BodyS.font_id(),
            if open {
                theme.palette.accent.primary
            } else {
                theme.palette.text.secondary
            },
        );
        response.on_hover_text(tooltip).clicked()
    })
}

fn curve_button_rect(bar_origin_x: f32, row: Rect) -> Rect {
    Rect::from_center_size(
        Pos2::new(bar_origin_x - 12.0, row.center().y),
        Vec2::new(20.0, row.height()),
    )
}
