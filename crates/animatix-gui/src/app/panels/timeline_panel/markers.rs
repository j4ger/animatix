//! Authored timing, not a reverse mapping of generated ECS properties.
use super::TimelineCoords;
use crate::app::commands::{ActionQueue, Command};
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::to_source::ToSource;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke};

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
            Stmt::Keyframe { time, body, span }
            | Stmt::RelativeKeyframe {
                offset: time,
                body,
                span,
            } => {
                let relative = matches!(statement, Stmt::RelativeKeyframe { .. });
                let value = resolve_time(time, timeline);
                let local = if relative {
                    previous.zip(value).map(|(a, b)| a + b)
                } else {
                    value
                };
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

fn resolve_time(time: &Time, timeline: Option<&Timeline>) -> Option<f64> {
    let value = match time {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
        // Tempo may be expression-driven. Do not silently assume the default BPM.
        Time::Beats(_) => return None,
        Time::SpeechMarker {
            track,
            marker,
            offset_ms,
        } => timeline?.transcript(track)?.find_marker(marker)? + *offset_ms as f64 / 1000.0,
        Time::SpeechWord {
            track,
            word,
            occurrence,
            offset_ms,
        } => {
            timeline?.transcript(track)?.find_word(word, *occurrence)?.0
                + *offset_ms as f64 / 1000.0
        },
    };
    // Speech anchors are lowered to nonnegative integer milliseconds by the engine.
    // Numeric relative offsets remain signed until composed with the prior marker.
    let value = if matches!(time, Time::SpeechMarker { .. } | Time::SpeechWord { .. }) {
        (value * 1000.0).round().max(0.0) / 1000.0
    } else {
        value
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

pub(crate) fn render_markers(
    ui: &mut egui::Ui,
    rect: Rect,
    coords: &TimelineCoords,
    markers: &[TimingMarker],
    commands: &mut ActionQueue,
) {
    let theme = eparts::theme(ui);
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, theme.palette.surface.base);
    painter.text(
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        "# Source markers",
        FontId::proportional(11.0),
        theme.palette.text.secondary,
    );
    let bar = Rect::from_min_max(Pos2::new(coords.bar_origin_x, rect.top()), rect.max);
    let painter = painter.with_clip_rect(bar.intersect(ui.clip_rect()));
    for group in clusters(markers, coords) {
        let marker = &markers[group[0]];
        let center = Pos2::new(coords.time_to_x(marker.time_s.unwrap()), rect.center().y);
        let r = 5.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                center + egui::vec2(0.0, -r),
                center + egui::vec2(r, 0.0),
                center + egui::vec2(0.0, r),
                center + egui::vec2(-r, 0.0),
            ],
            theme.palette.accent.primary,
            Stroke::NONE,
        ));
        let response = ui.interact(
            Rect::from_center_size(center, egui::vec2(18.0, rect.height())),
            ui.scope_id().with(("source_marker", group[0])),
            Sense::click(),
        );
        if response.clicked() {
            navigate(marker, commands);
        }
        if group.len() == 1 {
            response.on_hover_text(marker_label(marker));
        } else {
            painter.text(
                center + egui::vec2(8.0, -9.0),
                Align2::LEFT_CENTER,
                format!("×{}", group.len()),
                FontId::monospace(10.0),
                theme.palette.text.primary,
            );
            response.context_menu(|ui| {
                for index in &group {
                    if ui.button(marker_label(&markers[*index])).clicked() {
                        navigate(&markers[*index], commands);
                        ui.close();
                    }
                }
            });
            response.on_hover_text(
                "Several authored markers. Click first; right-click to choose a source location.",
            );
        }
    }
}

pub(crate) fn marker_menu(ui: &mut egui::Ui, markers: &[TimingMarker], commands: &mut ActionQueue) {
    let _ = crate::app::components::icon_menu(
        ui,
        egui_phosphor::regular::DIAMOND,
        &format!("Source markers · {} authored anchors", markers.len()),
        |ui| {
            ui.label("Direct scene/top-level # markers · source order");
            ui.weak("Generated loop/component timing is not expanded here.");
            for marker in markers {
                if ui.button(marker_label(marker)).clicked() {
                    navigate(marker, commands);
                    ui.close();
                }
            }
            if markers.iter().any(|m| m.time_s.is_none()) {
                ui.label(
                    "Unresolved timing is never guessed (beat / missing speech or scene anchor).",
                );
            }
        },
    );
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
                time: Time::Seconds(4.0),
                body: vec![],
                span: None,
            },
            Stmt::RelativeKeyframe {
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
                time: Time::Beats(2.0),
                body: vec![],
                span: None,
            },
            Stmt::RelativeKeyframe {
                offset: Time::Seconds(1.0),
                body: vec![],
                span: None,
            },
            Stmt::Keyframe {
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
