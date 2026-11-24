//! Authored timing, not a reverse mapping of generated ECS properties.
use super::TimelineCoords;
use crate::app::commands::{ActionQueue, Command};
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::to_source::ToSource;
use egui::{Pos2, Rect, Sense, Stroke};

#[derive(Debug, Clone)]
pub(crate) struct TimingMarker {
    pub expression: String,
    pub line: Option<usize>,
    pub scene: Option<String>,
    pub time_s: Option<f64>,
    pub action_count: usize,
}

pub(crate) fn collect_markers(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
) -> Vec<TimingMarker> {
    let mut result = Vec::new();
    collect_scope(statements, timeline, composition, None, Some(0.0), &mut result);
    result
}

pub(crate) fn review_markers(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> Vec<TimingMarker> {
    use animatix_syntax::diagnostics::DiagnosticCode;
    let mut markers = collect_markers(statements, timeline, composition);
    for problem in diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::NarrationBindingUnresolved)
    {
        if let Some(line) = problem.location.line {
            for marker in markers.iter_mut().filter(|m| m.line == line.checked_sub(1)) {
                marker.time_s = None;
            }
        }
    }
    markers
}

fn collect_scope(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    scene: Option<&str>,
    global_offset: Option<f64>,
    out: &mut Vec<TimingMarker>,
) {
    let mut previous = Some(0.0);
    for statement in statements {
        match statement {
            Stmt::Scene { name, body, .. } => {
                let timeline = composition.and_then(|c| c.scenes.get(name)).map(|s| &s.timeline);
                let offset = composition.and_then(|c| c.scene_start_times.get(name)).copied();
                collect_scope(body, timeline, composition, Some(name), offset, out);
            },
            statement if statement.is_cue_only_keyframe() => {},
            Stmt::Keyframe {
                time, body, span, ..
            }
            | Stmt::RelativeKeyframe {
                offset: time,
                body,
                span,
                ..
            } => {
                if let Time::SpeechMarker { marker, .. } = time
                    && animatix_syntax::narration::is_implicit_anchor(marker)
                    && body.is_empty()
                {
                    continue;
                }
                let relative = matches!(statement, Stmt::RelativeKeyframe { .. });
                let compiled =
                    span.and_then(|span| timeline?.marker_time(span)).map(|ms| ms / 1000.0);
                let value = resolve_time(time);
                let local = compiled.or_else(|| {
                    if relative {
                        previous.zip(value).map(|(a, b)| a + b)
                    } else {
                        value
                    }
                });
                previous = local;
                out.push(TimingMarker {
                    expression: format!("#{}{}", if relative { "+" } else { "" }, time.to_source()),
                    line: span.map(|span| span.start_line.saturating_sub(1)),
                    scene: scene.map(str::to_owned),
                    time_s: global_offset.zip(local).map(|(a, b)| a + b),
                    action_count: body.len(),
                });
            },
            // These structural forms can change the engine's current build time.
            // Do not invent the anchor of a subsequent relative marker.
            Stmt::Conditional { .. } | Stmt::Match { .. } | Stmt::ForLoop { .. } => previous = None,
            _ => {}, // Generated component/loop timing is not uniquely source-addressable.
        }
    }
}

fn resolve_time(time: &Time) -> Option<f64> {
    let value = match time {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
        // Tempo may be expression-driven. Do not silently assume the default BPM.
        Time::Beats(_) | Time::Expression(_) => return None,
        // Expressions execute in their lexical build environment. Re-evaluating
        // them against the final environment could place a marker incorrectly.
        // The compiler owns clip offsets, lexical scope and paragraph alignment.
        // With no compiled span we can still offer source navigation.
        Time::SpeechMarker { .. } | Time::SpeechWord { .. } => return None,
    };
    value.is_finite().then_some(value)
}

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

fn marker_label(marker: &TimingMarker) -> String {
    let scene = marker.scene.as_deref().unwrap_or("Scene");
    let time = marker
        .time_s
        .map(|s| format!("{s:.3}s global"))
        .unwrap_or_else(|| "time unresolved".into());
    let line = marker
        .line
        .map(|l| format!("line {}", l + 1))
        .unwrap_or_else(|| "source unavailable".into());
    format!(
        "{} · {scene} · {time} · {line} · {} statements",
        marker.expression, marker.action_count
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
        if group.len() == 1 {
            response.on_hover_text(marker_label(marker));
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
                    if ui.button(marker_label(&markers[*index])).clicked() {
                        navigate(&markers[*index], commands);
                        ui.close();
                    }
                }
            });
            response.on_hover_text(format!(
                "{} authored markers · Click first or right-click to choose",
                group.len()
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
        Button::ghost("Markers").with_icon(DIAMOND)
    } else {
        Button::icon(DIAMOND)
    };
    let mut tooltip = format!("Source markers · {} authored anchors", markers.len());
    if !source_current {
        tooltip.push_str("\nSource links resume after a successful rebuild.");
    }
    let response = ui.add(button.with_size(size).with_tooltip(tooltip));
    egui::Popup::menu(&response).show(|ui| {
        ui.label("Authored # markers in source order");
        if !source_current {
            ui.weak("Source links paused until a successful rebuild");
        }
        for marker in markers {
            if ui.button(marker_label(marker)).clicked() {
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
        let mut frame = |events| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(370.0, 200.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    super::super::ruler::bar_interaction(ui, bar, "ruler", &mut commands, |x| {
                        coords.x_to_time(x)
                    });
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
