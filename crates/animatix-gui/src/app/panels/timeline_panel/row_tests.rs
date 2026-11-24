//! Pointer-level regressions for the row-anchored curve.
use super::*;
use crate::app::panels::curve_plot::two_actor_timeline;
use animatix::timeline::SceneDimensions;
use egui::{Event, Modifiers, PointerButton, RawInput, pos2};

#[test]
fn narrow_actor_disclosure_and_property_button_open_curve_below_owner_row() {
    let timeline = two_actor_timeline();
    let id = timeline.get_entity_id("a").unwrap();
    let lanes = collect_actor_lanes(&timeline, id);
    assert_eq!(lanes.first().unwrap().0, "position");

    let egui = egui::Context::default();
    let panel = egui::Id::unique("row_test");
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(260.0, 250.0));
    let coords = TimelineCoords {
        bar_origin_x: 78.0,
        bar_width: 182.0,
        scroll_s: 0.0,
        visible_s: 2.0,
    };
    let preview = PreviewPaneState::new(
        2.0,
        SceneDimensions {
            width: 640,
            height: 360,
        },
    );
    let mut expanded = HashSet::new();
    let mut collapsed = HashSet::new();
    let mut selected = HashSet::new();
    let mut keyframes = Vec::new();
    let mut commands = ActionQueue::new();
    let start = 32.0;
    let curve_height = 56.0;
    let mut toggle_pos = Pos2::ZERO;
    let mut curve_pos = Pos2::ZERO;
    let mut plain_bottom = 0.0;
    let mut last_bottom = 0.0;

    let events = |pos: Pos2, pressed| {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    };
    for step in 0..10 {
        let input = match step {
            1 => events(toggle_pos, true),
            2 => events(toggle_pos, false),
            4 => events(curve_pos, true),
            5 | 8 => events(curve_pos, false),
            7 => events(curve_pos, true),
            _ => vec![],
        };
        let mut output = egui.run_ui(
            RawInput {
                screen_rect: Some(screen),
                events: input,
                ..Default::default()
            },
            |ui| {
                let sp = crate::app::design_tokens::spatial::spatial(ui);
                let row_h = sp.timeline.track_row_height;
                toggle_pos = pos2(sp.base.space_2 + 11.0, start + row_h * 0.5);
                curve_pos = pos2(coords.bar_origin_x - 12.0, start + row_h * 1.5);
                let curve = curves::Control::new(ui, panel, None);
                if step == 6 {
                    assert_eq!(curve.owner(), Some(("a", "position")));
                }
                let mut rows = TimelineRowCtx {
                    timeline: Some(&timeline),
                    coords: &coords,
                    scroll_rect: screen,
                    bar_origin_x: coords.bar_origin_x,
                    playhead_x: coords.time_to_x(0.0),
                    scene_offset: 0.0,
                    active_scene: None,
                    scene_for_selection: None,
                    shift_held: false,
                    theme: eparts::theme(ui),
                    sp: &sp,
                    preview: &preview,
                    collapsed_actors: &mut collapsed,
                    expanded_properties: &mut expanded,
                    selected_actors: &mut selected,
                    selected_keyframes: &mut keyframes,
                    curve: &curve,
                    curve_height,
                    commands: &mut commands,
                };
                last_bottom = rows.render_actor_row(ui, "a", 0, 0, start);
                plain_bottom = start + row_h * (1 + lanes.len()) as f32;
            },
        );
        output.textures_delta.clear();
        if step >= 3 {
            assert!(expanded.contains("a"), "actor list click must expand, not select the label");
            assert!(
                selected.is_empty(),
                "disclosure and graph clicks never select the actor label"
            );
        }
        if step == 3 || step == 9 {
            assert_eq!(last_bottom, plain_bottom);
        } else if step == 6 {
            assert_eq!(last_bottom, plain_bottom + curve_height);
        }
    }
}
