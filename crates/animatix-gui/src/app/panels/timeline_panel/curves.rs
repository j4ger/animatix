//! Optional read-only curve beside the timeline's existing tracks.
use animatix::timeline::Timeline;
use egui::{Align2, Id, Pos2, Rect, Sense, Stroke, Vec2, pos2};

use super::TimelineCoords;
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use crate::app::design_tokens::{spatial::STROKE_WIDTH, typography::TextRole};
use crate::app::panels::curve_plot::{self, Axis, CurveChannel};

pub(super) fn lane_height(available: f32) -> f32 {
    // Keep the narration and curve on screen together in the default split.
    // When smaller still, the existing vertical scroll remains the escape hatch.
    available.clamp(56.0, 100.0)
}

#[derive(Clone, PartialEq, Eq)]
struct Selection {
    scene: Option<String>,
    actor: String,
    property: String,
    channel: String,
}

pub(super) struct Control<'a> {
    id: Id,
    scene: Option<&'a str>,
    selection: Option<Selection>,
}

impl<'a> Control<'a> {
    pub(super) fn new(ui: &egui::Ui, panel: Id, scene: Option<&'a str>) -> Self {
        let id = panel.with("inline_row_curve");
        let selection = ui.data(|d| d.get_temp::<Option<Selection>>(id).flatten());
        Self {
            id,
            scene,
            selection: selection.filter(|s| s.scene.as_deref() == scene),
        }
    }

    pub(super) fn owner(&self) -> Option<(&str, &str)> {
        let selection = self.selection.as_ref()?;
        Some((&selection.actor, &selection.property))
    }

    pub(super) fn active(&self, actor: &str, property: &str) -> Option<&str> {
        let selection = self.selection.as_ref()?;
        (selection.actor == actor && selection.property == property)
            .then_some(selection.channel.as_str())
    }

    pub(super) fn toggle(&self, ui: &egui::Ui, actor: &str, property: &str, channel: &str) {
        if self.active(actor, property).is_some() {
            ui.data_mut(|d| {
                d.insert_temp::<Option<Selection>>(self.id, None);
                d.insert_temp(self.id.with("reveal"), false);
            });
            ui.ctx().request_repaint();
        } else {
            self.store(ui, actor, property, channel, true);
        }
    }

    fn store(&self, ui: &egui::Ui, actor: &str, property: &str, channel: &str, reveal: bool) {
        let selection = Selection {
            scene: self.scene.map(str::to_owned),
            actor: actor.to_owned(),
            property: property.to_owned(),
            channel: channel.to_owned(),
        };
        ui.data_mut(|d| {
            d.insert_temp(self.id, Some(selection));
            d.insert_temp(self.id.with("reveal"), reveal);
        });
        ui.ctx().request_repaint();
    }

    pub(super) fn reveal(&self, ui: &mut egui::Ui, rect: Rect) {
        let pending = ui.data_mut(|d| d.remove_temp::<bool>(self.id.with("reveal")));
        if pending == Some(true) {
            let clip = ui.clip_rect();
            if rect.top() < clip.top() || rect.bottom() > clip.bottom() {
                ui.scroll_to_rect(rect, None);
            }
        }
    }

    pub(super) fn choose_channel(
        &self,
        ui: &mut egui::Ui,
        gutter: Rect,
        property: &str,
        current: &CurveChannel,
        channels: &[&CurveChannel],
    ) {
        if channels.len() <= 1 || gutter.width() < 28.0 {
            return;
        }
        let width = (gutter.width() - 7.0).min(57.0);
        let rect = Rect::from_min_size(gutter.min + Vec2::new(4.0, 5.0), Vec2::new(width, 18.0));
        ui.scope_builder(
            egui::UiBuilder::new().id_salt(self.id.with("channel_menu")).max_rect(rect),
            |ui| {
                let short_name = |label: &str| label.rsplit('.').next().unwrap_or(label).to_owned();
                egui::ComboBox::from_id_salt("curve_channel")
                    .width(width - 10.0)
                    .selected_text(short_name(&current.base_label))
                    .show_ui(ui, |ui| {
                        for channel in channels {
                            if ui
                                .selectable_label(
                                    channel.base_label == current.base_label,
                                    short_name(&channel.base_label),
                                )
                                .clicked()
                            {
                                self.store(
                                    ui,
                                    &current.actor,
                                    property,
                                    &channel.base_label,
                                    false,
                                );
                            }
                        }
                    });
            },
        );
    }
}

pub(super) fn property_channels<'a>(
    channels: &'a [CurveChannel],
    property: &str,
) -> Vec<&'a CurveChannel> {
    channels
        .iter()
        .filter(|c| {
            c.base_label == property
                || c.base_label.strip_prefix(property).is_some_and(|s| s.starts_with('.'))
        })
        .collect()
}

pub(super) fn collect(timeline: &Timeline, actor: &str, theme: eparts::Theme) -> Vec<CurveChannel> {
    timeline
        .get_entity_id(actor)
        .map(|id| curve_plot::collect_curves(timeline, actor, id, theme))
        .unwrap_or_default()
}

