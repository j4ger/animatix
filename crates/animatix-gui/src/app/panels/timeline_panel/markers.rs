//! Authored timing, not a reverse mapping of generated ECS properties.
use super::TimelineCoords;
use crate::app::commands::{ActionQueue, Command};
use crate::app::i18n::render;
#[cfg(test)]
use animatix_syntax::ast::{Stmt, Time};
use egui::{Pos2, Rect, Sense, Stroke};

#[cfg(test)]
pub(crate) use crate::app::document::source_timing::collect_markers;
pub(crate) use crate::app::document::source_timing::{TimingMarker, review_markers};

fn clusters(markers: &[TimingMarker], coords: &TimelineCoords) -> Vec<Vec<usize>> {
    let mut visible: Vec<_> = markers
        .iter()
        .enumerate()
        .filter_map(|(i, marker)| {
            let time = marker.time_s?;
            (time >= coords.scroll_s && time <= coords.scroll_s + coords.visible_s).then_some(i)
        })
        .collect();
    visible.sort_by(|a, b| markers[*a].time_s.partial_cmp(&markers[*b].time_s).unwrap());
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for index in visible {
        let x = coords.time_to_x(markers[index].time_s.unwrap());
        if let Some(last) = groups.last_mut()
            && x - coords.time_to_x(markers[last[0]].time_s.unwrap()) < 20.0
        {
            last.push(index);
        } else {
            groups.push(vec![index]);
        }
    }
    groups
}

fn marker_label(ui: &egui::Ui, marker: &TimingMarker) -> String {
    use crate::app::i18n::{render, text};
    let scene = marker.scene.clone().unwrap_or_else(|| text(ui, "gui-inspector-scene"));
    let time = marker
        .time_s
        .map(|s| render(ui, "gui-timeline-marker-time", &[("time", format!("{s:.3}"))]))
        .unwrap_or_else(|| text(ui, "gui-timeline-marker-unresolved"));
    let line = marker
        .line
        .map(|l| render(ui, "gui-timeline-marker-line", &[("line", (l + 1).to_string())]))
        .unwrap_or_else(|| text(ui, "gui-timeline-marker-no-source"));
    render(
        ui,
        "gui-timeline-marker-label",
        &[
            ("expression", marker.expression.clone()),
            ("scene", scene),
            ("time", time),
            ("location", line),
            ("count", marker.action_count.to_string()),
        ],
    )
}

pub(crate) fn navigate(marker: &TimingMarker, commands: &mut ActionQueue) {
    if let Some(time) = marker.time_s {
        commands.push_back(Command::ScrubTo(time).into());
    }
    if let Some(line) = marker.line {
        commands.push_back(Command::ScrollToLine(line, 0).into());
    }
}

/// Authoring markers share the ruler with the labels. Their short click targets
/// occupy its lower edge; the label area and spaces between markers still scrub.
pub(super) fn render_ruler_markers(
    ui: &mut egui::Ui,
    bar: Rect,
    coords: &TimelineCoords,
    markers: &[TimingMarker],
    commands: &mut ActionQueue,
) {
    let theme = eparts::theme(ui);
    let band = super::ruler::marker_band(bar);
    let painter = ui.painter().with_clip_rect(band.intersect(ui.clip_rect()));
    for group in clusters(markers, coords) {
        let marker = &markers[group[0]];
        let center = Pos2::new(coords.time_to_x(marker.time_s.unwrap()), band.center().y);
        let hit = Rect::from_center_size(center, egui::vec2(18.0, band.height())).intersect(band);
        if !ui.is_rect_visible(hit) {
            continue;
        }
        let response =
            ui.interact(hit, ui.scope_id().with(("source_marker", group[0])), Sense::click());
        let color = if response.hovered() || response.has_focus() {
            theme.palette.text.primary
        } else {
            theme.palette.accent.primary
        };
        let paint_diamond = |at: Pos2, r: f32| {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    at + egui::vec2(0.0, -r),
                    at + egui::vec2(r, 0.0),
                    at + egui::vec2(0.0, r),
                    at + egui::vec2(-r, 0.0),
                ],
                color,
                Stroke::NONE,
            ));
        };
        paint_diamond(center, 3.5);
        if response.clicked() {
            navigate(marker, commands);
        }
        let label = marker_label(ui, marker);
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label));
        if group.len() == 1 {
            response.on_hover_text(label);
        } else {
            // A smaller neighboring lozenge denotes a cluster without a numeric
            // label colliding with time text or a nearby marker.
            let direction = if band.right() - center.x < 8.0 {
                -1.0
            } else {
                1.0
            };
            paint_diamond(center + egui::vec2(direction * 6.0, 0.0), 1.5);
            response.context_menu(|ui| {
                for index in &group {
                    if ui.button(marker_label(ui, &markers[*index])).clicked() {
                        navigate(&markers[*index], commands);
                        ui.close();
                    }
                }
            });
            response.on_hover_text(render(
                ui,
                "gui-timeline-marker-cluster",
                &[("count", group.len().to_string())],
            ));
        }
    }
}

