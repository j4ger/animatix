//! Timeline tree hierarchy rendering: left sidebar label column, actor tree, collapse/expand,
//! visibility/lock/solo controls.

use std::collections::HashSet;

use animatix::timeline::{ActorField, PropertyValue, Timeline, read_property_value};
use egui::{Align2, Color32, Pos2, Rect, Sense, Vec2};

use crate::app::commands::{ActionQueue, ActorCommand, DocumentCommand, PropertyEdit};
use crate::app::components::text_tooltip;
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

/// Render the left header column of an actor track row (label, tree chevron, expand toggle, eye/solo/lock).
pub(crate) fn render_actor_row_header(
    ui: &mut egui::Ui,
    layout: &ActorRowHeaderLayout<'_>,
    timeline: Option<&Timeline>,
    collapsed_actors: &mut HashSet<String>,
    expanded_properties: &mut HashSet<String>,
    selected_actors: &mut HashSet<String>,
    commands: &mut ActionQueue,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let ActorRowHeaderLayout {
        actor_label,
        depth,
        track_idx,
        at_top,
        at_bot,
        scroll_rect,
        bar_origin_x,
        is_selected,
        is_collapsed,
        has_children,
        prop_expanded,
    } = *layout;
    let painter = ui.painter();
    let track_rect = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), at_top),
        Pos2::new(scroll_rect.right(), at_bot),
    );

    // Alternating row background
    if track_idx % 2 == 0 {
        painter.rect_filled(track_rect, 0.0, theme.components.list.odd.bg);
    }

    // Selection highlight
    if is_selected {
        painter.rect_filled(track_rect, 0.0, theme.palette.accent.selection);
        let accent = Rect::from_min_size(track_rect.min, Vec2::new(2.0, track_rect.height()));
        painter.rect_filled(accent, 0.0, theme.palette.accent.primary);
    }

    // Label column background
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(bar_origin_x, at_bot)),
        0.0,
        theme.palette.surface.base,
    );

    // Indent based on depth
    let indent = depth as f32 * 14.0;

    // Chevron toggle button (only if actor has children)
    let chevron_x = scroll_rect.left() + sp.base.space_2 + indent;
    if has_children {
        let chevron_icon = if is_collapsed {
            egui_phosphor::regular::CARET_RIGHT
        } else {
            egui_phosphor::regular::CARET_DOWN
        };
        let chevron_rect = Rect::from_min_size(
            Pos2::new(chevron_x, at_top + 2.0),
            Vec2::new(14.0, sp.timeline.track_row_height - 4.0),
        );
        let chevron_resp =
            ui.interact(chevron_rect, ui.id().with(("actor_chevron", actor_label)), Sense::click());
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
            if is_collapsed {
                collapsed_actors.remove(actor_label);
            } else {
                collapsed_actors.insert(actor_label.to_string());
            }
        }
    }

    // Solo / eye / lock live in a fixed strip at the right edge of the
    // label column; the label is clipped to `icon_strip_left` so the
    // two never overlap.
    const ICON_W: f32 = 16.0;
    const ICON_GAP: f32 = 2.0;
    let icon_strip_left = bar_origin_x - sp.base.space_2 - (ICON_W * 3.0 + ICON_GAP * 2.0);

    // Property expand toggle (LIST icon)
    let prop_toggle_x = chevron_x + if has_children { 18.0 } else { 4.0 };
    let prop_toggle_rect = Rect::from_min_size(
        Pos2::new(prop_toggle_x, at_top + 2.0),
        Vec2::new(14.0, sp.timeline.track_row_height - 4.0),
    );
    let prop_toggle_resp =
        ui.interact(prop_toggle_rect, ui.id().with(("prop_toggle", actor_label)), Sense::click());
    painter.text(
        prop_toggle_rect.center(),
        Align2::CENTER_CENTER,
        egui_phosphor::regular::LIST,
        TextRole::Micro.font_id(),
        if prop_expanded {
            theme.palette.accent.primary
        } else {
            theme.palette.text.muted
        },
    );
    if prop_toggle_resp.clicked() {
        if prop_expanded {
            expanded_properties.remove(actor_label);
        } else {
            expanded_properties.insert(actor_label.to_string());
        }
    }

    // Track label — elided to the icon strip, not to a fixed character
    // count: solo/eye/lock claim a fixed slice of the label column, so
    // a character-count cut still runs long names under the icons.
    let label_x = prop_toggle_x + 16.0 + sp.base.space_2;
    let label_max_w = (icon_strip_left - sp.base.space_2 - label_x).max(24.0);
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
        if is_selected {
            theme.palette.text.primary
        } else {
            theme.palette.text.secondary
        },
    );

    // Click on label to select actor
    let label_area =
        Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(bar_origin_x, at_bot));
    let label_resp =
        ui.interact(label_area, ui.id().with(("actor_label", track_idx)), Sense::click());
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
        // Only scroll if the clicked row is not fully visible
        let viewport = ui.clip_rect();
        let row_visible = at_top >= viewport.top() && at_bot <= viewport.bottom();
        if !row_visible {
            let track_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), at_top),
                Pos2::new(scroll_rect.right(), at_bot),
            );
            ui.scroll_to_rect(track_rect, Some(egui::Align::Center));
        }
    }

    // Eye / lock / solo controls, right-aligned in the label column
    if let Some(track) = timeline.and_then(|tl| tl.get_track(actor_label)) {
        let icon_size = Vec2::new(ICON_W, sp.timeline.track_row_height - 4.0);
        let lock_rect = Rect::from_min_size(
            Pos2::new(bar_origin_x - sp.base.space_2 - ICON_W, at_top + 2.0),
            icon_size,
        );
        let eye_rect = Rect::from_min_size(
            Pos2::new(lock_rect.left() - ICON_GAP - ICON_W, at_top + 2.0),
            icon_size,
        );
        let solo_rect = Rect::from_min_size(
            Pos2::new(eye_rect.left() - ICON_GAP - ICON_W, at_top + 2.0),
            icon_size,
        );
        debug_assert!(
            (solo_rect.left() - icon_strip_left).abs() < 0.5,
            "label clip must match the icon strip"
        );

        let visible = timeline.is_some_and(|t| t.is_actor_visible(&track.label));
        let eye_icon = if visible {
            egui_phosphor::regular::EYE
        } else {
            egui_phosphor::regular::EYE_CLOSED
        };
        let eye_resp =
            ui.interact(eye_rect, ui.id().with(("actor_eye", actor_label)), Sense::click());
        painter.text(
            eye_rect.center(),
            Align2::CENTER_CENTER,
            eye_icon,
            TextRole::BodyS.font_id(),
            if !visible {
                theme.palette.text.disabled
            } else if eye_resp.hovered() {
                theme.palette.text.primary
            } else {
                theme.palette.text.secondary
            },
        );
        text_tooltip(
            ui,
            eye_resp.id.with("tooltip"),
            &eye_resp,
            if visible { "Hide layer" } else { "Show layer" },
        );
        if eye_resp.clicked() {
            commands.push_back(ActorCommand::ToggleActorVisibility(actor_label.to_string()).into());
        }

        // Solo is authored state (`solo: true` in the declaration)
        let solo = matches!(
            read_property_value(track, ActorField::Tagged("solo"), 0),
            Some(PropertyValue::Bool(true))
        );
        let solo_resp =
            ui.interact(solo_rect, ui.id().with(("actor_solo", actor_label)), Sense::click());
        painter.text(
            solo_rect.center(),
            Align2::CENTER_CENTER,
            egui_phosphor::regular::HEADPHONES,
            TextRole::BodyS.font_id(),
            if solo {
                theme.palette.status.warning
            } else if solo_resp.hovered() {
                theme.palette.text.primary
            } else {
                theme.palette.text.disabled
            },
        );
        text_tooltip(
            ui,
            solo_resp.id.with("tooltip"),
            &solo_resp,
            if solo {
                "Unsolo layer"
            } else {
                "Solo layer (hides every other layer)"
            },
        );
        if solo_resp.clicked() {
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor_label.to_string(),
                    property: "solo".to_string(),
                    value: PropertyValue::Bool(!solo),
                    create_keyframe: false,
                })
                .into(),
            );
        }

        let locked = timeline.is_some_and(|t| t.is_actor_locked(&track.label));
        let lock_icon = if locked {
            egui_phosphor::regular::LOCK_KEY
        } else {
            egui_phosphor::regular::LOCK_KEY_OPEN
        };
        let lock_resp =
            ui.interact(lock_rect, ui.id().with(("actor_lock", actor_label)), Sense::click());
        painter.text(
            lock_rect.center(),
            Align2::CENTER_CENTER,
            lock_icon,
            TextRole::BodyS.font_id(),
            if locked {
                theme.palette.status.warning
            } else if lock_resp.hovered() {
                theme.palette.text.primary
            } else {
                theme.palette.text.disabled
            },
        );
        text_tooltip(
            ui,
            lock_resp.id.with("tooltip"),
            &lock_resp,
            if locked { "Unlock layer" } else { "Lock layer" },
        );
        if lock_resp.clicked() {
            commands.push_back(ActorCommand::ToggleActorLock(actor_label.to_string()).into());
        }
    }
}

/// Render the left header column of an expanded property lane row (colored dot, indented property name).
pub(crate) fn render_property_row_header(
    painter: &egui::Painter,
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
) {
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

    // Property label (indented deeper than actor label)
    let prop_indent = depth as f32 * 14.0 + 18.0;

    // Small colored dot indicator
    let dot_x = scroll_rect.left() + sp.base.space_2 + prop_indent;
    painter.circle_filled(Pos2::new(dot_x + 3.0, prop_rect.center().y), 3.0, group_col);

    let prop_label_x = dot_x + 10.0;
    painter.text(
        Pos2::new(prop_label_x, prop_rect.center().y),
        Align2::LEFT_CENTER,
        prop_name,
        TextRole::Micro.font_id(),
        theme.palette.text.muted,
    );
}