pub(super) fn lane(
    ui: &mut egui::Ui,
    rect: Rect,
    coords: &TimelineCoords,
    channel: &CurveChannel,
    scene_offset: f64,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    has_channel_picker: bool,
) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let theme = eparts::theme(ui);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.palette.surface.base);
    let gutter = Rect::from_min_max(rect.min, Pos2::new(coords.bar_origin_x, rect.bottom()));
    let has_value_labels = gutter.width() >= 105.0;
    let mut caption_rect = gutter;
    if has_value_labels {
        caption_rect.max.x -= 35.0;
    }
    let caption = painter.with_clip_rect(caption_rect.intersect(ui.clip_rect()));
    if !has_channel_picker {
        caption.text(
            caption_rect.min + Vec2::new(7.0, 7.0),
            Align2::LEFT_TOP,
            &channel.base_label,
            TextRole::BodyS.font_id(),
            channel.color,
        );
    }
    let lane = Rect::from_min_max(pos2(coords.bar_origin_x, rect.top()), rect.right_bottom());
    if lane.width() <= 0.0 {
        return;
    }
    let plot = lane.shrink2(Vec2::new(0.0, 10.0));
    let axis = curve_axis(plot, coords, channel, scene_offset);
    let paint = painter.with_clip_rect(lane.intersect(ui.clip_rect()));
    curve_plot::draw_grid_lines(&paint, &axis);
    if has_value_labels {
        let labels_rect =
            Rect::from_min_max(pos2(caption_rect.right(), gutter.top()), gutter.right_bottom());
        let labels = painter.with_clip_rect(labels_rect.intersect(ui.clip_rect()));
        for (y, value) in curve_plot::value_ticks(&axis) {
            labels.text(
                pos2(gutter.right() - 3.0, y),
                Align2::RIGHT_CENTER,
                format!("{value:.1}"),
                TextRole::Micro.font_id(),
                theme.palette.text.muted,
            );
        }
    }
    curve_plot::draw_curve(
        &paint,
        &axis,
        channel,
        animatix::time::scene_local_seconds(preview.playback.current_time_s(), scene_offset),
        theme,
    );
    painter.line_segment(
        [lane.left_top(), lane.left_bottom()],
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
    );
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
    );
    let response =
        ui.interact(lane, ui.scope_id().with("inline_curve_seek"), Sense::click_and_drag());
    if (response.clicked() || response.dragged() || response.drag_stopped())
        && let Some(pos) = response
            .interact_pointer_pos()
            .or_else(|| ui.input(|input| input.pointer.latest_pos()))
    {
        let time = coords.x_to_time(pos.x).clamp(0.0, preview.playback.duration_s.max(0.0));
        commands.push_back(Command::ScrubTo(time).into());
    }
    response.on_hover_text(crate::app::i18n::text(ui, "gui-timeline-curves-seek"));
}