pub(super) fn marker_menu(
    ui: &mut egui::Ui,
    markers: &[TimingMarker],
    commands: &mut ActionQueue,
    size: eparts::Size,
    source_current: bool,
) {
    use crate::app::components::button::Button;
    use egui_phosphor::regular::DIAMOND;

    let button = if ui.available_width() >= 360.0 {
        Button::ghost(crate::app::i18n::text(ui, "gui-timeline-markers")).with_icon(DIAMOND)
    } else {
        Button::icon(DIAMOND)
    };
    let mut tooltip =
        render(ui, "gui-timeline-marker-count", &[("count", markers.len().to_string())]);
    if !source_current {
        tooltip.push('\n');
        tooltip.push_str(&crate::app::i18n::text(ui, "gui-source-navigation-resume"));
    }
    let response = ui.add(button.with_size(size).with_tooltip(tooltip));
    egui::Popup::menu(&response).show(|ui| {
        ui.label(crate::app::i18n::text(ui, "gui-timeline-markers-order"));
        if !source_current {
            ui.weak(crate::app::i18n::text(ui, "gui-source-navigation-paused"));
        }
        for marker in markers {
            if ui.button(marker_label(ui, marker)).clicked() {
                navigate(marker, commands);
                ui.close();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn marker(time: Option<f64>, line: usize) -> TimingMarker {
        TimingMarker {
            expression: "#1s".into(),
            line: Some(line),
            scene: None,
            time_s: time,
            action_count: 1,
        }
    }
    #[test]
    fn negative_relative_offset_is_not_clamped_before_addition() {
        let ast = vec![
            Stmt::Keyframe {
                cue_group: None,
                time: Time::Seconds(4.0),
                body: vec![],
                span: None,
            },
            Stmt::RelativeKeyframe {
                cue_group: None,
                offset: Time::Seconds(-1.0),
                body: vec![],
                span: None,
            },
        ];
        assert_eq!(collect_markers(&ast, None, None)[1].time_s, Some(3.0));
    }
    #[test]
    fn same_time_markers_are_not_deduplicated() {
        let coords = TimelineCoords {
            bar_origin_x: 100.0,
            bar_width: 500.0,
            scroll_s: 1.0,
            visible_s: 5.0,
        };
        let marks = vec![
            marker(Some(0.0), 0),
            marker(Some(2.0), 1),
            marker(Some(2.0), 2),
            marker(None, 3),
        ];
        assert_eq!(clusters(&marks, &coords), vec![vec![1, 2]]);
    }
    #[test]
    fn shared_ruler_separates_cluster_navigation_from_scrubbing() {
        use crate::app::commands::ShellAction;
        use egui::{Event, Modifiers, PointerButton, RawInput, pos2};

        let ctx = egui::Context::default();
        let bar = Rect::from_min_max(pos2(100.0, 32.0), pos2(350.0, 54.0));
        let coords = TimelineCoords {
            bar_origin_x: bar.left(),
            bar_width: bar.width(),
            scroll_s: 8.0,
            visible_s: 2.0,
        };
        let markers = [
            marker(Some(7.9), 1),   // panned out of view
            marker(Some(8.50), 10), // first member of a visible cluster
            marker(Some(8.55), 11),
            marker(Some(9.0), 20),  // independent below a labeled tick
            marker(Some(10.1), 23), // panned out of view
        ];
        assert_eq!(clusters(&markers, &coords), vec![vec![1, 2], vec![3]]);
        let mut commands = ActionQueue::default();
        let mut preview = crate::app::PreviewPaneState::new(20.0, crate::app::DEFAULT_PREVIEW_SIZE);
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(370.0, 200.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    super::super::ruler::scrub_ruler(
                        ui,
                        bar,
                        &coords,
                        &mut preview,
                        &mut commands,
                        Vec::new,
                    );
                    render_ruler_markers(ui, bar, &coords, &markers, &mut commands);
                },
            );
            output.textures_delta.clear();
        };
        frame(vec![]);
        let mut click = |pos| {
            let event = |pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(vec![Event::PointerMoved(pos), event(true)]);
            frame(vec![event(false)]);
        };
        let lower = super::super::ruler::marker_band(bar).center().y;
        click(pos2(coords.time_to_x(8.50), lower));
        // The exact same x a few pixels above the annotation, plus the gap
        // between annotations, remain regular ruler scrub targets.
        click(pos2(coords.time_to_x(8.50), 37.0));
        click(pos2(coords.time_to_x(8.75), lower));
        click(pos2(coords.time_to_x(9.0), lower));

        for (t, line) in [(8.5, Some(10)), (8.5, None), (8.75, None), (9.0, Some(20))] {
            assert!(matches!(
                commands.pop_front(),
                Some(ShellAction::Command(Command::ScrubTo(actual))) if (actual - t).abs() < 1e-6
            ));
            if let Some(line) = line {
                assert!(matches!(
                    commands.pop_front(),
                    Some(ShellAction::Command(Command::ScrollToLine(actual, 0))) if actual == line
                ));
            }
        }
        assert!(commands.pop_front().is_none());
    }

    #[test]
    fn unresolved_marker_only_navigates_source() {
        let mut queue = ActionQueue::default();
        navigate(&marker(None, 3), &mut queue);
        assert_eq!(queue.len(), 1);
        assert!(matches!(
            queue.pop_front(),
            Some(crate::app::commands::ShellAction::Command(Command::ScrollToLine(3, 0)))
        ));
    }
    #[test]
    fn relative_chain_does_not_guess_after_unknown_beat() {
        let ast = vec![
            Stmt::Keyframe {
                cue_group: None,
                time: Time::Beats(2.0),
                body: vec![],
                span: None,
            },
            Stmt::RelativeKeyframe {
                cue_group: None,
                offset: Time::Seconds(1.0),
                body: vec![],
                span: None,
            },
            Stmt::Keyframe {
                cue_group: None,
                time: Time::Seconds(4.0),
                body: vec![],
                span: None,
            },
        ];
        let result = collect_markers(&ast, None, None);
        assert_eq!(
            result.iter().map(|m| m.time_s).collect::<Vec<_>>(),
            vec![None, None, Some(4.0)]
        );
    }
}