fn curve_axis(
    plot: Rect,
    coords: &TimelineCoords,
    channel: &CurveChannel,
    scene_offset: f64,
) -> Axis {
    let (min, max) = curve_plot::value_window(&[channel]);
    Axis::new(plot, coords.visible_s, min, max)
        .with_time_window(coords.scroll_s - scene_offset, coords.visible_s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_uses_remaining_height_without_collapsing_to_an_unusable_sliver() {
        assert_eq!(lane_height(64.0), 64.0);
        assert_eq!(lane_height(200.0), 100.0);
        assert_eq!(lane_height(20.0), 56.0);
    }

    #[test]
    fn row_selection_is_scoped_to_scene_actor_and_property() {
        let ctx = egui::Context::default();
        let panel = Id::unique("timeline");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let control = Control::new(ui, panel, Some("one"));
            assert_eq!(control.owner(), None);
            control.toggle(ui, "pulse", "position", "position.X");

            let next = Control::new(ui, panel, Some("one"));
            assert_eq!(next.owner(), Some(("pulse", "position")));
            assert_eq!(next.active("pulse", "position"), Some("position.X"));
            assert_eq!(next.active("other", "position"), None);
            assert_eq!(next.active("pulse", "opacity"), None);
            next.store(ui, "pulse", "position", "position.Y", false);
            assert_eq!(
                Control::new(ui, panel, Some("one")).active("pulse", "position"),
                Some("position.Y")
            );
            assert_eq!(Control::new(ui, panel, Some("two")).owner(), None);
            next.toggle(ui, "pulse", "position", "position.X");
            assert_eq!(Control::new(ui, panel, Some("one")).owner(), None);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn very_short_zoomed_window_matches_timeline() {
        let plot = Rect::from_min_size(pos2(20.0, 0.0), Vec2::new(400.0, 80.0));
        let coords = TimelineCoords {
            bar_origin_x: 20.0,
            bar_width: 400.0,
            scroll_s: 0.025,
            visible_s: 0.0125,
        };
        let axis = Axis::new(plot, coords.visible_s, 0.0, 1.0)
            .with_time_window(coords.scroll_s, coords.visible_s);
        assert!((axis.x(0.03125) - coords.time_to_x(0.03125)).abs() < 0.001);
    }

    #[test]
    fn graph_click_dispatches_global_seek_at_matching_keyframe() {
        use crate::app::commands::ShellAction;
        use egui::{Event, Modifiers, PointerButton, RawInput};

        let timeline = curve_plot::two_actor_timeline();
        let channel = curve_plot::collect_curves(
            &timeline,
            "a",
            timeline.get_entity_id("a").unwrap(),
            eparts::Theme::default(),
        )
        .into_iter()
        .find(|c| c.base_label == "position.X")
        .unwrap();
        let coords = TimelineCoords {
            bar_origin_x: 120.0,
            bar_width: 400.0,
            scroll_s: 8.25,
            visible_s: 0.5,
        };
        let rect = Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(520.0, 80.0));
        let preview = PreviewPaneState::new(
            10.0,
            animatix::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
        );
        let mut commands = ActionQueue::new();
        let ctx = egui::Context::default();
        let pos = pos2(coords.time_to_x(8.5), 40.0);
        let button = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let mut selected_keyframes = Vec::new();
        for events in [
            vec![],
            vec![Event::PointerMoved(pos), button(true)],
            vec![button(false)],
        ] {
            let mut output = ctx.run_ui(
                RawInput {
                    events,
                    screen_rect: Some(rect.expand(100.0)),
                    ..RawInput::default()
                },
                |ui| {
                    lane(ui, rect, &coords, &channel, 8.0, &preview, &mut commands, false);
                    use super::super::keyframes::{KeyframeRenderCtx, render_property_keyframes};
                    let mut kf = KeyframeRenderCtx {
                        coords: &coords,
                        preview: &preview,
                        active_scene: Some("second"),
                        scene_offset: 8.0,
                        scene_for_selection: Some("second".to_owned()),
                        selected_keyframes: &mut selected_keyframes,
                        shift_held: false,
                        theme: eparts::theme(ui),
                    };
                    let property_row = Rect::from_min_max(pos2(120.0, 85.0), pos2(520.0, 105.0));
                    render_property_keyframes(
                        ui,
                        &[500],
                        "a",
                        "position",
                        0,
                        property_row,
                        None,
                        &mut kf,
                    );
                },
            );
            assert!(
                output.shapes.iter().any(|shape| match &shape.shape {
                    egui::Shape::Path(path) => path
                        .points
                        .iter()
                        .any(|p| (p.x - pos.x).abs() < 0.01 && (p.y - 95.0).abs() < 7.0),
                    _ => false,
                }),
                "second-scene property diamond must align with the shared global x"
            );
            output.textures_delta.clear();
        }
        assert!(
            matches!(commands.pop_front(), Some(ShellAction::Command(Command::ScrubTo(t))) if (t - 8.5).abs() < 0.0001)
        );
        assert!(commands.is_empty());

        let last_motion = pos2(coords.time_to_x(8.60), pos.y);
        let released = pos2(coords.time_to_x(8.70), pos.y);
        for events in [
            vec![Event::PointerMoved(pos), button(true)],
            vec![Event::PointerMoved(last_motion)],
            vec![Event::PointerButton {
                pos: released,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        ] {
            let mut output = ctx.run_ui(
                RawInput {
                    events,
                    screen_rect: Some(rect.expand(100.0)),
                    ..RawInput::default()
                },
                |ui| lane(ui, rect, &coords, &channel, 8.0, &preview, &mut commands, false),
            );
            output.textures_delta.clear();
        }
        assert!(
            matches!(commands.pop_back(), Some(ShellAction::Command(Command::ScrubTo(t))) if (t - 8.70).abs() < 0.0001)
        );
    }

    #[test]
    fn second_scene_curve_and_global_tracks_share_the_same_x_and_seek_after_panning() {
        let timeline = curve_plot::two_actor_timeline();
        let id = timeline.get_entity_id("a").unwrap();
        let curve = curve_plot::collect_curves(&timeline, "a", id, eparts::Theme::default())
            .into_iter()
            .find(|c| c.base_label == "position.X")
            .unwrap();
        let coords = TimelineCoords {
            bar_origin_x: 120.0,
            bar_width: 400.0,
            scroll_s: 8.25,
            visible_s: 0.5,
        };
        let axis = curve_axis(
            Rect::from_min_size(pos2(120.0, 0.0), Vec2::new(400.0, 80.0)),
            &coords,
            &curve,
            8.0,
        );
        assert!(
            axis.x(0.0) < 120.0 && axis.x(1.0) > 520.0,
            "offscreen samples are not pinned to viewport edges"
        );
        let key_x = axis.x(0.5);
        assert_eq!(key_x, coords.time_to_x(8.5));
        assert_eq!(coords.x_to_time(key_x), 8.5);
    }
}
